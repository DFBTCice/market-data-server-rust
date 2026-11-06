#!/usr/bin/env python3
"""24 小时采样分析：覆盖率与采样缺口、进程是否重启、换版后前 10 分钟消费者重连情况、计数器增量。

用法：python3 evidence/1.2.0/tools/analyze_24h.py [prod.csv] [换版完成时间 ISO，默认取 CSV 第一行]
输出：同目录 analysis.json，并打印摘要。缺口 = 相邻两个成功采样点间隔超过 2 个采样周期，或采样行全空（请求失败）。
"""
import csv
import json
import pathlib
import sys
from datetime import datetime, timedelta

ROOT = pathlib.Path(__file__).resolve().parents[3]
CSV = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "evidence/1.2.0/nas-198/post-swap-24h/prod.csv"
INTERVAL = 60
COUNTERS = [
    "ws_kicked_write_timeout", "ws_kicked_lagged", "ticks_dropped", "klines_dropped", "quotes_dropped",
    "reconnect_okx", "reconnect_okx_kline", "reconnect_okx_quote", "reconnect_binance", "reconnect_binance_quote",
    "rt_idle_unsubscribed", "rt_demand_subscribed", "rt_demand_failed", "rt_limit_rejected",
]


def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def num(v):
    return float(v) if v not in ("", None) else None


rows = list(csv.DictReader(CSV.open()))
assert rows, f"{CSV} 没有采样行"
ok = [r for r in rows if r.get("rss_bytes")]
failed = [r["ts_utc"] for r in rows if not r.get("rss_bytes")]
swap_at = ts(sys.argv[2]) if len(sys.argv) > 2 else ts(rows[0]["ts_utc"])
first, last = ts(rows[0]["ts_utc"]), ts(rows[-1]["ts_utc"])

gaps = []
for a, b in zip(ok, ok[1:]):
    d = (ts(b["ts_utc"]) - ts(a["ts_utc"])).total_seconds()
    if d > 2 * INTERVAL:
        gaps.append({"from": a["ts_utc"], "to": b["ts_utc"], "seconds": int(d), "missing_samples": int(d // INTERVAL) - 1})

starts = sorted({r["process_start_time_seconds"] for r in ok if r.get("process_start_time_seconds")})
first10 = [r for r in ok if swap_at <= ts(r["ts_utc"]) < swap_at + timedelta(minutes=10)]
clients = [int(num(r["ws_clients"])) for r in first10 if r.get("ws_clients")]


def delta(col):
    vals = [num(r.get(col)) for r in ok if r.get(col)]
    return None if not vals else vals[-1] - vals[0]


rss = [num(r["rss_bytes"]) for r in ok]
result = {
    "csv": str(CSV.relative_to(ROOT)) if CSV.is_relative_to(ROOT) else str(CSV),
    "first_sample_utc": rows[0]["ts_utc"],
    "last_sample_utc": rows[-1]["ts_utc"],
    "span_hours": round((last - first).total_seconds() / 3600, 2),
    "expected_samples": int((last - first).total_seconds() // INTERVAL) + 1,
    "rows": len(rows),
    "ok_samples": len(ok),
    "failed_samples": failed,
    "gaps": gaps,
    "coverage_pct": round(100 * len(ok) / (int((last - first).total_seconds() // INTERVAL) + 1), 2),
    "process_start_times": starts,
    "restarted_during_window": len(starts) > 1,
    "first_10_min_ws_clients": clients,
    "first_10_min_ws_clients_min": min(clients) if clients else None,
    "first_10_min_ws_clients_changes": sum(1 for a, b in zip(clients, clients[1:]) if a != b),
    "rss_mib": {"first": round(rss[0] / 2**20, 1), "max": round(max(rss) / 2**20, 1), "last": round(rss[-1] / 2**20, 1)},
    "counter_deltas": {c: delta(c) for c in COUNTERS},
    "rt_subscriptions_last": num(ok[-1].get("rt_subscriptions")),
}
out = CSV.parent / "analysis.json"
out.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n")
print(json.dumps({k: result[k] for k in (
    "span_hours", "coverage_pct", "gaps", "failed_samples", "restarted_during_window",
    "first_10_min_ws_clients", "first_10_min_ws_clients_min", "first_10_min_ws_clients_changes", "rss_mib")},
    indent=1, ensure_ascii=False))
print(f"written {out}")
