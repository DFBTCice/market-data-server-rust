#!/usr/bin/env python3
"""汇总 canary_198.py 的 docker-stats.csv（每 10 秒一次）：各旁路容器的样本数、内存（usage / anon）与 CPU% 的首末、均值、最大值。
用法：python3 evidence/1.2.0/tools/summarize_canary_stats.py <canary 输出目录>...（结果写入各目录的 stats-summary.json 并打印）"""
import csv
import json
import pathlib
import statistics
import sys

for d in map(pathlib.Path, sys.argv[1:]):
    rows = list(csv.DictReader(open(d / "docker-stats.csv")))
    out = {}
    for side in dict.fromkeys(r["side"] for r in rows):
        r = [x for x in rows if x["side"] == side]
        num = lambda k: [float(x[k]) for x in r if x[k] not in ("", None)]
        mem, anon, cpu = num("mem_usage_mib"), num("mem_anon_mib"), num("cpu_pct")
        out[side] = {"samples": len(r), "from": r[0]["at_utc"], "to": r[-1]["at_utc"],
                     "mem_usage_mib": {"first": mem[0], "last": mem[-1], "mean": round(statistics.mean(mem), 1), "max": max(mem)},
                     "mem_anon_mib": {"first": anon[0], "last": anon[-1], "mean": round(statistics.mean(anon), 1), "max": max(anon)},
                     "cpu_pct_of_one_core_x_cpus": {"mean": round(statistics.mean(cpu), 2), "max": max(cpu)} if cpu else None}
    (d / "stats-summary.json").write_text(json.dumps(out, indent=2) + "\n")
    print(d.name, json.dumps(out, indent=1))
