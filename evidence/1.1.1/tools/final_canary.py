#!/usr/bin/env python3
"""最终镜像旁路：同一现网订阅，240s 抓取 + >=2h 两消费者负载；最后只清理本次旁路容器。"""
import datetime
import hashlib
import io
import json
import pathlib
import subprocess
import tarfile
import time
import urllib.request

from pt_exp import D, call, cmd_run, guard, is_exp

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "evidence/1.1.1/final-artifact"
CAP = OUT / "canary"
MEM = OUT / "memory-2h"
HOST = "192.168.1.198"
SPEC = [("old", "md-exp-final-old-110", "md-server:1.1.0-quote-bnb", 28108, 50108),
        ("new", "md-exp-final-new-111", "md-server:1.1.1", 28109, 50109)]
DURATION = 7260  # 超过 2h，按采样实际跨度验收。
CAP.mkdir(parents=True, exist_ok=True)
MEM.mkdir(parents=True, exist_ok=True)
created, processes, handles = [], [], []


def spawn(args, log):
    handle = open(log, "w")
    handles.append(handle)
    proc = subprocess.Popen(args, cwd=ROOT, stdout=handle, stderr=subprocess.STDOUT)
    processes.append(proc)
    return proc


def run(args, log, allowed=(0,)):
    with open(log, "w") as handle:
        code = subprocess.run(args, cwd=ROOT, stdout=handle, stderr=subprocess.STDOUT).returncode
    assert code in allowed, f"exit={code}: {args}; see {log}"
    return code


def get(port, path):
    return urllib.request.urlopen(f"http://{HOST}:{port}{path}", timeout=15).read()


try:
    guard()
    subscriptions = json.loads(get(8081, "/api/v1/subscriptions"))
    subs_file = CAP / "subscriptions.before.json"
    subs_file.write_text(json.dumps(subscriptions, indent=2) + "\n")
    topic = lambda s: (f"tick.{s['exchange']}.{s['symbol']}" if s["data_type"] == 1 else
                       f"kline.{s['kline_interval']}.{s['exchange']}.{s['symbol']}" if s["data_type"] == 2 else
                       f"quote.{s['exchange']}.{s['symbol']}")
    topics = [topic(s) for s in subscriptions]
    legacy = [t for t in topics if not t.startswith("quote.")]
    quotes = [t for t in topics if t.startswith("quote.")]
    for side, name, image, port, grpc in SPEC:
        cmd_run(name, image, port, grpc, [], 2)
        created.append(name)
    for side, name, image, port, grpc in SPEC:
        deadline = time.time() + 60
        while True:
            try:
                assert json.loads(get(port, "/health"))["status"] == "ok"
                break
            except Exception:
                assert time.time() < deadline, f"{name}: health timeout"
                time.sleep(1)
        run(["node", "scripts/compat/restore_subs.mjs", str(subs_file),
             f"http://{HOST}:{port}", "--apply"], CAP / f"{side}.restore.log")
        # 从实际运行的镜像读回二进制，证明复验对象与最终产物逐字节一致。
        if side == "new":
            archive = call("GET", f"{D}/containers/{name}/archive?path=/usr/local/bin/md-server", want_raw=True)
            with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
                binary = tar.extractfile(next(m for m in tar.getmembers() if m.isfile())).read()
            actual = hashlib.sha256(binary).hexdigest()
            expected = json.loads((OUT / "image-manifest.json").read_text())["binary_sha256"]
            assert actual == expected
            (CAP / "image-binary-sha256.txt").write_text(actual + "\n")
    time.sleep(30)
    (MEM / "started-at.txt").write_text(datetime.datetime.now(datetime.timezone.utc).isoformat() + "\n")
    loads = []
    samplers = []
    for side, name, image, port, grpc in SPEC:
        samplers.append(spawn(["node", "scripts/compat/sample_metrics.mjs", f"http://{HOST}:{port}",
                                str(DURATION), "60", str(MEM / f"{side}.csv"), "--snapshot-every-min", "60"],
                               MEM / f"{side}.sampler.log"))
        loads.append(spawn(["node", "scripts/compat/load_consumers.mjs", f"ws://{HOST}:{port}/ws/v1/data",
                            str(subs_file), str(DURATION), str(MEM / f"{side}.consumers.log"),
                            "--static-klines", "2", "--churn-n", "2", "--churn-period", "90", "--churn-off", "20"],
                           MEM / f"{side}.load.stdout.log"))
    captures = []
    for side, name, image, port, grpc in SPEC:
        for mode, path, wanted in (("gateway", "/ws/v1/data", topics), ("legacy", "/ws", legacy)):
            captures.append(spawn(["node", "scripts/compat/capture_ws.mjs", mode,
                                   f"ws://{HOST}:{port}{path}", "240", str(CAP / f"{side}-{mode}"), *wanted],
                                  CAP / f"{side}-{mode}.log"))
    assert all(proc.wait() == 0 for proc in captures)
    codes = {}
    for side, name, image, port, grpc in SPEC:
        (CAP / f"{side}.subscriptions.json").write_bytes(get(port, "/api/v1/subscriptions"))
        (CAP / f"{side}.metrics.txt").write_bytes(get(port, "/metrics"))
    comparisons = [("push-gateway", ["push", str(CAP / "old-gateway"), str(CAP / "new-gateway"), *legacy]),
                   ("push-legacy", ["push", str(CAP / "old-legacy"), str(CAP / "new-legacy"), *legacy]),
                   ("noquote-legacy", ["noquote", str(CAP / "new-legacy")]),
                   ("subs", ["subs", str(CAP / "old.subscriptions.json"), str(CAP / "new.subscriptions.json")]),
                   ("metrics", ["metrics", str(CAP / "old.metrics.txt"), str(CAP / "new.metrics.txt")]),
                   ("selftest", ["selftest"])]
    for name, args in comparisons:
        codes[name] = run(["node", "scripts/compat/compare.mjs", *args], CAP / f"compare-{name}.log")
    # quote 原始对比独立留证，只豁免主审已裁定的 Binance 数量配对噪声。
    if quotes:
        quote_log = CAP / "compare-quote.log"
        codes["quote_raw"] = run(["node", "scripts/compat/compare.mjs", "push", str(CAP / "old-gateway"),
                                   str(CAP / "new-gateway"), *quotes], quote_log, (0, 1))
        section = ""
        for line in quote_log.read_text().splitlines():
            if line.startswith("== "):
                section = line[3:]
            if "FAIL" in line and "RESULT:" not in line:
                assert section == "quote.binance.BNBUSDT" and "FAIL 逐字节:" in line, line
    (CAP / "gate-results.json").write_text(json.dumps(codes, indent=2) + "\n")
    print("240s compatibility completed:", codes, flush=True)
    assert all(proc.wait() == 0 for proc in loads + samplers)
    (MEM / "finished-at.txt").write_text(datetime.datetime.now(datetime.timezone.utc).isoformat() + "\n")
    print("2h capture completed; run analyze_final_memory.py before any swap", flush=True)
finally:
    for proc in processes:
        if proc.poll() is None:
            proc.terminate()
    for proc in processes:
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
    for handle in handles:
        handle.close()
    for name in created:
        assert is_exp(name)
        call("DELETE", f"{D}/containers/{name}?force=1&v=1")
        print("removed", name, flush=True)
