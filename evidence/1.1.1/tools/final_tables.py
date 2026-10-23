#!/usr/bin/env python3
"""生成调查报告用的最终表格（markdown）：evidence/1.1.1/report/d3-table.md

每个旁路容器一行：版本/分配器/CPU/负载 → 负载期时长、RSS 起止、预热 60 分钟后的线性斜率、
负载撤掉后（idle，0 个 WS 客户端）的 RSS 变化。
用法：/usr/local/bin/python3 evidence/1.1.1/tools/final_tables.py
"""
import csv, os
from datetime import datetime, timezone
import numpy as np

ROOT = os.path.join(os.path.dirname(__file__), "..")
D3 = os.path.join(ROOT, "d3-load")
OUT = os.path.join(ROOT, "report")
os.makedirs(OUT, exist_ok=True)
MIB = 2 ** 20

# name: (版本, 分配器, CPU, 负载说明)
META = {
    "c0": ("v0 = 1.1.0 网关逻辑 + 仪表", "musl", 1, "抖动+强制重连(OKX/20min)"),
    "c1": ("v1 = 1.1.1 修复(A1/A2)", "musl", 1, "抖动+强制重连"),
    "c2": ("v2 = 1.1.0 + jemalloc", "jemalloc", 1, "抖动+强制重连"),
    "c3": ("v3 = 1.1.1(A1/A2) + jemalloc", "jemalloc", 1, "抖动+强制重连"),
    "c4": ("v0", "musl", 1, "仅抖动"),
    "c5": ("v0", "musl", 1, "仅强制重连"),
    "c6": ("v0", "musl", 1, "无抖动无重连（安静基线）"),
    "c7": ("v1, ping=15s", "musl", 1, "仅抖动"),
    "c8": ("v1, ping=15s", "musl", 1, "仅抖动"),
    "c9": ("1.0.0 原版二进制", "musl", 1, "仅抖动"),
    "c10": ("v3 (A1/A2+jemalloc)", "jemalloc", 2, "抖动+重连+30min 停读"),
    "c11": ("v1 (A1/A2)", "musl", 2, "抖动+重连+30min 停读"),
    "c12": ("v0 = 1.1.0 网关", "musl", 2, "抖动+强制重连"),
    "c13": ("v1 (A1/A2)", "musl", 2, "抖动+强制重连"),
    "c14": ("v4 = A1/A2/A5", "musl", 2, "抖动+重连+停读(A5 会踢)"),
    "c15": ("v4 = A1/A2/A5（musl 候选）", "musl", 2, "抖动+重连+每20min停读60s"),
    "c16": ("v1 (无 A5，对照)", "musl", 2, "抖动+重连+每20min停读60s"),
    "c17": ("v5 = A1/A2/A5 + jemalloc（推荐候选）", "jemalloc", 2, "抖动+重连+每20min停读60s"),
}


def ts(s):
    return datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc).timestamp()


def rows_of(*paths):
    out = []
    for p in paths:
        if os.path.exists(p) and os.path.getsize(p) > 0:
            out += list(csv.DictReader(open(p)))
    return out


def series(rows):
    t = np.array([ts(r["ts_utc"]) for r in rows])
    rss = np.array([float(r["rss_bytes"]) / MIB for r in rows])
    return t, rss


def slope(t, y, warm_min=60):
    m = (t - t[0]) >= warm_min * 60
    if m.sum() < 5:
        return float("nan")
    return np.polyfit((t[m] - t[m][0]) / 3600.0, y[m], 1)[0]


lines = [
    "| 实例 | 版本 | 分配器 | CPU | 负载 | 负载期 h | RSS 起→止 MiB | 峰值 MiB | 预热 60 分钟后斜率 MiB/h | 撤掉消费者后 RSS 变化（idle 1.5h） |",
    "|---|---|---|---|---|---|---|---|---|---|",
]
for n in range(18):
    c = f"c{n}"
    rows = rows_of(os.path.join(D3, f"{c}.csv"), os.path.join(D3, f"{c}b.csv"))
    if not rows:
        continue
    t, rss = series(rows)
    idle = rows_of(os.path.join(D3, "idle", f"{c}.idle.csv"))
    if idle:
        _, ir = series(idle)
        idle_s = f"{ir[0]:.1f} → {ir[-1]:.1f}（{ir[-1] - ir[0]:+.1f}）"
    else:
        idle_s = "—"
    v, a, cpu, load = META[c]
    sl = slope(t, rss)
    lines.append(
        f"| {c} | {v} | {a} | {cpu} | {load} | {(t[-1] - t[0]) / 3600:.2f} | {rss[0]:.1f} → {rss[-1]:.1f} | {rss.max():.1f} | "
        + (f"{sl:+.2f}" if sl == sl else "n/a")
        + f" | {idle_s} |"
    )

# 现网
pp = os.path.join(ROOT, "d4-prod-readonly", "prod.csv")
if os.path.exists(pp):
    rows = list(csv.DictReader(open(pp)))
    t, rss = series(rows)
    lines.append(
        f"| 现网 md-server | 1.1.0 | musl | 2 | 真实（39 订阅，2 个消费者） | {(t[-1] - t[0]) / 3600:.2f}（只读采样） | {rss[0]:.1f} → {rss[-1]:.1f} | {rss.max():.1f} | {slope(t, rss, 0):+.2f} | — |"
    )
open(os.path.join(OUT, "d3-table.md"), "w").write("\n".join(lines) + "\n")
print("\n".join(lines))
