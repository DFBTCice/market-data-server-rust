#!/usr/bin/env python3
"""换版后 10 分钟验收（只读：GET 现网接口 + Portainer 容器信息）。
判据：健康、RestartCount=0、订阅数回到换前、md_ws_active_clients=2、各连接器重连增量 ≤ 1（无重连风暴）、
新指标出现且开关为 1、jemalloc 指标在、写超时为 0。用法：python3 evidence/1.2.0/tools/check_after_swap.py"""
import datetime
import json
import pathlib
import re
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "evidence/1.1.1/tools"))
from pt_exp import D, call, guard  # noqa: E402

OUT = ROOT / "evidence/1.2.0/nas-198/swap"


def get(path):
    return urllib.request.urlopen("http://192.168.1.198:8081" + path, timeout=10).read()


def reconnects(text):
    return {m.group(1): float(m.group(2)) for m in re.finditer(r'^md_connector_reconnect_total\{exchange="([^"]+)"\} (\S+)$', text, re.M)}


guard()
result = json.loads((OUT / "swap-result.json").read_text())
assert result["status"] == "immediate_checks_passed", result
before = json.loads((OUT / "subscriptions.before.json").read_text())
c = call("GET", f"{D}/containers/md-server/json")
metrics = get("/metrics").decode()
subs = json.loads(get("/api/v1/subscriptions"))
first = (OUT / "metrics.after.txt").read_text()
key = lambda s: (s["exchange"], s["data_type"], s["symbol"], s.get("kline_interval", ""))
missing = sorted(set(map(key, before)) - set(map(key, subs)))
deltas = {k: v - reconnects(first).get(k, 0) for k, v in reconnects(metrics).items()}
checks = {
    "same_container": c["Id"] == result["new_container_id"],
    "healthy": c["State"].get("Health", {}).get("Status") == "healthy",
    "restart_count_0": c["RestartCount"] == 0,
    "subscriptions_back": not missing,
    "ws_active_clients_2": "\nmd_ws_active_clients 2\n" in metrics,
    "no_reconnect_storm": all(d <= 1 for d in deltas.values()),
    "runtime_switch_on": "\nmd_runtime_subscriptions_enabled 1\n" in metrics,
    "jemalloc_metrics": "\nmd_jemalloc_allocated_bytes " in metrics,
    "write_timeout_0": "\nmd_ws_kicked_write_timeout_total 0\n" in metrics,
}
record = {"checked_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "checks": checks,
          "subscriptions_before": len(before), "subscriptions_now": len(subs), "missing": missing,
          "reconnect_deltas_since_swap": deltas, "pass": all(checks.values())}
(OUT / "ten-minute.json").write_text(json.dumps(record, indent=2) + "\n")
(OUT / "metrics.ten-minute.txt").write_text(metrics)
print(json.dumps(record, indent=2))
sys.exit(0 if record["pass"] else 1)
