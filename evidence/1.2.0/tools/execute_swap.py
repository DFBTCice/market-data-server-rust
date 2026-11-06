#!/usr/bin/env python3
"""1.2.0 换版（见 evidence/1.2.0/nas-198/SWAP_PLAN_1.2.0.md）：
前置检查（全部在动容器之前）→ 记录换前订阅 → 换容器（旧容器改名 *-bak 停止保留）→ 健康后立刻 restore_subs → 即时检查 → 失败自动回退。
与 1.1.1 的 pt_swap.py swap 的区别：容器环境 RUST_LOG 改为 warn,md_runtime_subs=info；复制旧容器标签时去掉镜像来源标签；
自动回退走 swaplib.rollback（先确认 / 重建回退容器，不通过则不停当前容器）。

用法：
  现网：APPROVED="<业务方批准人与时间>" python3 evidence/1.2.0/tools/execute_swap.py
  演练：python3 evidence/1.2.0/tools/execute_swap.py --drill <输出目录>   # 只操作 md-exp-120-drill*，端口 28125
"""
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import swaplib as S  # noqa: E402
from swaplib import D, call, guard  # noqa: E402

DRILL = "--drill" in sys.argv
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
T = S.DRILL if DRILL else S.PROD
NAS = S.ROOT / "evidence/1.2.0/nas-198"
OUT = pathlib.Path(ARGS[0]) if DRILL else NAS / "swap"
MANIFEST = (NAS / "drill" / "image-manifest.json") if DRILL else (NAS / "image-manifest.json")
MIN_CLIENTS = 0 if DRILL else 2  # 演练容器没有业务消费者
OUT.mkdir(parents=True, exist_ok=True)


def log(msg):
    line = f"{S.now()} {msg}"
    print(line, flush=True)
    with open(OUT / "execution.log", "a") as f:
        f.write(line + "\n")


def run(args, filename, timeout=120):
    with (OUT / filename).open("w") as f:
        subprocess.run(args, cwd=S.ROOT, stdout=f, stderr=subprocess.STDOUT, check=True, timeout=timeout)


def check_canary():
    """金丝雀已通过：第 1 部分 gate-results 全 0（quote_raw 另核）；第 2 部分以修正判据后的 canary-rerun 为准；
    第一次运行的第 2 部分只有 config_subscriptions_kept 为 false，且按 source=config 复核通过（带正对照）。"""
    gates = json.loads((NAS / "canary/gate-results.json").read_text())
    assert all(code == 0 for name, code in gates.items() if name != "quote_raw"), f"金丝雀第 1 部分未通过: {gates}"
    for part in ("short", "prod"):
        rerun = json.loads((NAS / "canary-rerun" / part / "summary.json").read_text())
        assert rerun["pass"] is True, f"canary-rerun/{part} 未通过"
        first = json.loads((NAS / "canary" / part / "summary.json").read_text())
        failed = [k for k, v in first["checks"].items() if v not in (True, "not-in-baseline")]
        assert failed == ["config_subscriptions_kept"], f"canary/{part} 第一次失败项不是只有配置订阅判定: {failed}"
    run(["node", "evidence/1.2.0/tools/recheck_config_kept.mjs", "evidence/1.2.0/nas-198/canary/short/timeline.jsonl",
         "evidence/1.2.0/nas-198/canary/prod/timeline.jsonl"], "precheck-recheck-config-kept.log", 60)


if not DRILL:
    approved = os.environ.get("APPROVED", "").strip()
    assert approved, "须设置 APPROVED=<业务方批准人与时间>"
else:
    approved = "drill"
assert not (OUT / "swap-result.json").exists(), "已有执行结果，拒绝重复换版"
check_canary()
sha = S.expected_sha()
assert hashlib.sha256(S.ARTIFACT.read_bytes()).hexdigest() == sha, "本地产物 sha 不是固定期望值"
manifest = json.loads(MANIFEST.read_text())
assert manifest["binary_sha256"] == sha, "镜像清单里的二进制 sha 不是固定期望值"
guard()
T.check_name(T.current)
T.check_name(T.bak)
old = call("GET", f"{D}/containers/{T.current}/json")
assert old["Image"] == S.OLD_IMAGE_ID and old["State"]["Running"], f"{T.current} 不是正在运行的 1.1.1"
image = call("GET", f"{D}/images/{manifest['tag']}/json")
assert image["Id"] == manifest["id"] and image["Config"]["Labels"]["md-server.binary.sha256"] == sha
names = S.containers_by_name()
assert T.bak not in names, f"{T.bak} 已存在"
subscriptions = S.http_get(T, "/api/v1/subscriptions")
parsed = json.loads(subscriptions)
assert isinstance(parsed, list) and all(s["data_type"] in (1, 2, 3) for s in parsed) and (DRILL or parsed)
metrics_before = S.http_get(T, "/metrics")
clients_before = S.metric(metrics_before.decode(), "md_ws_active_clients") or 0
assert clients_before >= MIN_CLIENTS, \
    f"换版前消费者只有 {clients_before:g} 个（< {MIN_CLIENTS}），换版后会被自动回退；先确认 botd 等消费者都已连上"
(OUT / "subscriptions.before.json").write_bytes(subscriptions)
(OUT / "metrics.before.txt").write_bytes(metrics_before)
inspect_path = OUT / "old-container.inspect.json"
inspect_path.write_text(json.dumps(old, indent=2) + "\n")

env = [e for e in old["Config"]["Env"] if e.split("=", 1)[0] not in S.NEW_ENV] + [f"{k}={v}" for k, v in S.NEW_ENV.items()]
spec = S.spec_from_inspect(old, manifest["tag"], env=env,
                           labels={"md-server.version": "1.2.0", "md-server.replaced": old["Id"][:12]})
(OUT / "new-container.spec.json").write_text(json.dumps(spec, indent=2) + "\n")
started = S.now()
(OUT / "swap-started.json").write_text(json.dumps({"mode": T.mode, "started_at_utc": started, "approved": approved,
    "old_container_id": old["Id"], "old_image_id": old["Image"], "old_restart_count": old["RestartCount"],
    "subscriptions": len(parsed)}, indent=2) + "\n")
log(f"swap start ({T.mode}): {T.current} {old['Id'][:12]} -> {manifest['tag']}; subscriptions={len(parsed)}")
try:
    t_stop = time.time()
    call("POST", f"{D}/containers/{T.current}/rename?name={T.bak}")
    call("POST", f"{D}/containers/{old['Id']}/update", {"RestartPolicy": {"Name": "no"}})
    call("POST", f"{D}/containers/{old['Id']}/stop?t=15")
    created = call("POST", f"{D}/containers/create?name={T.current}", spec)
    call("POST", f"{D}/containers/{created['Id']}/start")
    up = S.wait_health(T, 60)
    assert up is not None, "新容器 60 秒内未就绪"
    outage = round(time.time() - t_stop, 1)
    run(["node", "scripts/compat/restore_subs.mjs", str(OUT / "subscriptions.before.json"), T.url(""), "--apply"],
        "restore.log", 60)
    after = S.http_get(T, "/api/v1/subscriptions")
    (OUT / "subscriptions.after.json").write_bytes(after)
    deadline = time.time() + 90
    while True:
        current = call("GET", f"{D}/containers/{T.current}/json")
        metrics = S.http_get(T, "/metrics").decode()
        clients = S.metric(metrics, "md_ws_active_clients") or 0
        if current["State"].get("Health", {}).get("Status") == "healthy" and clients >= MIN_CLIENTS:
            break
        assert time.time() < deadline, f"健康或消费者（>= {MIN_CLIENTS}）90 秒内未恢复"
        time.sleep(2)
    assert current["Image"] == manifest["id"]
    assert all(f"{k}={v}" in current["Config"]["Env"] for k, v in S.NEW_ENV.items())
    for field in ("Memory", "MemorySwap", "NanoCpus", "RestartPolicy", "PortBindings"):
        assert current["HostConfig"][field] == old["HostConfig"][field], field
    labels = current["Config"]["Labels"] or {}
    assert labels.get("md-server.binary.sha256") == sha, "容器标签里的二进制 sha 不是新产物（镜像标签被旧标签覆盖）"
    assert hashlib.sha256(S.read_file(T.current, "/usr/local/bin/md-server")).hexdigest() == sha
    assert S.read_file(T.current, "/etc/md-server/config.yaml") == S.RELEASE_CONFIG.read_bytes()
    assert S.metric(metrics, "md_runtime_subscriptions_enabled") == 1
    for name in ("allocated", "active", "resident", "retained", "mapped", "metadata"):
        assert f"\nmd_jemalloc_{name}_bytes " in metrics
    (OUT / "metrics.after.txt").write_text(metrics)
    result = {"status": "immediate_checks_passed", "mode": T.mode, "started_at_utc": started, "approved": approved,
              "checked_at_utc": S.now(), "outage_seconds": outage, "health_ready_seconds": up,
              "old_container_id": old["Id"], "new_container_id": current["Id"], "new_image_id": current["Image"],
              "binary_sha256": sha, "backup_name": T.bak,
              "subscriptions_before": len(parsed), "subscriptions_after": len(json.loads(after)),
              "ws_active_clients": clients, "ws_kicked_write_timeout_total": S.metric(metrics, "md_ws_kicked_write_timeout_total"),
              "docker_health": "healthy", "ten_minute_acceptance": "pending"}
    (OUT / "swap-result.json").write_text(json.dumps(result, indent=2) + "\n")
    log(f"swap done: {json.dumps(result)}")
except BaseException as exc:
    log(f"swap failed: {exc}; reverting")
    status = "rolled_back"
    try:
        revert = S.revert_after_failed_swap(T, old["Id"], inspect_path, log)
    except S.RollbackFailed as rexc:
        status = "revert_failed_new_container_restored" if rexc.restored else "revert_failed_manual_recovery_needed"
        revert = {"error": str(rexc), "restored": rexc.restored}
        log(f"revert failed: {rexc}")
    except BaseException as rexc:
        status, revert = "revert_failed_manual_recovery_needed", {"error": repr(rexc)}
        log(f"revert failed: {rexc!r}")
    try:
        with (OUT / "rollback-restore.log").open("w") as f:
            restore_code = subprocess.run(["node", "scripts/compat/restore_subs.mjs", str(OUT / "subscriptions.before.json"),
                                           T.url(""), "--apply"], cwd=S.ROOT, stdout=f, stderr=subprocess.STDOUT,
                                          timeout=60).returncode
    except Exception as restore_exc:
        restore_code = repr(restore_exc)
    (OUT / "swap-result.json").write_text(json.dumps({"status": status, "error": str(exc), "revert": revert,
        "restore_subs_exit": restore_code, "at_utc": S.now()}, indent=2, ensure_ascii=False) + "\n")
    log(f"swap result: {status}; restore_subs exit {restore_code}")
    raise
