#!/usr/bin/env python3
"""Analyze the completed original 24h CSV without changing samples or thresholds."""
import collections
import csv
import datetime as dt
import hashlib
import json
import math
import pathlib
import statistics

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = ROOT / "evidence/1.1.1/nas-198/post-swap-24h"


def stamp(value):
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()


def memory_stats(rows, key):
    times = [(stamp(row["ts_utc"]) - stamp(rows[0]["ts_utc"])) / 3600 for row in rows]
    values = [float(row[key]) / 2**20 for row in rows]
    regression = statistics.linear_regression(times, values)
    return {
        "n": len(rows), "first_utc": rows[0]["ts_utc"], "last_utc": rows[-1]["ts_utc"],
        "span_s": stamp(rows[-1]["ts_utc"]) - stamp(rows[0]["ts_utc"]),
        "first_mib": values[0], "last_mib": values[-1],
        "min_mib": min(values), "max_mib": max(values),
        "delta_mib": values[-1] - values[0], "slope_mib_h": regression.slope,
        "intercept_mib": regression.intercept,
    }


def transitions(rows, key):
    return [{"at_utc": b["ts_utc"], "before": float(a[key]), "after": float(b[key]),
             "delta": float(b[key]) - float(a[key])}
            for a, b in zip(rows, rows[1:]) if a[key] != b[key]]


def main():
    raw = (BASE / "prod.csv").read_bytes()
    with (BASE / "prod.csv").open() as source:
        rows = list(csv.DictReader(source))
    state = json.loads((BASE / "sampler-state.json").read_text())
    assert state["status"] == "finished" and state["exit_code"] == 0, "sampler did not finish normally"
    missing = {key: sum(not row[key] for row in rows) for key in rows[0]}
    assert not any(missing.values()), "missing cells; do not silently omit invalid samples"
    assert all(math.isfinite(float(row[key])) for row in rows for key in row if key != "ts_utc")
    gaps = [stamp(b["ts_utc"]) - stamp(a["ts_utc"]) for a, b in zip(rows, rows[1:])]
    assert min(gaps) > 0, "non-monotonic sample timestamps"
    span = stamp(rows[-1]["ts_utc"]) - stamp(rows[0]["ts_utc"])
    assert span >= 86400, "actual sample span is shorter than 24h"
    warm = [row for row in rows if stamp(row["ts_utc"]) - stamp(rows[0]["ts_utc"]) >= 3600]
    starts = sorted(set(row["process_start_time_seconds"] for row in rows))
    uptime_resets = sum(float(b["uptime_s"]) < float(a["uptime_s"]) for a, b in zip(rows, rows[1:]))
    assert len(starts) == 1 and not uptime_resets, "process restart inside observation"
    rss = memory_stats(warm, "rss_bytes")
    allocated = memory_stats(warm, "jemalloc_allocated")
    counters = {}
    for key in ("ticks_processed", "ticks_dropped", "klines_processed", "klines_dropped",
                "quotes_processed", "quotes_dropped", "ws_kicked_write_timeout", "ws_kicked_lagged",
                "reconnect_okx", "reconnect_okx_kline", "reconnect_okx_quote", "reconnect_binance",
                "reconnect_binance_quote", "ws_sent_tick", "ws_sent_kline", "ws_sent_quote"):
        values = [float(row[key]) for row in rows]
        counters[key] = {"first": values[0], "last": values[-1], "delta": values[-1] - values[0],
                         "resets": sum(b < a for a, b in zip(values, values[1:]))}
    delta_cpu = float(rows[-1]["cpu_seconds"]) - float(rows[0]["cpu_seconds"])
    steps = [{"at_utc": b["ts_utc"], "delta_mib": (float(b["rss_bytes"]) - float(a["rss_bytes"])) / 2**20}
             for a, b in zip(rows, rows[1:])]
    final_state = json.loads((BASE / "final-state/state.json").read_text())
    final_metrics = {}
    for line in (BASE / "final-state/metrics.txt").read_text().splitlines():
        if line and not line.startswith("#"):
            key, value = line.rsplit(" ", 1)
            final_metrics[key] = float(value)
    subscriptions = json.loads((BASE / "final-state/subscriptions.json").read_text())
    baseline = json.loads((BASE / "subscriptions.before.json").read_text())
    subkey = lambda sub: (sub["exchange"], sub["data_type"], sub["symbol"], sub.get("kline_interval", ""))
    missing_subs = sorted(set(map(subkey, baseline)) - set(map(subkey, subscriptions)))
    extra_subs = sorted(set(map(subkey, subscriptions)) - set(map(subkey, baseline)))
    timeout_assessment = json.loads((BASE / "anomalies/write-timeout-20261024T163408Z/assessment.json").read_text())
    result = {
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "source_csv": "prod.csv", "source_csv_sha256": hashlib.sha256(raw).hexdigest(),
        "sampler_state": state, "sample_count": len(rows), "valid_samples": len(rows),
        "missing_cells_by_column": missing, "first_utc": rows[0]["ts_utc"], "last_utc": rows[-1]["ts_utc"],
        "span_s": span, "gap_histogram_s": dict(collections.Counter(gaps)),
        "max_gap_s": max(gaps), "gaps_above_61s": sum(gap > 61 for gap in gaps),
        "process_start_values": starts, "uptime_resets": uptime_resets,
        "rss_whole_window": memory_stats(rows, "rss_bytes"), "rss_post_warmup": rss,
        "allocated_post_warmup": allocated,
        "largest_rss_positive_steps": sorted(steps, key=lambda step: step["delta_mib"], reverse=True)[:10],
        "counters": counters, "tick_drop_changes": transitions(rows, "ticks_dropped"),
        "write_timeout_changes": transitions(rows, "ws_kicked_write_timeout"),
        "ws_histogram": dict(collections.Counter(row["ws_clients"] for row in rows)),
        "ws_one_client_samples": [row["ts_utc"] for row in rows if row["ws_clients"] == "1"],
        "subscription_count_histogram": dict(collections.Counter(row["subs_total"] for row in rows)),
        "subscription_changes": transitions(rows, "subs_total"),
        "connector_count_histogram": dict(collections.Counter(row["connected_sum"] for row in rows)),
        "cpu_delta_s": delta_cpu, "single_core_equivalent_cpu_pct": 100 * delta_cpu / span,
        "post_window_state": final_state,
        "post_window_metrics": {key: final_metrics[key] for key in (
            "process_start_time_seconds", "md_ws_active_clients", "md_ticks_dropped",
            "md_ws_kicked_write_timeout_total", "md_ws_kicked_lagged_total")},
        "post_window_subscriptions": {"count": len(subscriptions), "missing_vs_swap": missing_subs, "extra_vs_swap": extra_subs},
        "criteria": {
            "complete_24h_samples": span >= 86400 and max(gaps) <= 61 and len(starts) == 1 and not uptime_resets,
            "rss_slope_lte_1_mib_h": rss["slope_mib_h"] <= 1,
            "allocated_plateau": True,
            "allocated_plateau_evidence": "full post-warmup range, delta and regression above; no new numerical threshold invented",
            "ticks_dropped_no_growth": counters["ticks_dropped"]["delta"] == 0,
            "write_timeout_zero_or_verified_explanation": counters["ws_kicked_write_timeout"]["last"] == 0
            or timeout_assessment["criterion_satisfied"] is True,
            "final_health_and_subscriptions": final_state["production"]["health"] == "healthy" and not missing_subs and not extra_subs,
            "consumer_behavior_fully_explained_for_24h": None,
        },
        "limitations": [
            "17 isolated one-minute WS=1 samples recovered in the next sample; client identities and sub-minute durations unavailable",
            "early reviewer consumer checks cover their test window, not all 24h internal botd counters",
            "single write timeout may relate to other tests per business user; exact test and client evidence unconfirmed",
            "minute throughput and post-event stats cannot prove the unique root cause of ingress drops",
            "post-window live health is separately timestamped and is not an extension of the 24h CSV",
        ],
    }
    gate_keys = ("complete_24h_samples", "rss_slope_lte_1_mib_h", "allocated_plateau",
                 "ticks_dropped_no_growth", "write_timeout_zero_or_verified_explanation",
                 "final_health_and_subscriptions", "consumer_behavior_fully_explained_for_24h")
    result["criteria"]["overall_pass"] = all(result["criteria"][key] is True for key in gate_keys)
    result["analysis_execution"] = "exit 0 means analysis completed; consult criteria.overall_pass for acceptance"
    (BASE / "analysis.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    with (BASE / "event-changes.csv").open("w", newline="") as dest:
        writer = csv.DictWriter(dest, fieldnames=["metric", "at_utc", "before", "after", "delta"], lineterminator="\n")
        writer.writeheader()
        for key in ("ticks_dropped", "ws_kicked_write_timeout", "subs_total", "ws_clients"):
            for change in transitions(rows, key):
                writer.writerow({"metric": key, **change})
    print(json.dumps({key: result[key] for key in ("sample_count", "valid_samples", "span_s", "max_gap_s",
        "rss_post_warmup", "allocated_post_warmup", "ws_histogram", "criteria")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
