#!/usr/bin/env python3
"""换后与原版旁路同窗口240秒，临时增加prod的两个抓取连接，结束清理并恢复2个业务消费者。"""
import json
import datetime
import pathlib
import subprocess
import time
import urllib.request

from pt_exp import D, call, cmd_run, guard, is_exp

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = ROOT / "evidence/1.1.1/nas-198/post-swap-24h"
OUT = BASE / "compat"
NAME = "md-exp-postswap-old-110"
OUT.mkdir(exist_ok=True)
handles, children = [], []
created = False


def run(args, name):
    with (OUT / name).open("w") as log:
        subprocess.run(args, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True)


def get(port, path):
    return urllib.request.urlopen(f"http://192.168.1.198:{port}{path}", timeout=10).read()


guard()
assert json.loads((BASE / "swap-result.json").read_text())["status"] == "immediate_checks_passed"
try:
    cmd_run(NAME, "md-server:1.1.0-quote-bnb", 28108, 50108, [], 2)
    created = True
    time.sleep(10)
    subs = get(8081, "/api/v1/subscriptions")
    (OUT / "subscriptions.before.json").write_bytes(subs)
    run(["node", "scripts/compat/restore_subs.mjs", str(OUT / "subscriptions.before.json"),
         "http://192.168.1.198:28108", "--apply"], "restore-old.log")
    time.sleep(20)
    topics = [f"tick.{s['exchange']}.{s['symbol']}" if s["data_type"] == 1 else
              f"kline.{s['kline_interval']}.{s['exchange']}.{s['symbol']}" for s in json.loads(subs) if s["data_type"] in (1, 2)]
    (OUT / "capture-started.json").write_text(json.dumps({"at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "prod_temporary_capture_clients": 2}, indent=2) + "\n")
    for side, port in (("old", 28108), ("new", 8081)):
        for mode, path in (("gateway", "/ws/v1/data"), ("legacy", "/ws")):
            log = (OUT / f"{side}-{mode}.log").open("w")
            handles.append(log)
            children.append(subprocess.Popen(["node", "scripts/compat/capture_ws.mjs", mode,
                f"ws://192.168.1.198:{port}{path}", "240", str(OUT / f"{side}-{mode}"), *topics],
                cwd=ROOT, stdout=log, stderr=subprocess.STDOUT))
    assert all(p.wait() == 0 for p in children)
    for side, port in (("old", 28108), ("new", 8081)):
        (OUT / f"{side}.subscriptions.json").write_bytes(get(port, "/api/v1/subscriptions"))
        (OUT / f"{side}.metrics.txt").write_bytes(get(port, "/metrics"))
    for mode in ("gateway", "legacy"):
        run(["node", "scripts/compat/compare.mjs", "push", str(OUT / f"old-{mode}"),
             str(OUT / f"new-{mode}"), *topics], f"compare-push-{mode}.log")
    run(["node", "scripts/compat/compare.mjs", "noquote", str(OUT / "new-legacy")], "compare-noquote.log")
    for mode, ext in (("subs", "subscriptions.json"), ("metrics", "metrics.txt")):
        run(["node", "scripts/compat/compare.mjs", mode, str(OUT / f"old.{ext}"), str(OUT / f"new.{ext}")], f"compare-{mode}.log")
    run(["node", "scripts/compat/compare.mjs", "selftest"], "compare-selftest.log")
    (OUT / "result.json").write_text(json.dumps({"pass": True, "capture_s": 240, "topics": topics,
        "prod_temporary_capture_clients": 2}, indent=2) + "\n")
    print("post-swap compatibility PASS", flush=True)
finally:
    for p in children:
        if p.poll() is None:
            p.terminate()
    for log in handles:
        log.close()
    if created and is_exp(NAME):
        call("DELETE", f"{D}/containers/{NAME}?force=1&v=1")
    (OUT / "capture-finished.json").write_text(json.dumps({"at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "prod_temporary_capture_clients_closed": 2}, indent=2) + "\n")
