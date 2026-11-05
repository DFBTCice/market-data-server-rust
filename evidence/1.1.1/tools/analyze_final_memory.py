#!/usr/bin/env python3
"""用实际采样跨度、连续性、预热后 OLS 与旧版正对照审核最终产物；未完成绝不写 PASS。"""
import csv
import datetime
import json
import pathlib
import statistics

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "evidence/1.1.1/final-artifact/memory-2h"
MIB = 1024 ** 2


def analyze(side):
    rows = list(csv.DictReader((OUT / f"{side}.csv").open()))
    stamp = lambda row: datetime.datetime.fromisoformat(row["ts_utc"].replace("Z", "+00:00")).timestamp()
    valid = [r for r in rows if r["rss_bytes"] and r["uptime_s"]]
    assert len(valid) >= 2, "尚无足够样本"
    start = stamp(valid[0])
    duration = stamp(valid[-1]) - start
    warm = [r for r in valid if stamp(r) - start >= 3600]
    assert len(warm) >= 2, "尚未完成 1h 预热"
    times = [(stamp(r) - start) / 3600 for r in warm]
    def metric(key):
        values = [float(r[key]) / MIB for r in warm]
        xmean, ymean = statistics.mean(times), statistics.mean(values)
        slope = sum((x-xmean)*(y-ymean) for x, y in zip(times, values)) / sum((x-xmean)**2 for x in times)
        return {"first_mib": values[0], "last_mib": values[-1], "min_mib": min(values),
                "max_mib": max(values), "delta_mib": values[-1] - values[0], "slope_mib_h": slope}
    result = {"samples": len(rows), "valid_samples": len(valid), "duration_s": duration,
              "post_warmup_samples": len(warm), "post_warmup_duration_s": (times[-1]-times[0])*3600,
              "max_gap_s": max(stamp(b)-stamp(a) for a, b in zip(valid, valid[1:])),
              "uptime_resets": sum(float(b["uptime_s"]) < float(a["uptime_s"]) for a, b in zip(valid, valid[1:])),
              "rss": metric("rss_bytes")}
    if side == "new":
        assert all(r["jemalloc_allocated"] for r in warm), "缺 jemalloc 指标"
        result["allocated"] = metric("jemalloc_allocated")
    return result


new, old = analyze("new"), analyze("old")
checks = {"finished_marker": (OUT / "finished-at.txt").exists(),
          "span_at_least_2h": all(x["duration_s"] >= 7200 for x in (new, old)),
          "warmup_1h_then_measure_1h": all(x["post_warmup_duration_s"] >= 3600 for x in (new, old)),
          "complete_samples": all(x["valid_samples"] == x["samples"] and x["max_gap_s"] <= 120 for x in (new, old)),
          "no_restarts": all(x["uptime_resets"] == 0 for x in (new, old)),
          "rss_slope_le_1_mib_h": new["rss"]["slope_mib_h"] <= 1,
          # “持平”的保守操作口径，原始值与回归一起交付，不能用短时低点代替趋势。
          "allocated_plateau": new["allocated"]["slope_mib_h"] <= 0.25 and new["allocated"]["delta_mib"] <= 1,
          "old_positive_control_growth": old["rss"]["slope_mib_h"] > 0 and old["rss"]["delta_mib"] >= 0.5}
result = {"new": new, "old": old, "checks": checks, "pass": all(checks.values())}
(OUT / "analysis.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
raise SystemExit(0 if result["pass"] else 1)
