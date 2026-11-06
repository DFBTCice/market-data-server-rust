#!/usr/bin/env python3
"""1.2.0 旁路金丝雀（198，只操作 Portainer endpoint 3；需业务方当场提供密码换得的 /tmp/pt_jwt）。主审 2026-11-06 复核意见 §四。

旁路容器各 256MiB 内存、2 核、restart=no、标签 purpose=md-runtime-subs-1.2.0，用完即删；现网 md-server 只读（GET）。
第 1 部分（同时运行、错开 5 秒启动，避免 OKX 单 IP 建连限频）：
  old   = md-server:1.1.1（现网镜像原样）
  off   = 1.2.0 产物叠加在 md-server:1.1.1 上，配置不变（开关关闭）       → 与 old 同窗口 240 秒 compare.mjs push|subs|metrics|noquote|selftest
  short = 1.2.0 产物 + 现网配置 + runtime_subscriptions（闲置 2 分钟、检查 10 秒、保护期 1 分钟、报价补订关闭）→ canary_consumers.mjs 试跑 420 秒
第 2 部分（第 1 部分容器删除后）：
  prod  = 1.2.0 产物 + evidence/1.2.0/nas-198/config.release.yaml（即换版镜像的配置：闲置 10、保护 10、检查 60 秒、报价补订关闭）
          → canary_consumers.mjs 现网参数复跑 900 秒
全程每 10 秒采样各旁路容器的 docker stats（内存、CPU）；前后读现网 /metrics 核对未受影响。

用法：python3 evidence/1.2.0/tools/canary_198.py <1.2.0 二进制> <期望 sha256> [输出目录]
"""
import csv
import datetime
import hashlib
import io
import json
import pathlib
import subprocess
import sys
import tarfile
import threading
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
OFF_IMAGE, SHORT_IMAGE, PROD_IMAGE = "md-server:exp-120-off", "md-server:exp-120-short", "md-server:exp-120-prod"
RUNTIME_LOG = ["RUST_LOG=warn,md_runtime_subs=info"]
PART1 = [  # side, container, image, http, grpc, env
    ("old", "md-exp-120-old", BASE_IMAGE, 28120, 50120, ["RUST_LOG=warn"]),
    ("off", "md-exp-120-off", OFF_IMAGE, 28121, 50121, ["RUST_LOG=warn"]),
    ("short", "md-exp-120-short", SHORT_IMAGE, 28122, 50122, RUNTIME_LOG),
]
PART2 = [("prod", "md-exp-120-prod", PROD_IMAGE, 28123, 50123, RUNTIME_LOG)]
SHORT_SECTION = """
# 1.2.0 金丝雀试跑：开关打开、参数调短、报价补订关闭
runtime_subscriptions:
  idle_unsubscribe: true
  idle_minutes: 2
  check_interval_seconds: 10
  startup_grace_minutes: 1
  max_runtime_subscriptions: 200
  on_demand_quote: false
"""

binary, expected_sha = pathlib.Path(sys.argv[1]), sys.argv[2]
OUT = pathlib.Path(sys.argv[3]) if len(sys.argv) > 3 else ROOT / "evidence/1.2.0/nas-198/canary"
OUT.mkdir(parents=True, exist_ok=True)
created_containers, created_images, processes, handles = [], [], [], []
active = {}  # side -> container name，供 stats 采样
stop_sampling = threading.Event()


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


def run_container(side, name, image, hport, gport, envs):
    assert name.startswith("md-exp-120-")
    body = {
        "Image": image,
        "Env": ["TZ=Asia/Shanghai"] + envs,
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
    active[side] = name
    log(f"started {name} {cid[:12]} {image} http={hport} env={envs}")


def remove_container(side, name):
    active.pop(side, None)
    if ours(name):
        call("DELETE", f"{D}/containers/{name}?force=1&v=1")
        created_containers.remove(name)
        log(f"removed container {name}")


def read_file(name, path):
    archive = call("GET", f"{D}/containers/{name}/archive?path={path}", want_raw=True)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        return tar.extractfile(next(m for m in tar.getmembers() if m.isfile())).read()


def start_and_restore(spec, subs_file):
    for side, name, image, hport, gport, envs in spec:
        run_container(side, name, image, hport, gport, envs)
        time.sleep(5)
    for side, name, image, hport, gport, envs in spec:
        deadline = time.time() + 60
        while True:
            try:
                assert json.loads(get(hport, "/health"))["status"] == "ok"
                break
            except Exception:
                assert time.time() < deadline, f"{name}: health timeout"
                time.sleep(1)
        if image != BASE_IMAGE:
            actual = hashlib.sha256(read_file(name, "/usr/local/bin/md-server")).hexdigest()
            (OUT / f"{side}.image-binary-sha256.txt").write_text(actual + "\n")
            assert actual == expected_sha, f"{name}: binary sha {actual} != {expected_sha}"
        (OUT / f"{side}.config.yaml").write_bytes(read_file(name, "/etc/md-server/config.yaml"))
        run(["node", "scripts/compat/restore_subs.mjs", str(subs_file), f"http://{HOST}:{hport}", "--apply"],
            OUT / f"{side}.restore.log")


def stats_sampler():
    """每 10 秒采样当前旁路容器的内存与 CPU（docker stats stream=false 自带前一读数，可算 CPU%）"""
    path = OUT / "docker-stats.csv"
    with open(path, "w", newline="") as f:
        csv.writer(f).writerow(["at_utc", "side", "mem_usage_mib", "mem_anon_mib", "cpu_pct"])
    while not stop_sampling.wait(10):
        for side, name in list(active.items()):
            try:
                s = call("GET", f"{D}/containers/{name}/stats?stream=false", timeout=30)
                mem = s.get("memory_stats", {})
                stats = mem.get("stats", {})
                cpu, pre = s.get("cpu_stats", {}), s.get("precpu_stats", {})
                cpu_delta = cpu.get("cpu_usage", {}).get("total_usage", 0) - pre.get("cpu_usage", {}).get("total_usage", 0)
                sys_delta = cpu.get("system_cpu_usage", 0) - pre.get("system_cpu_usage", 0)
                ncpu = cpu.get("online_cpus") or 1
                pct = 100.0 * cpu_delta / sys_delta * ncpu if sys_delta > 0 else ""
                row = [datetime.datetime.now(datetime.timezone.utc).isoformat(), side,
                       round(mem.get("usage", 0) / 1048576, 1), round(stats.get("anon", 0) / 1048576, 1),
                       round(pct, 2) if pct != "" else ""]
                with open(path, "a", newline="") as f:
                    csv.writer(f).writerow(row)
            except Exception as exc:  # 采样失败只记一行，不中断金丝雀
                log(f"stats sample failed for {name}: {exc}")


def prod_snapshot(tag):
    text = get(8081, "/metrics").decode()
    (OUT / f"prod.metrics.{tag}.txt").write_text(text)
    subs = json.loads(get(8081, "/api/v1/subscriptions"))
    pick = lambda n: next((l.split()[1] for l in text.splitlines() if l.startswith(n + " ")), None)
    snap = {"tag": tag, "at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "process_start_time_seconds": pick("process_start_time_seconds"),
            "process_resident_memory_bytes": pick("process_resident_memory_bytes"),
            "md_ws_active_clients": pick("md_ws_active_clients"), "md_quotes_processed": pick("md_quotes_processed"),
            "md_ticks_processed": pick("md_ticks_processed"), "subscriptions": len(subs)}
    with open(OUT / "prod-readonly.jsonl", "a") as f:
        f.write(json.dumps(snap) + "\n")
    return snap


try:
    guard()
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == expected_sha, "本地产物 sha 与期望不符"
    before = prod_snapshot("before")
    prod_subs = json.loads(get(8081, "/api/v1/subscriptions"))
    subs_file = OUT / "subscriptions.before.json"
    subs_file.write_text(json.dumps(prod_subs, indent=2) + "\n")
    log(f"prod (read-only): {before}")

    short_cfg = OUT / "config.short.yaml"
    short_cfg.write_text((ROOT / "evidence/1.1.1/final-artifact/config.final.yaml").read_text() + SHORT_SECTION)
    for image, cfg in ((OFF_IMAGE, None), (SHORT_IMAGE, str(short_cfg)),
                       (PROD_IMAGE, str(ROOT / "evidence/1.2.0/nas-198/config.release.yaml"))):
        cmd_build(str(binary), image, BASE_IMAGE, cfg)
        created_images.append(image)

    sampler = threading.Thread(target=stats_sampler, daemon=True)
    sampler.start()

    # ---- 第 1 部分：old / off 字节对比 + short 试跑 ----
    start_and_restore(PART1, subs_file)
    time.sleep(30)
    topic = lambda s: (f"tick.{s['exchange']}.{s['symbol']}" if s["data_type"] == 1 else
                       f"kline.{s['kline_interval']}.{s['exchange']}.{s['symbol']}" if s["data_type"] == 2 else
                       f"quote.{s['exchange']}.{s['symbol']}")
    topics = [topic(s) for s in prod_subs]
    legacy = [t for t in topics if not t.startswith("quote.")]
    quotes = [t for t in topics if t.startswith("quote.")]
    (OUT / "captured_at_utc.txt").write_text(datetime.datetime.now(datetime.timezone.utc).isoformat() + "\n")
    short = spawn(["node", "evidence/1.2.0/tools/canary_consumers.mjs", f"http://{HOST}:28122", str(OUT / "short"),
                   "--idle-sec=120", "--botd-hold-sec=90", "--total-sec=420"], OUT / "short.consumers.stdout.log")
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
    if quotes:  # docs/33 §4 已裁定：quote.binance.BNBUSDT 的数量配对差异不计失败，单独留证
        codes["quote_raw"] = run(["node", "scripts/compat/compare.mjs", "push", str(OUT / "old-gateway"),
                                  str(OUT / "off-gateway"), *quotes], OUT / "compare-quote.log", (0, 1))
    (OUT / "gate-results.json").write_text(json.dumps(codes, indent=2) + "\n")
    log(f"part 1 switch-off compatibility: {codes}")
    for side in ("old", "off"):
        name = dict((s, n) for s, n, *_ in PART1)[side]
        raw = call("GET", f"{D}/containers/{name}/logs?stdout=1&stderr=1&timestamps=1&tail=5000", want_raw=True)
        (OUT / f"{side}.container.log").write_text(demux(raw))
        remove_container(side, name)

    short_code = short.wait()
    (OUT / "short.subscriptions.final.json").write_bytes(get(28122, "/api/v1/subscriptions"))
    (OUT / "short.metrics.final.txt").write_bytes(get(28122, "/metrics"))
    raw = call("GET", f"{D}/containers/md-exp-120-short/logs?stdout=1&stderr=1&timestamps=1&tail=5000", want_raw=True)
    (OUT / "short.container.log").write_text(demux(raw))
    remove_container("short", "md-exp-120-short")
    log(f"part 2a short trial exit={short_code}; see {OUT / 'short' / 'summary.json'}")
    prod_snapshot("after-part1")

    # ---- 第 2 部分：现网参数复跑 ----
    start_and_restore(PART2, subs_file)
    prod = spawn(["node", "evidence/1.2.0/tools/canary_consumers.mjs", f"http://{HOST}:28123", str(OUT / "prod"),
                  "--idle-sec=600", "--botd-hold-sec=60", "--total-sec=900"], OUT / "prod.consumers.stdout.log")
    prod_code = prod.wait()
    (OUT / "prod.subscriptions.final.json").write_bytes(get(28123, "/api/v1/subscriptions"))
    (OUT / "prod.metrics.final.txt").write_bytes(get(28123, "/metrics"))
    raw = call("GET", f"{D}/containers/md-exp-120-prod/logs?stdout=1&stderr=1&timestamps=1&tail=5000", want_raw=True)
    (OUT / "prod.container.log").write_text(demux(raw))
    remove_container("prod", "md-exp-120-prod")
    log(f"part 2b production-parameter rerun exit={prod_code}; see {OUT / 'prod' / 'summary.json'}")
    after = prod_snapshot("after")
    assert after["process_start_time_seconds"] == before["process_start_time_seconds"], "现网进程被重启过"
    log(f"prod (read-only) after: {after}")
finally:
    stop_sampling.set()
    for proc in processes:
        if proc.poll() is None:
            proc.terminate()
    for handle in handles:
        handle.close()
    for name in list(created_containers):
        try:
            if ours(name):
                call("DELETE", f"{D}/containers/{name}?force=1&v=1")
                log(f"removed container {name}")
        except SystemExit as exc:
            log(f"cleanup failed for {name}: {exc}")
    for image in created_images:
        assert image.startswith("md-server:exp-120-")
        call("DELETE", f"{D}/images/{urllib.parse.quote(image, safe='')}")
        log(f"removed image {image}")
