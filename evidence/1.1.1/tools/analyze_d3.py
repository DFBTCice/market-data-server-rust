#!/usr/bin/env python3
"""D3/D4 分析：读取 evidence/1.1.1/d3-load/c*.csv 与 d4-prod-readonly/prod.csv，输出统计表与图。

用法：/usr/local/bin/python3 evidence/1.1.1/tools/analyze_d3.py [--warmup-min 60] [--out evidence/1.1.1/report]
  * 斜率 = 预热期之后的线性回归（MiB/h）与首末点差；
  * 图：RSS 随时间（旁路各容器 + 现网）、md_ws_subscriptions_active、tokio 存活 task、jemalloc allocated/resident，
        并在图上标注 OKX 重连（竖线）与 churn（来自 consumers 日志的周期数）。
"""
import csv, os, re, sys, glob, json
from datetime import datetime, timezone
import numpy as np

ROOT = os.path.join(os.path.dirname(__file__), "..")
D3 = os.path.join(ROOT, "d3-load")
D4 = os.path.join(ROOT, "d4-prod-readonly")
args = sys.argv[1:]
WARM = float(args[args.index("--warmup-min") + 1]) if "--warmup-min" in args else 60.0
OUT = args[args.index("--out") + 1] if "--out" in args else os.path.join(ROOT, "report")
os.makedirs(OUT, exist_ok=True)

LABEL = {
    "c0": "c0 v0 (1.1.0网关) 抖动+重连",
    "c1": "c1 v1 (1.1.1修复) 抖动+重连",
    "c2": "c2 v2 (1.1.0+jemalloc) 抖动+重连",
    "c3": "c3 v3 (1.1.1+jemalloc) 抖动+重连",
    "c4": "c4 v0 仅抖动",
    "c5": "c5 v0 仅重连",
    "c6": "c6 v0 安静基线",
    "c7": "c7 v1 ping15s(a)",
    "c8": "c8 v1 ping15s(b)",
    "c9": "c9 原版1.0.0 仅抖动",
    "c10": "c10 v3(1.1.1+jemalloc) 2核 +停读",
    "c11": "c11 v1(1.1.1) 2核 +停读",
    "c12": "c12 v0(1.1.0网关) 2核 抖动+重连",
    "c13": "c13 v1(1.1.1) 2核 抖动+重连",
    "c14": "c14 v4(1.1.1+写超时) 2核 +停读",
    "c15": "c15 v4(最终候选A1+A5) 2核 +注入",
    "c16": "c16 v1(无写超时,对照) 2核 +注入",
    "c17": "c17 v5(A5+jemalloc) 2核 +注入",
}


def ts(s):
    return datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc).timestamp()


def load(path):
    rows = list(csv.DictReader(open(path)))
    out = {}
    for k in rows[0].keys():
        col = []
        for r in rows:
            v = r[k]
            if k == "ts_utc":
                col.append(ts(v))
            else:
                try:
                    col.append(float(v))
                except ValueError:
                    col.append(float("nan"))
        out[k] = np.array(col)
    return out


def slope_per_h(t, y, warm_min):
    m = (t - t[0]) >= warm_min * 60
    if m.sum() < 5:
        return float("nan")
    p = np.polyfit((t[m] - t[m][0]) / 3600.0, y[m], 1)
    return p[0]


def events(path, kind):
    ev = []
    if os.path.exists(path):
        for l in open(path):
            if kind in l:
                try:
                    ev.append(datetime.strptime(l[:19], "%Y-%m-%dT%H:%M:%S").replace(tzinfo=timezone.utc).timestamp())
                except ValueError:
                    pass
    return ev


data = {}
def load_merged(path):
    """cN.csv + 续跑文件 cNb.csv（同一容器、采样进程到期后接着采样）按时间拼接。"""
    d = load(path)
    cont = path[:-4] + "b.csv"
    if os.path.exists(cont) and os.path.getsize(cont) > 0:
        try:
            d2 = load(cont)
        except IndexError:  # 只有表头（续跑刚启动）
            d2 = None
        if d2 is not None and len(d2["ts_utc"]) > 0:
            d = {k: np.concatenate([d[k], d2[k]]) for k in d}
    return d


for p in sorted(glob.glob(os.path.join(D3, "c*.csv")), key=lambda x: os.path.basename(x)):
    b = os.path.basename(p)
    if not re.fullmatch(r"c\d+\.csv", b):
        continue
    data[b[:-4]] = load_merged(p)
prod = load(os.path.join(D4, "prod.csv")) if os.path.exists(os.path.join(D4, "prod.csv")) else None

MIB = 2 ** 20
print(f"{'实例':<34} {'样本':>5} {'时长h':>6} {'RSS起':>7} {'RSS止':>7} {'斜率MiB/h(预热{:.0f}m后)':>22} {'ws_subs':>8} {'tasks':>6} {'okx重连':>8} {'drop':>5}".format(WARM))
for k, d in sorted(data.items(), key=lambda kv: int(kv[0][1:])):
    t = d["ts_utc"]; rss = d["rss_bytes"] / MIB
    rec = d["reconnect_okx"][-1] + d["reconnect_okx_kline"][-1] + d["reconnect_okx_quote"][-1]
    print(f"{LABEL[k]:<34} {len(t):>5} {(t[-1]-t[0])/3600:>6.2f} {rss[0]:>7.1f} {rss[-1]:>7.1f} {slope_per_h(t, rss, WARM):>22.2f} {d['ws_subscriptions_active'][-1]:>8.0f} {d['tokio_alive_tasks'][-1]:>6.0f} {rec:>8.0f} {d['ticks_dropped'][-1]:>5.0f}")
if prod is not None:
    t = prod["ts_utc"]; rss = prod["rss_bytes"] / MIB
    print(f"{'现网 md-server (只读)':<34} {len(t):>5} {(t[-1]-t[0])/3600:>6.2f} {rss[0]:>7.1f} {rss[-1]:>7.1f} {slope_per_h(t, rss, 0):>22.2f} {'-':>8} {'-':>6} {prod['reconnect_okx'][-1]+prod['reconnect_okx_kline'][-1]+prod['reconnect_okx_quote'][-1]:>8.0f} {prod['ticks_dropped'][-1]:>5.0f}")

for k in ("c2", "c3", "c10", "c17"):
    if k in data and not np.isnan(data[k]["jemalloc_allocated"][-1]):
        d = data[k]; t = d["ts_utc"]
        print(f"\n[{LABEL[k]}] jemalloc (MiB)  起→止 / 斜率MiB/h:")
        for name in ("allocated", "active", "resident", "retained", "mapped"):
            y = d[f"jemalloc_{name}"] / MIB
            print(f"   {name:<10} {y[0]:7.2f} → {y[-1]:7.2f}   slope={slope_per_h(t, y, WARM):6.2f}")

try:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    plt.rcParams["font.sans-serif"] = ["PingFang SC", "Heiti SC", "Arial Unicode MS", "DejaVu Sans"]
    plt.rcParams["axes.unicode_minus"] = False
except Exception as e:  # pragma: no cover
    print("matplotlib 不可用，跳过作图:", e)
    sys.exit(0)

t0 = min(d["ts_utc"][0] for d in data.values())
hrs = lambda t: (t - t0) / 3600.0
colors = {"c0": "#d62728", "c1": "#2ca02c", "c2": "#ff7f0e", "c3": "#17becf", "c4": "#9467bd", "c5": "#8c564b", "c6": "#7f7f7f", "c7": "#bcbd22", "c8": "#e377c2", "c9": "#000000", "c10": "#17becf", "c11": "#bcbd22", "c12": "#a50f15", "c13": "#08519c", "c14": "#238b45", "c15": "#006d2c", "c16": "#cb181d", "c17": "#6a3d9a"}


def decorate(ax, key="c0"):
    # 标注 c0 的 OKX 强制/自然重连事件
    for e in events(os.path.join(D3, f"{key}.events.log"), "RECONNECT"):
        ax.axvline(hrs(e), color="#cccccc", lw=0.6, zorder=0)
    ax.set_xlabel("小时（自旁路实验开始）")
    ax.grid(alpha=0.3)


fig, axes = plt.subplots(2, 2, figsize=(15, 9))
ax = axes[0, 0]
for k, d in sorted(data.items(), key=lambda kv: int(kv[0][1:])):
    ax.plot(hrs(d["ts_utc"]), d["rss_bytes"] / MIB, label=LABEL[k], color=colors[k], lw=1.3)
ax.set_title("旁路实验：RSS（MiB）；灰竖线=c0 的 OKX 重连"); ax.set_ylabel("MiB"); ax.legend(fontsize=7); decorate(ax)
ax = axes[0, 1]
for k in ("c0", "c1", "c4", "c5", "c6"):
    if k in data:
        ax.plot(hrs(data[k]["ts_utc"]), data[k]["ws_subscriptions_active"], label=LABEL[k], color=colors[k])
ax.set_title("md_ws_subscriptions_active（全部 WS 连接持有的接收端数）"); ax.legend(fontsize=7); decorate(ax)
ax = axes[1, 0]
for k in ("c2", "c3", "c10", "c17"):
    if k in data and not np.isnan(data[k]["jemalloc_allocated"][-1]):
        h = hrs(data[k]["ts_utc"])
        ax.plot(h, data[k]["jemalloc_allocated"] / MIB, label=f"{k} allocated", color=colors[k], ls="-")
        ax.plot(h, data[k]["jemalloc_resident"] / MIB, label=f"{k} resident", color=colors[k], ls="--")
ax.set_title("jemalloc：allocated(实线) vs resident(虚线)  MiB"); ax.legend(fontsize=7); decorate(ax)
ax = axes[1, 1]
for k in ("c0", "c1", "c5"):
    if k in data:
        ax.plot(hrs(data[k]["ts_utc"]), data[k]["tokio_alive_tasks"], label=LABEL[k], color=colors[k])
ax.set_title("tokio 存活 task 数（重连前后应回到同一水平）"); ax.legend(fontsize=7); decorate(ax)
plt.tight_layout(); plt.savefig(os.path.join(OUT, "d3-overview.png"), dpi=110); plt.close()

if prod is not None:
    import matplotlib.dates as mdates
    fig, (ax, axb) = plt.subplots(2, 1, figsize=(13, 7), sharex=True, gridspec_kw={"height_ratios": [3, 1.3]})
    X = lambda t: np.array([datetime.fromtimestamp(v, timezone.utc) for v in np.atleast_1d(t)])
    xs = X(prod["ts_utc"])
    ax.plot(xs, prod["rss_bytes"] / MIB, color="#d62728", lw=1.4, label="现网 md-server RSS（1.1.0，只读逐分钟采样）")
    ax.set_ylabel("RSS MiB"); ax.grid(alpha=0.3)
    evp = os.path.join(D4, "prod.events.log")
    kinds = [("RECONNECT", "#7f7f7f", "-", "OKX/币安 连接器重连"), ("TICKS_DROPPED", "#1f77b4", "--", "tick 入口队列丢弃（行情突发）"),
             ("WS_CLIENTS", "#2ca02c", ":", "WS 客户端数变化"), ("SUB+", "#ff7f0e", ":", "新增订阅")]
    for kind, col, ls, lab in kinds:
        first = True
        for e in events(evp, kind):
            for a_ in (ax, axb):
                a_.axvline(X(e)[0], color=col, lw=0.9, ls=ls, label=(lab if first and a_ is ax else None), alpha=0.8)
            first = False
    # 标注 RSS 单分钟台阶（>=2.5 MiB）
    r = prod["rss_bytes"] / MIB
    for i in range(1, len(r)):
        if r[i] - r[i - 1] >= 2.5:
            ax.annotate(f"+{r[i]-r[i-1]:.1f}", (xs[i], r[i]), textcoords="offset points", xytext=(0, 8), ha="center", fontsize=7, color="#d62728")
    ax.legend(loc="upper left", fontsize=8)
    ax.set_title("现网 RSS 逐分钟 + 事件对齐（UTC）；数字 = 单分钟 RSS 台阶 MiB")
    # 下图：每分钟处理量与发送量（突发/停读会在此体现）
    tp = np.diff(prod["ticks_processed"]); ws = np.diff(prod["ws_sent_tick"])
    axb.plot(xs[1:], tp, color="#9467bd", lw=0.8, label="tick 处理数/分钟"); axb.plot(xs[1:], ws, color="#17becf", lw=0.8, label="ws 推送 tick 数/分钟")
    axb.set_ylabel("条/分钟"); axb.grid(alpha=0.3); axb.legend(loc="upper left", fontsize=8)
    axb.xaxis.set_major_formatter(mdates.DateFormatter("%H:%M")); axb.set_xlabel("UTC 时间（2026-10-10；北京时间 = UTC+8）")
    plt.tight_layout(); plt.savefig(os.path.join(OUT, "d4-prod-rss.png"), dpi=110); plt.close()
print("charts ->", OUT)
