#!/usr/bin/env python3
"""D6（重连后无孤儿 task）与 D7（OKX 重连频率 / ping 间隔 A/B）分析。

输入：evidence/1.1.1/d3-load/cN.csv（+续跑 cNb.csv）、cN.events.log、d4-prod-readonly/prod.csv
输出：stdout（同时由调用方重定向到 evidence/1.1.1/report/d6-d7.txt）

D6 判据：对每个 RECONNECT 事件，取事件前 2 分钟的 tokio_alive_tasks 为 before，事件后 5~8 分钟中
        的众数/最后值为 after；after == before 视为"无孤儿 task"。同时输出整个运行期的最小/最大/末值。
D7：未启用强制重连的容器（c4/c6 ping=25s；c7/c8 ping=15s）按「每连接器小时」折算自然重连率，并与现网比较。
"""
import csv, os, re, sys
from datetime import datetime, timezone
from collections import Counter

ROOT = os.path.join(os.path.dirname(__file__), "..")
D3 = os.path.join(ROOT, "d3-load")
D4 = os.path.join(ROOT, "d4-prod-readonly")


def ts(s):
    return datetime.strptime(s[:19], "%Y-%m-%dT%H:%M:%S").replace(tzinfo=timezone.utc).timestamp()


def read_csv(name):
    rows = []
    for p in (os.path.join(D3, f"{name}.csv"), os.path.join(D3, f"{name}b.csv")):
        if os.path.exists(p) and os.path.getsize(p) > 0:
            rows += list(csv.DictReader(open(p)))
    return rows


def fnum(x):
    try:
        return float(x)
    except (TypeError, ValueError):
        return float("nan")


def d6(name):
    rows = read_csv(name)
    if not rows:
        return None
    t = [ts(r["ts_utc"]) for r in rows]
    tasks = [fnum(r["tokio_alive_tasks"]) for r in rows]
    evp = os.path.join(D3, f"{name}.events.log")
    evs = sorted({ts(l[:19]) for l in open(evp) if " RECONNECT " in l}) if os.path.exists(evp) else []
    res = []
    for e in evs:
        before = [tasks[i] for i in range(len(t)) if e - 150 <= t[i] <= e - 60]
        dip = [tasks[i] for i in range(len(t)) if e - 30 <= t[i] <= e + 180]
        after = [tasks[i] for i in range(len(t)) if e + 300 <= t[i] <= e + 480]
        if not before or not after:
            continue
        res.append((e, before[-1], min(dip) if dip else float("nan"), Counter(after).most_common(1)[0][0]))
    ok = sum(1 for r in res if r[1] == r[3])
    return {
        "events": len(evs), "judged": len(res), "ok": ok,
        "min": min(x for x in tasks if x == x), "max": max(x for x in tasks if x == x), "last": tasks[-1],
        "dip_min": min((r[2] for r in res), default=float("nan")),
        "bad": [(datetime.fromtimestamp(r[0], timezone.utc).strftime("%H:%M:%S"), r[1], r[3]) for r in res if r[1] != r[3]],
    }


print("== D6：重连前后 tokio 存活 task 数（after==before 即无孤儿 task）==")
print(f"{'容器':<6} {'RECONNECT事件':>12} {'可判定':>6} {'一致':>5} {'全程min':>8} {'max':>5} {'末值':>5} {'重连瞬间最低':>10}  不一致明细")
for n in ["c0", "c1", "c2", "c3", "c5", "c10", "c11", "c12", "c13", "c14", "c15", "c16", "c17"]:
    r = d6(n)
    if r:
        print(f"{n:<6} {r['events']:>12} {r['judged']:>6} {r['ok']:>5} {r['min']:>8.0f} {r['max']:>5.0f} {r['last']:>5.0f} {r['dip_min']:>10.0f}  {r['bad'][:4]}")

print("\n== D7：未强制重连的容器，OKX/币安 自然重连频率 ==")
CONN = ["reconnect_okx", "reconnect_okx_kline", "reconnect_okx_quote", "reconnect_binance", "reconnect_binance_quote"]
print(f"{'实例':<26} {'运行h':>6} " + " ".join(f"{c.replace('reconnect_', ''):>14}" for c in CONN) + f" {'合计':>5} {'次/连接器·h':>12}")


def line(label, rows):
    if not rows:
        return
    h = (ts(rows[-1]["ts_utc"]) - ts(rows[0]["ts_utc"])) / 3600
    vals = [fnum(rows[-1][c]) - fnum(rows[0][c]) for c in CONN]
    tot = sum(vals)
    print(f"{label:<26} {h:>6.2f} " + " ".join(f"{v:>14.0f}" for v in vals) + f" {tot:>5.0f} {tot / (h * len(CONN)):>12.2f}")


for n, lab in [("c4", "c4 ping=25s"), ("c6", "c6 ping=25s(静默)"), ("c7", "c7 ping=15s"), ("c8", "c8 ping=15s")]:
    line(lab, read_csv(n))
pp = os.path.join(D4, "prod.csv")
if os.path.exists(pp):
    line("现网 1.1.0 (ping=25s)", list(csv.DictReader(open(pp))))
    rows = list(csv.DictReader(open(pp)))
    # 现网计数器是进程自启动累计（uptime≈21.8h 起算）
    h = fnum(rows[-1]["uptime_s"]) / 3600
    tot = sum(fnum(rows[-1][c]) for c in CONN)
    print(f"{'现网(进程启动至今累计)':<26} {h:>6.2f} " + " ".join(f"{fnum(rows[-1][c]):>14.0f}" for c in CONN) + f" {tot:>5.0f} {tot / (h * len(CONN)):>12.2f}")

# 自然重连间隔
print("\n== D7：自然重连的时间点（容器内每连接器独立发生；ping=25s 与 15s 无差别）==")
for n in ["c4", "c6", "c7", "c8"]:
    evp = os.path.join(D3, f"{n}.events.log")
    if not os.path.exists(evp):
        continue
    evs = [l.strip() for l in open(evp) if " RECONNECT " in l]
    print(f"[{n}] {len(evs)} 次：" + "; ".join(re.sub(r"^\S+T(\d\d:\d\d:\d\d)\S* RECONNECT md_connector_reconnect_total\{exchange=\"([^\"]+)\"\} (\S+)", r"\1 \2 \3", e) for e in evs))
