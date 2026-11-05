#!/usr/bin/env python3
"""只读保存异常窗口日志、当前旁路指标与 NAS/Mac 时钟差。"""
import datetime
import json
import pathlib
import re
import time
import urllib.request

from pt_exp import D, call, demux, guard

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "evidence/1.1.1/final-artifact/anomalies"
OUT.mkdir(exist_ok=True)
guard()
epoch = lambda text: int(datetime.datetime.fromisoformat(text.replace("Z", "+00:00")).timestamp())
window = f"since={epoch('2026-10-23T17:48:00Z')}&until={epoch('2026-10-23T17:54:00Z')}"
for side, name, port in (("new", "md-exp-final-new-111", 28109), ("old", "md-exp-final-old-110", 28108)):
    raw = call("GET", f"{D}/containers/{name}/logs?stdout=1&stderr=1&timestamps=1&{window}", want_raw=True)
    (OUT / f"{side}.server-window.log").write_text(demux(raw))
    metrics = urllib.request.urlopen(f"http://192.168.1.198:{port}/metrics", timeout=10).read()
    (OUT / f"{side}.metrics-now.txt").write_bytes(metrics)
start = time.time()
info = call("GET", f"{D}/info")
end = time.time()
server = info["SystemTime"]
stamp = datetime.datetime.fromisoformat(re.sub(r"(\.\d{6})\d+", r"\1", server).replace("Z", "+00:00"))
row = {"client_start_utc": datetime.datetime.fromtimestamp(start, datetime.timezone.utc).isoformat(),
       "client_end_utc": datetime.datetime.fromtimestamp(end, datetime.timezone.utc).isoformat(),
       "nas_system_time": server, "rtt_s": end-start,
       "nas_minus_client_midpoint_s": stamp.timestamp()-(start+end)/2}
(OUT / "clock-alignment.json").write_text(json.dumps(row, indent=2) + "\n")
print(json.dumps(row))
