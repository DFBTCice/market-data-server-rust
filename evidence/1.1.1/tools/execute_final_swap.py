#!/usr/bin/env python3
"""已授权的一次换版：门禁/身份检查→即时订阅→既有swap工具→立即restore→失败回滚。"""
import datetime
import hashlib
import io
import json
import pathlib
import subprocess
import tarfile
import time
import urllib.request

from pt_exp import D, call, guard

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "evidence/1.1.1/nas-198/post-swap-24h"
ART = ROOT / "evidence/1.1.1/final-artifact"
IMAGE = "md-server:1.1.1"
IMAGE_ID = "sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010"
SHA = "3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d"
BAK = "md-server-1.1.0-quote-bnb-bak"
SWAP = ROOT / "evidence/1.1.0/nas-198/tools/pt_swap.py"
OUT.mkdir(parents=True, exist_ok=True)


def get(path):
    return urllib.request.urlopen("http://192.168.1.198:8081" + path, timeout=10).read()


def run(args, filename, timeout=120):
    with (OUT / filename).open("w") as log:
        subprocess.run(args, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=timeout)


def archive_file(name, path):
    raw = call("GET", f"{D}/containers/{name}/archive?path={path}", want_raw=True)
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        return archive.extractfile(next(m for m in archive.getmembers() if m.isfile())).read()


assert datetime.datetime.now(datetime.timezone.utc) >= datetime.datetime.fromisoformat("2026-10-23T19:53:00+00:00"), "尚未到已告知窗口"
assert not (OUT / "swap-result.json").exists(), "已有执行结果，拒绝重复换版"
analysis = json.loads((ART / "memory-2h/analysis.json").read_text())
assert analysis["pass"] and all(analysis["checks"].values())
assert (ART / "memory-2h/finished-at.txt").exists()
assessment = json.loads((ART / "anomalies/assessment.json").read_text())
assert assessment["complete"] and assessment["safe_to_swap"]
guard()
old = call("GET", f"{D}/containers/md-server/json")
assert old["Config"]["Image"] == "md-server:1.1.0-quote-bnb" and old["State"]["Running"]
assert call("GET", f"{D}/images/{IMAGE}/json")["Id"] == IMAGE_ID
names = [n.lstrip("/") for c in call("GET", f"{D}/containers/json?all=1") for n in c["Names"]]
assert BAK not in names and "RUST_LOG=warn" in old["Config"]["Env"]
subscriptions = get("/api/v1/subscriptions")
parsed = json.loads(subscriptions)
assert isinstance(parsed, list) and parsed and all(s["data_type"] in (1, 2, 3) for s in parsed)
(OUT / "subscriptions.before.json").write_bytes(subscriptions)
(OUT / "metrics.before.txt").write_bytes(get("/metrics"))
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
(OUT / "swap-started.json").write_text(json.dumps({"started_at_utc": started, "old_container_id": old["Id"],
    "old_image_id": old["Image"], "old_restart_count": old["RestartCount"], "subscriptions": len(parsed)}, indent=2) + "\n")
try:
    run(["python3", str(SWAP), "swap", IMAGE, BAK], "swap.log")
    # 健康返回后第一件事就是补订阅；后面的指标、身份检查不得挤到它前面。
    run(["node", "scripts/compat/restore_subs.mjs", str(OUT / "subscriptions.before.json"),
         "http://192.168.1.198:8081", "--apply"], "restore.log", 60)
    after = get("/api/v1/subscriptions")
    (OUT / "subscriptions.after.json").write_bytes(after)
    run(["node", "scripts/compat/compare.mjs", "subs", str(OUT / "subscriptions.before.json"),
         str(OUT / "subscriptions.after.json")], "compare-subs.log", 15)
    deadline = time.time() + 45
    while True:
        current = call("GET", f"{D}/containers/md-server/json")
        metrics = get("/metrics").decode()
        if current["State"].get("Health", {}).get("Status") == "healthy" and "md_ws_active_clients 2\n" in metrics:
            break
        assert time.time() < deadline, "健康或两个消费者未恢复"
        time.sleep(1)
    assert current["Image"] == IMAGE_ID and "RUST_LOG=warn" in current["Config"]["Env"]
    for field in ("Memory", "MemorySwap", "NanoCpus", "RestartPolicy", "PortBindings"):
        assert current["HostConfig"][field] == old["HostConfig"][field], field
    binary = archive_file("md-server", "/usr/local/bin/md-server")
    assert hashlib.sha256(binary).hexdigest() == SHA
    assert archive_file("md-server", "/etc/md-server/config.yaml") == (ART / "config.final.yaml").read_bytes()
    for name in ("allocated", "active", "resident", "retained", "mapped", "metadata"):
        assert f"\nmd_jemalloc_{name}_bytes " in metrics
    assert "md_ws_kicked_write_timeout_total 0\n" in metrics
    (OUT / "metrics.after.txt").write_text(metrics)
    result = {"status": "immediate_checks_passed", "started_at_utc": started,
              "checked_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "old_container_id": old["Id"], "new_container_id": current["Id"], "new_image_id": current["Image"],
              "binary_sha256": SHA, "backup_name": BAK, "subscriptions_before": len(parsed),
              "subscriptions_after": len(json.loads(after)), "ws_active_clients": 2, "docker_health": "healthy",
              "ten_minute_acceptance": "pending", "observation_24h": "pending"}
    (OUT / "swap-result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2), flush=True)
except BaseException as exc:
    # 处理swap在“旧容器已改名、新容器尚未创建”之间失败的情况。
    containers = call("GET", f"{D}/containers/json?all=1")
    by_name = {n.lstrip("/"): c for c in containers for n in c["Names"]}
    if "md-server" in by_name and by_name["md-server"]["Id"] != old["Id"] and BAK in by_name:
        run(["python3", str(SWAP), "rollback", BAK], "rollback.log")
    else:
        if "md-server" not in by_name and BAK in by_name:
            call("POST", f"{D}/containers/{BAK}/rename?name=md-server")
        call("POST", f"{D}/containers/{old['Id']}/update", {"RestartPolicy": {"Name": "unless-stopped"}})
        restored = call("GET", f"{D}/containers/{old['Id']}/json")
        if not restored["State"]["Running"]:
            call("POST", f"{D}/containers/{old['Id']}/start")
        time.sleep(3)
    run(["node", "scripts/compat/restore_subs.mjs", str(OUT / "subscriptions.before.json"),
         "http://192.168.1.198:8081", "--apply"], "rollback-restore.log", 60)
    (OUT / "swap-result.json").write_text(json.dumps({"status": "rolled_back", "error": str(exc),
        "at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat()}, indent=2) + "\n")
    raise
