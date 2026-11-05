#!/usr/bin/env python3
"""每小时只读保存现网检查；不重启采样，不代替完整24小时验收。"""
import argparse
import csv
import datetime as dt
import json
import pathlib
import statistics
import urllib.request

from pt_exp import D, call, demux, guard

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = ROOT / "evidence/1.1.1/nas-198/post-swap-24h"


def stamp(value):
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()


def metric_stats(rows, key):
    values = [float(row[key]) / 2**20 for row in rows]
    times = [(stamp(row["ts_utc"]) - stamp(rows[0]["ts_utc"])) / 3600 for row in rows]
    mx, my = statistics.mean(times), statistics.mean(values)
    return {
        "first_mib": values[0], "last_mib": values[-1],
        "min_mib": min(values), "max_mib": max(values),
        "slope_mib_h": sum((x-mx)*(y-my) for x, y in zip(times, values))
        / sum((x-mx)**2 for x in times),
    }


def get(path):
    with urllib.request.urlopen("http://192.168.1.198:8081" + path, timeout=10) as response:
        return response.read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hour", required=True, help="UTC checkpoint folder, e.g. 20261024T0309Z")
    parser.add_argument("--since", required=True, help="UTC ISO timestamp for logs/events")
    args = parser.parse_args()
    assert len(args.hour) == 14 and args.hour.isalnum(), "invalid checkpoint folder"
    since = stamp(args.since)
    out = BASE / "hourly" / args.hour
    out.mkdir(parents=True, exist_ok=True)
    assert not (out / "check.json").exists(), "checkpoint already exists; refuse overwrite"
    expected = json.loads((BASE / "swap-result.json").read_text())
    guard()
    container = call("GET", D + "/containers/md-server/json")
    assert container["Id"] == expected["new_container_id"]
    assert container["Image"] == expected["new_image_id"]
    raw = call("GET", f"{D}/containers/md-server/logs?stdout=1&stderr=1&timestamps=1&since={int(since)}&tail=1000", want_raw=True)
    logs = raw.decode("utf8", "replace") if container["Config"].get("Tty") else demux(raw)
    (out / "server.log").write_text(logs)
    text = get("/metrics").decode()
    (out / "metrics.txt").write_text(text)
    metrics = {line.rsplit(" ", 1)[0]: float(line.rsplit(" ", 1)[1])
               for line in text.splitlines() if line and not line.startswith("#")}
    health = json.loads(get("/health"))
    subscriptions = json.loads(get("/api/v1/subscriptions"))
    (out / "subscriptions.json").write_text(json.dumps(subscriptions, indent=2) + "\n")
    oms = call("GET", D + "/containers/oms-m2f-service/json")
    with (BASE / "prod.csv").open() as source:
        rows = list(csv.DictReader(source))
    required = ("rss_bytes", "jemalloc_allocated", "uptime_s", "process_start_time_seconds",
                "ws_kicked_write_timeout", "ws_kicked_lagged", "ticks_dropped", "subs_total", "ws_clients")
    valid = [row for row in rows if all(row.get(key, "") != "" for key in required)]
    first = stamp(rows[0]["ts_utc"])
    warm = [row for row in valid if stamp(row["ts_utc"]) - first >= 3600]
    assert len(warm) >= 2, "insufficient post-warmup data"
    key = lambda sub: (sub["exchange"], sub["data_type"], sub["symbol"], sub.get("kline_interval", ""))
    baseline = json.loads((BASE / "subscriptions.before.json").read_text())
    current = {"rss_mib": metrics["process_resident_memory_bytes"] / 2**20,
               "allocated_mib": metrics["md_jemalloc_allocated_bytes"] / 2**20,
               "ws_clients": metrics["md_ws_active_clients"], "subscriptions": len(subscriptions),
               "missing_vs_swap": len(set(map(key, baseline)) - set(map(key, subscriptions))),
               "extra_vs_swap": len(set(map(key, subscriptions)) - set(map(key, baseline))),
               "write_timeouts": metrics["md_ws_kicked_write_timeout_total"],
               "lagged": metrics["md_ws_kicked_lagged_total"], "ticks_dropped": metrics["md_ticks_dropped"]}
    rss = metric_stats(warm, "rss_bytes")
    now = dt.datetime.now(dt.timezone.utc)
    result = {
        "at_utc": now.isoformat(), "phase": "running; not a complete 24h result",
        "healthy": container["State"].get("Health", {}).get("Status"), "health_response": health,
        "restart_count": container["RestartCount"], "container_id": container["Id"], "image_id": container["Image"],
        "sample_count": len(rows), "valid_samples": len(valid), "first_utc": rows[0]["ts_utc"], "last_utc": rows[-1]["ts_utc"],
        "span_s": stamp(rows[-1]["ts_utc"]) - first,
        "max_gap_s": max(stamp(b["ts_utc"]) - stamp(a["ts_utc"]) for a, b in zip(valid, valid[1:])),
        "sample_age_s": now.timestamp() - stamp(rows[-1]["ts_utc"]),
        "process_starts": sorted(set(row["process_start_time_seconds"] for row in valid)),
        "uptime_resets": sum(float(b["uptime_s"]) < float(a["uptime_s"]) for a, b in zip(valid, valid[1:])),
        "post_warmup_span_s": stamp(warm[-1]["ts_utc"]) - stamp(warm[0]["ts_utc"]),
        "rss_post_warmup": rss, "allocated_post_warmup": metric_stats(warm, "jemalloc_allocated"),
        "rss_last_60min": metric_stats([row for row in warm if stamp(row["ts_utc"]) >= stamp(warm[-1]["ts_utc"]) - 3600], "rss_bytes"),
        "current": current,
        "write_timeout_sample_max": max(float(row["ws_kicked_write_timeout"]) for row in valid),
        "lagged_sample_max": max(float(row["ws_kicked_lagged"]) for row in valid),
        "tick_drop_sample_max": max(float(row["ticks_dropped"]) for row in valid),
        "oms_readonly_health": {"health": oms["State"].get("Health", {}).get("Status"), "running": oms["State"]["Running"],
                                "started_at": oms["State"]["StartedAt"], "restart_count": oms["RestartCount"]},
        "analysis_is_provisional": True, "provisional_rss_target_exceeded": rss["slope_mib_h"] > 1,
        "server_log_collection": {"since_epoch": int(since), "raw_bytes": len(raw), "text_bytes": len(logs.encode()), "level": "RUST_LOG=warn", "tail_limit": 1000},
        "new_events": [line for line in (BASE / "prod.events.log").read_text().splitlines() if stamp(line.split()[0]) >= since],
    }
    (out / "check.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ("at_utc", "healthy", "restart_count", "sample_count", "valid_samples", "span_s", "max_gap_s", "process_starts", "uptime_resets", "rss_post_warmup", "allocated_post_warmup", "current", "new_events")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
