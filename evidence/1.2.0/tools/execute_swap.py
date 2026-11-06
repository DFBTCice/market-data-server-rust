#!/usr/bin/env python3
"""1.2.0 换版（业务方批准后执行，见 evidence/1.2.0/nas-198/SWAP_PLAN_1.2.0.md）：
门禁 / 身份检查 → 记录换前订阅 → 换容器（旧容器改名 md-server-1.1.1-bak 停止保留）→ 立刻 restore_subs → 即时检查 → 失败自动回退。
与 1.1.1 的 pt_swap.py swap 唯一区别：容器环境 RUST_LOG 改为 warn,md_runtime_subs=info（业务方决定），其余参数沿用旧容器。

用法：APPROVED="<批准人与时间>" python3 evidence/1.2.0/tools/execute_swap.py
"""
import datetime
import hashlib
import io
import json
import os
import pathlib
import subprocess
import sys
import tarfile
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "evidence/1.1.1/tools"))
from pt_exp import D, call, guard  # noqa: E402

NAS = ROOT / "evidence/1.2.0/nas-198"
OUT = NAS / "swap"
CANARY = NAS / "canary"
OLD_IMAGE_ID = "sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010"  # 现网 md-server:1.1.1
BAK = "md-server-1.1.1-bak"
NEW_ENV = {"RUST_LOG": "warn,md_runtime_subs=info", "TZ": "Asia/Shanghai"}
ROLLBACK = ROOT / "evidence/1.1.0/nas-198/tools/pt_swap.py"
OUT.mkdir(parents=True, exist_ok=True)


def get(path):
    return urllib.request.urlopen("http://192.168.1.198:8081" + path, timeout=10).read()


def run(args, filename, timeout=120):
    with (OUT / filename).open("w") as f:
        subprocess.run(args, cwd=ROOT, stdout=f, stderr=subprocess.STDOUT, check=True, timeout=timeout)


def read_file(name, path):
    raw = call("GET", f"{D}/containers/{name}/archive?path={path}", want_raw=True)
    with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
        return tar.extractfile(next(m for m in tar.getmembers() if m.isfile())).read()


def wait_health(limit=60):
    t0 = time.time()
    while time.time() - t0 < limit:
        try:
            if json.loads(get("/health")).get("status") == "ok":
                return round(time.time() - t0, 1)
        except Exception:
            pass
        time.sleep(0.5)
    return None


approved = os.environ.get("APPROVED", "").strip()
assert approved, "须设置 APPROVED=<业务方批准人与时间>"
assert not (OUT / "swap-result.json").exists(), "已有执行结果，拒绝重复换版"
manifest = json.loads((NAS / "image-manifest.json").read_text())
gates = json.loads((CANARY / "gate-results.json").read_text())
assert all(code == 0 for name, code in gates.items() if name != "quote_raw"), f"金丝雀第 1 部分未通过: {gates}"
for part in ("short", "prod"):
    summary = json.loads((CANARY / part / "summary.json").read_text())
    assert summary["pass"], f"金丝雀 {part} 未通过"
guard()
old = call("GET", f"{D}/containers/md-server/json")
assert old["Image"] == OLD_IMAGE_ID and old["State"]["Running"], "现网不是预期的 md-server:1.1.1"
image = call("GET", f"{D}/images/{manifest['tag']}/json")
assert image["Id"] == manifest["id"] and image["Config"]["Labels"]["md-server.binary.sha256"] == manifest["binary_sha256"]
names = [n.lstrip("/") for c in call("GET", f"{D}/containers/json?all=1") for n in c["Names"]]
assert BAK not in names, f"{BAK} 已存在"
subscriptions = get("/api/v1/subscriptions")
parsed = json.loads(subscriptions)
assert isinstance(parsed, list) and parsed and all(s["data_type"] in (1, 2, 3) for s in parsed)
(OUT / "subscriptions.before.json").write_bytes(subscriptions)
(OUT / "metrics.before.txt").write_bytes(get("/metrics"))
(OUT / "old-container.inspect.json").write_text(json.dumps(old, indent=2) + "\n")

cfg, hc = old["Config"], old["HostConfig"]
env = [e for e in cfg["Env"] if e.split("=", 1)[0] not in NEW_ENV] + [f"{k}={v}" for k, v in NEW_ENV.items()]
spec = {
    "Image": manifest["tag"], "Env": env, "Cmd": cfg["Cmd"], "Entrypoint": cfg.get("Entrypoint"), "User": cfg.get("User"),
    "ExposedPorts": cfg["ExposedPorts"],
    "Labels": {**(cfg.get("Labels") or {}), "md-server.version": "1.2.0", "md-server.replaced": old["Id"][:12]},
    "HostConfig": {k: hc[k] for k in ("PortBindings", "RestartPolicy", "Memory", "MemorySwap", "NanoCpus", "NetworkMode", "LogConfig")},
}
(OUT / "new-container.spec.json").write_text(json.dumps(spec, indent=2) + "\n")
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
(OUT / "swap-started.json").write_text(json.dumps({"started_at_utc": started, "approved": approved, "old_container_id": old["Id"],
    "old_image_id": old["Image"], "old_restart_count": old["RestartCount"], "subscriptions": len(parsed)}, indent=2) + "\n")
try:
    t_stop = time.time()
    call("POST", f"{D}/containers/md-server/rename?name={BAK}")
    call("POST", f"{D}/containers/{old['Id']}/update", {"RestartPolicy": {"Name": "no"}})
    call("POST", f"{D}/containers/{old['Id']}/stop?t=15")
    created = call("POST", f"{D}/containers/create?name=md-server", spec)
    call("POST", f"{D}/containers/{created['Id']}/start")
    up = wait_health(60)
    assert up is not None, "新容器 60 秒内未就绪"
    outage = round(time.time() - t_stop, 1)
    # 健康后第一件事就是补订阅
    run(["node", "scripts/compat/restore_subs.mjs", str(OUT / "subscriptions.before.json"),
         "http://192.168.1.198:8081", "--apply"], "restore.log", 60)
    after = get("/api/v1/subscriptions")
    (OUT / "subscriptions.after.json").write_bytes(after)
    deadline = time.time() + 90
    while True:
        current = call("GET", f"{D}/containers/md-server/json")
        metrics = get("/metrics").decode()
        if current["State"].get("Health", {}).get("Status") == "healthy" and "md_ws_active_clients 2\n" in metrics:
            break
        assert time.time() < deadline, "健康或两个消费者 90 秒内未恢复"
        time.sleep(2)
    assert current["Image"] == manifest["id"]
    assert all(f"{k}={v}" in current["Config"]["Env"] for k, v in NEW_ENV.items())
    for field in ("Memory", "MemorySwap", "NanoCpus", "RestartPolicy", "PortBindings"):
        assert current["HostConfig"][field] == old["HostConfig"][field], field
    assert hashlib.sha256(read_file("md-server", "/usr/local/bin/md-server")).hexdigest() == manifest["binary_sha256"]
    assert read_file("md-server", "/etc/md-server/config.yaml") == (NAS / "config.release.yaml").read_bytes()
    for needle in ("\nmd_runtime_subscriptions_enabled 1\n", "\nmd_ws_kicked_write_timeout_total 0\n"):
        assert needle in metrics, needle
    for name in ("allocated", "active", "resident", "retained", "mapped", "metadata"):
        assert f"\nmd_jemalloc_{name}_bytes " in metrics
    (OUT / "metrics.after.txt").write_text(metrics)
    result = {"status": "immediate_checks_passed", "started_at_utc": started, "approved": approved,
              "checked_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "outage_seconds": outage, "health_ready_seconds": up,
              "old_container_id": old["Id"], "new_container_id": current["Id"], "new_image_id": current["Image"],
              "binary_sha256": manifest["binary_sha256"], "backup_name": BAK,
              "subscriptions_before": len(parsed), "subscriptions_after": len(json.loads(after)),
              "ws_active_clients": 2, "docker_health": "healthy", "ten_minute_acceptance": "pending"}
    (OUT / "swap-result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2), flush=True)
except BaseException as exc:
    containers = call("GET", f"{D}/containers/json?all=1")
    by_name = {n.lstrip("/"): c for c in containers for n in c["Names"]}
    if "md-server" in by_name and by_name["md-server"]["Id"] != old["Id"] and BAK in by_name:
        run(["python3", str(ROLLBACK), "rollback", BAK], "rollback.log")
    else:
        if "md-server" not in by_name and BAK in by_name:
            call("POST", f"{D}/containers/{BAK}/rename?name=md-server")
        call("POST", f"{D}/containers/{old['Id']}/update", {"RestartPolicy": {"Name": "unless-stopped"}})
        if not call("GET", f"{D}/containers/{old['Id']}/json")["State"]["Running"]:
            call("POST", f"{D}/containers/{old['Id']}/start")
        wait_health(60)
    run(["node", "scripts/compat/restore_subs.mjs", str(OUT / "subscriptions.before.json"),
         "http://192.168.1.198:8081", "--apply"], "rollback-restore.log", 60)
    (OUT / "swap-result.json").write_text(json.dumps({"status": "rolled_back", "error": str(exc),
        "at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat()}, indent=2) + "\n")
    raise
