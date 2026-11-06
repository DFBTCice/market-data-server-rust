#!/usr/bin/env python3
"""1.2.0 旁路金丝雀（198，只操作 Portainer endpoint 3；需业务方当场提供密码换得的 /tmp/pt_jwt）。

三个旁路容器，各 256MiB 内存、2 核、restart=no、标签 purpose=md-runtime-subs-1.2.0，用完即删：
  old = md-server:1.1.1（现网镜像原样）
  off = 1.2.0 产物叠加在 md-server:1.1.1 上，配置不变（开关关闭）
  on  = 1.2.0 产物 + 现网配置 + runtime_subscriptions（闲置 2 分钟、检查 10 秒、保护期 1 分钟），RUST_LOG=warn,md_runtime_subs=info
流程：读现网订阅 → 起容器（错开启动，避免 OKX 单 IP 建连限频）→ 从运行容器读回二进制核对 sha → restore_subs.mjs 灌同一份订阅 →
  old/off 同窗口 240 秒抓取并 compare.mjs（push|subs|metrics|noquote|selftest，quote 单独留证）；on 跑 canary_consumers.mjs。
现网 md-server 只读（GET 订阅列表），不重启、不改。

用法：python3 evidence/1.2.0/tools/canary_198.py <1.2.0 二进制> <期望 sha256> [输出目录]
"""
import datetime
import hashlib
import io
import json
import pathlib
import subprocess
import sys
import tarfile
import time
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "evidence/1.1.1/tools"))
from pt_exp import D, call, cmd_build, demux, guard  # noqa: E402

HOST = "192.168.1.198"
LABEL = "md-runtime-subs-1.2.0"
MEM = 256 * 1024 * 1024
BASE_IMAGE = "md-server:1.1.1"
OFF_IMAGE, ON_IMAGE = "md-server:exp-120-off", "md-server:exp-120-on"
SPEC = [  # side, container, image, http, grpc, extra env
    ("old", "md-exp-120-old", BASE_IMAGE, 28120, 50120, []),
    ("off", "md-exp-120-off", OFF_IMAGE, 28121, 50121, []),
    ("on", "md-exp-120-on", ON_IMAGE, 28122, 50122, ["RUST_LOG=warn,md_runtime_subs=info"]),
]
RUNTIME_SECTION = """
# 1.2.0 金丝雀：开关打开、参数调短
runtime_subscriptions:
  idle_unsubscribe: true
  idle_minutes: 2
  check_interval_seconds: 10
  startup_grace_minutes: 1
  max_runtime_subscriptions: 200
"""

binary, expected_sha = pathlib.Path(sys.argv[1]), sys.argv[2]
OUT = pathlib.Path(sys.argv[3]) if len(sys.argv) > 3 else ROOT / "evidence/1.2.0/nas-198/canary"
OUT.mkdir(parents=True, exist_ok=True)
created_containers, created_images, processes, handles = [], [], [], []


def log(msg):
    line = f"{datetime.datetime.now(datetime.timezone.utc).isoformat()} {msg}"
    print(line, flush=True)
    with open(OUT / "canary-run.log", "a") as f:
        f.write(line + "\n")


def get(port, path):
    return urllib.request.urlopen(f"http://{HOST}:{port}{path}", timeout=15).read()


def spawn(args, logfile):
    handle = open(logfile, "w")
    handles.append(handle)
    proc = subprocess.Popen(args, cwd=ROOT, stdout=handle, stderr=subprocess.STDOUT)
    processes.append(proc)
    return proc


def run(args, logfile, allowed=(0,)):
    with open(logfile, "w") as handle:
        code = subprocess.run(args, cwd=ROOT, stdout=handle, stderr=subprocess.STDOUT).returncode
    assert code in allowed, f"exit={code}: {args}; see {logfile}"
    return code


def ours(name):
    c = call("GET", f"{D}/containers/{name}/json")
    return (c.get("Config", {}).get("Labels") or {}).get("purpose") == LABEL


def run_container(name, image, hport, gport, envs):
    assert name.startswith("md-exp-120-")
    body = {
        "Image": image,
        "Env": ["TZ=Asia/Shanghai"] + (envs or ["RUST_LOG=warn"]),
        "ExposedPorts": {"8081/tcp": {}, "50051/tcp": {}},
        "Labels": {"purpose": LABEL, "variant": image},
        "HostConfig": {
            "PortBindings": {"8081/tcp": [{"HostPort": str(hport)}], "50051/tcp": [{"HostPort": str(gport)}]},
            "RestartPolicy": {"Name": "no"},
            "Memory": MEM, "MemorySwap": MEM, "NanoCpus": 2_000_000_000,
            "LogConfig": {"Type": "json-file", "Config": {"max-size": "20m", "max-file": "3"}},
        },
    }
    cid = call("POST", f"{D}/containers/create?name={name}", body)["Id"]
    created_containers.append(name)
    call("POST", f"{D}/containers/{cid}/start")
    log(f"started {name} {cid[:12]} {image} http={hport}")


def read_binary_sha(name):
    archive = call("GET", f"{D}/containers/{name}/archive?path=/usr/local/bin/md-server", want_raw=True)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        data = tar.extractfile(next(m for m in tar.getmembers() if m.isfile())).read()
    return hashlib.sha256(data).hexdigest()


try:
    guard()
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == expected_sha, "本地产物 sha 与期望不符"
    prod = json.loads(get(8081, "/api/v1/subscriptions"))
    subs_file = OUT / "subscriptions.before.json"
    subs_file.write_text(json.dumps(prod, indent=2) + "\n")
    (OUT / "prod.metrics.before.txt").write_bytes(get(8081, "/metrics"))
    log(f"prod subscriptions={len(prod)} (read-only)")

    on_cfg = OUT / "config.on.yaml"
    on_cfg.write_text((ROOT / "evidence/1.1.1/final-artifact/config.final.yaml").read_text() + RUNTIME_SECTION)
    cmd_build(str(binary), OFF_IMAGE, BASE_IMAGE)
    created_images.append(OFF_IMAGE)
    cmd_build(str(binary), ON_IMAGE, BASE_IMAGE, str(on_cfg))
    created_images.append(ON_IMAGE)

    for side, name, image, hport, gport, envs in SPEC:
        run_container(name, image, hport, gport, envs)
        time.sleep(5)
    for side, name, image, hport, gport, envs in SPEC:
        deadline = time.time() + 60
        while True:
            try:
                assert json.loads(get(hport, "/health"))["status"] == "ok"
                break
            except Exception:
                assert time.time() < deadline, f"{name}: health timeout"
                time.sleep(1)
        if side != "old":
            actual = read_binary_sha(name)
            (OUT / f"{side}.image-binary-sha256.txt").write_text(actual + "\n")
            assert actual == expected_sha, f"{name}: binary sha {actual} != {expected_sha}"
        run(["node", "scripts/compat/restore_subs.mjs", str(subs_file), f"http://{HOST}:{hport}", "--apply"],
            OUT / f"{side}.restore.log")
    time.sleep(30)

    topic = lambda s: (f"tick.{s['exchange']}.{s['symbol']}" if s["data_type"] == 1 else
                       f"kline.{s['kline_interval']}.{s['exchange']}.{s['symbol']}" if s["data_type"] == 2 else
                       f"quote.{s['exchange']}.{s['symbol']}")
    topics = [topic(s) for s in prod]
    legacy = [t for t in topics if not t.startswith("quote.")]
    quotes = [t for t in topics if t.startswith("quote.")]
    (OUT / "captured_at_utc.txt").write_text(datetime.datetime.now(datetime.timezone.utc).isoformat() + "\n")
    canary = spawn(["node", "evidence/1.2.0/tools/canary_consumers.mjs", f"http://{HOST}:28122", str(OUT / "on"),
                    "--idle-sec=120", "--botd-hold-sec=90", "--total-sec=420"], OUT / "on.consumers.stdout.log")
    captures = []
    for side, port in (("old", 28120), ("off", 28121)):
        for mode, path, wanted in (("gateway", "/ws/v1/data", topics), ("legacy", "/ws", legacy)):
            captures.append(spawn(["node", "scripts/compat/capture_ws.mjs", mode, f"ws://{HOST}:{port}{path}", "240",
                                   str(OUT / f"{side}-{mode}"), *wanted], OUT / f"{side}-{mode}.log"))
    assert all(p.wait() == 0 for p in captures)
    for side, port in (("old", 28120), ("off", 28121)):
        (OUT / f"{side}.subscriptions.json").write_bytes(get(port, "/api/v1/subscriptions"))
        (OUT / f"{side}.metrics.txt").write_bytes(get(port, "/metrics"))
    codes = {}
    for name, args in [("push-gateway", ["push", str(OUT / "old-gateway"), str(OUT / "off-gateway"), *legacy]),
                       ("push-legacy", ["push", str(OUT / "old-legacy"), str(OUT / "off-legacy"), *legacy]),
                       ("noquote-legacy", ["noquote", str(OUT / "off-legacy")]),
                       ("subs", ["subs", str(OUT / "old.subscriptions.json"), str(OUT / "off.subscriptions.json")]),
                       ("metrics", ["metrics", str(OUT / "old.metrics.txt"), str(OUT / "off.metrics.txt")]),
                       ("selftest", ["selftest"])]:
        codes[name] = run(["node", "scripts/compat/compare.mjs", *args], OUT / f"compare-{name}.log", (0, 1))
    if quotes:  # 主审已裁定：quote.binance.BNBUSDT 的数量配对差异不计失败，单独留证
        codes["quote_raw"] = run(["node", "scripts/compat/compare.mjs", "push", str(OUT / "old-gateway"),
                                  str(OUT / "off-gateway"), *quotes], OUT / "compare-quote.log", (0, 1))
    (OUT / "gate-results.json").write_text(json.dumps(codes, indent=2) + "\n")
    log(f"switch-off compatibility: {codes}")

    canary_code = canary.wait()
    (OUT / "on.subscriptions.final.json").write_bytes(get(28122, "/api/v1/subscriptions"))
    (OUT / "on.metrics.final.txt").write_bytes(get(28122, "/metrics"))
    raw = call("GET", f"{D}/containers/md-exp-120-on/logs?stdout=1&stderr=1&timestamps=1&tail=5000", want_raw=True)
    (OUT / "on.container.log").write_text(demux(raw))
    for side, name, *_ in SPEC:
        s = call("GET", f"{D}/containers/{name}/stats?stream=false", timeout=30)
        (OUT / f"{side}.stats.json").write_text(json.dumps({"memory_stats": s.get("memory_stats"),
                                                             "cpu_stats": s.get("cpu_stats")}, indent=2) + "\n")
    (OUT / "prod.metrics.after.txt").write_bytes(get(8081, "/metrics"))
    log(f"switch-on canary exit={canary_code}; see {OUT / 'on' / 'summary.json'}")
finally:
    for proc in processes:
        if proc.poll() is None:
            proc.terminate()
    for handle in handles:
        handle.close()
    for name in created_containers:
        if ours(name):
            call("DELETE", f"{D}/containers/{name}?force=1&v=1")
            log(f"removed container {name}")
    for image in created_images:
        assert image.startswith("md-server:exp-120-")
        call("DELETE", f"{D}/images/{urllib.parse.quote(image, safe='')}")
        log(f"removed image {image}")
