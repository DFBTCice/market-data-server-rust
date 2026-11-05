#!/usr/bin/env python3
"""只读运行24小时逐分钟采样，保存PID/开始/结束标记，不覆盖已有样本。"""
import datetime
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "evidence/1.1.1/nas-198/post-swap-24h"
assert json.loads((OUT / "swap-result.json").read_text())["status"] == "immediate_checks_passed"
assert not (OUT / "prod.csv").exists(), "已有样本，拒绝覆盖"
with (OUT / "sampler.log").open("w") as log:
    proc = subprocess.Popen(["caffeinate", "-is", "node", "scripts/compat/sample_metrics.mjs",
        "http://192.168.1.198:8081", "86400", "60", str(OUT / "prod.csv"), "--snapshot-every-min", "60"],
        cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    state = {"pid": proc.pid, "started_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
             "duration_s": 86400, "interval_s": 60, "status": "running"}
    (OUT / "sampler-state.json").write_text(json.dumps(state, indent=2) + "\n")
    print(json.dumps(state), flush=True)
    code = proc.wait()
state.update(status="finished" if code == 0 else "failed", exit_code=code,
             finished_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
(OUT / "sampler-state.json").write_text(json.dumps(state, indent=2) + "\n")
print(json.dumps(state), flush=True)
raise SystemExit(code)
