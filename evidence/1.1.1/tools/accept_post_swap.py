#!/usr/bin/env python3
"""换版后十分钟只读验收；不提前通过，不替代完整24h。失败输出证据供立即回滚。"""
import csv
import datetime
import json
import pathlib
import time
import urllib.request

from pt_exp import D, call, guard

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "evidence/1.1.1/nas-198/post-swap-24h"
result = json.loads((OUT / "swap-result.json").read_text())
deadline = datetime.datetime.fromisoformat(result["checked_at_utc"]).timestamp() + 600
expected_id = result["new_container_id"]
guard()


def get(path):
    return urllib.request.urlopen("http://192.168.1.198:8081" + path, timeout=10).read()


def metrics(text):
    return {line.rsplit(" ", 1)[0]: float(line.rsplit(" ", 1)[1]) for line in text.splitlines()
            if line and not line.startswith("#")}


checks = 0
low_clients_since = None
while True:
    now = time.time()
    c = call("GET", f"{D}/containers/md-server/json")
    assert c["Id"] == expected_id and c["State"]["Health"]["Status"] == "healthy" and c["RestartCount"] == 0
    assert json.loads(get("/health"))["status"] == "ok"
    m = metrics(get("/metrics").decode())
    assert m["md_ws_kicked_write_timeout_total"] == 0 and m["md_ticks_dropped"] == 0
    assert m["process_resident_memory_bytes"] < 400 * 1024 ** 2
    low_clients_since = (low_clients_since or now) if m["md_ws_active_clients"] < 2 else None
    assert low_clients_since is None or now-low_clients_since < 30, "业务消费者持续少于2"
    with (OUT / "acceptance-checks.jsonl").open("a") as log:
        log.write(json.dumps({"at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "ws_clients": m["md_ws_active_clients"], "rss_bytes": m["process_resident_memory_bytes"],
            "ticks_dropped": m["md_ticks_dropped"], "write_timeouts": m["md_ws_kicked_write_timeout_total"],
            "uptime_s": m["process_uptime_seconds"]}) + "\n")
    checks += 1
    if now >= deadline:
        break
    time.sleep(min(10, deadline-now))
assert json.loads((OUT / "compat/result.json").read_text())["pass"]
assert (OUT / "compat/capture-finished.json").exists() and m["md_ws_active_clients"] == 2
before = json.loads((OUT / "subscriptions.before.json").read_text())
after_raw = get("/api/v1/subscriptions")
after = json.loads(after_raw)
key = lambda s: (s["exchange"], s["data_type"], s["symbol"], s.get("kline_interval", ""))
assert set(map(key, before)) == set(map(key, after))
(OUT / "subscriptions.after-10min.json").write_bytes(after_raw)
rows = list(csv.DictReader((OUT / "prod.csv").open()))
assert len(rows) >= 9 and all(r["rss_bytes"] and r["ws_kicked_write_timeout"] == "0" and r["ticks_dropped"] == "0" for r in rows)
times = [datetime.datetime.fromisoformat(r["ts_utc"].replace("Z", "+00:00")).timestamp() for r in rows]
assert max(b-a for a, b in zip(times, times[1:])) <= 120
report = {"pass": True, "checked_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "seconds_since_immediate_checks": now-(deadline-600), "active_check_count": checks,
          "minute_samples": len(rows), "ws_active_clients": m["md_ws_active_clients"],
          "subscriptions_before": len(before), "subscriptions_after": len(after), "missing": 0, "extra": 0,
          "ticks_dropped": m["md_ticks_dropped"], "write_timeouts": m["md_ws_kicked_write_timeout_total"],
          "rss_mib": m["process_resident_memory_bytes"] / 2**20,
          "cpu_seconds": m["process_cpu_seconds_total"], "uptime_s": m["process_uptime_seconds"],
          "compatibility_pass": True, "observation_24h": "running; not yet complete"}
(OUT / "acceptance-10min.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2), flush=True)
