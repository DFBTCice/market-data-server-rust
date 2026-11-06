#!/usr/bin/env python3
"""换版工具旁路演练（主审 2026-11-07 复核意见 §二-6）。只操作 md-exp-120-drill* 容器与 md-server:exp-120-release 镜像（端口 28125 / 50125）；
现网 md-server 只读（GET 订阅、指标与容器信息）。演练路径：
  0 构建演练镜像（build_release_image.py --drill）
  1 起演练用 1.1.1 容器 md-exp-120-drill（256 MiB、2 核、unless-stopped），灌入现网订阅
  2 换版演练 swap-1（execute_swap.py --drill）
  3 一级回退：idle_unsubscribe 1→0→1，on_demand_quote 0→1→0（runtime_toggle.py --drill）
  4 10 分钟验收逻辑（check_after_swap.py --drill，立即执行）
  5 二级回退正常路径（rollback_to_111.py --drill，回退容器存在）
  6 再换版 swap-2，删除回退容器，二级回退「回退容器不存在 → 先重建再切换」路径
  7 再换版 swap-3，把回退容器换成参数一致但内存只有 6 MiB 的 1.1.1（真实起不来），二级回退必须把 1.2.0 放回、补订阅、写结果
  8 清理全部 md-exp-120-drill* 与演练镜像，核对现网未受影响
只用已提交的工具版本：开跑前断言 evidence/1.2.0/tools 与 scripts/compat 没有未提交改动，并记录提交号。
用法：python3 evidence/1.2.0/tools/drill_198.py"""
import json
import pathlib
import subprocess
import sys
import urllib.parse
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import swaplib as S  # noqa: E402
from swaplib import D, call, guard  # noqa: E402

T = S.DRILL
OUT = S.ROOT / "evidence/1.2.0/nas-198/drill"
dirty = subprocess.run(["git", "status", "--porcelain", "--", "evidence/1.2.0/tools", "scripts/compat"], cwd=S.ROOT,
                       capture_output=True, text=True, check=True).stdout.strip()
assert not dirty, f"演练必须使用已提交的工具版本，先提交：\n{dirty}"
TOOLS_COMMIT = subprocess.run(["git", "rev-parse", "HEAD"], cwd=S.ROOT, capture_output=True, text=True, check=True).stdout.strip()
OUT.mkdir(parents=True, exist_ok=True)
LABEL = "md-runtime-subs-1.2.0-drill"
IMAGE = "md-server:exp-120-release"
TOOLS = "evidence/1.2.0/tools"
steps = []


def log(msg):
    line = f"{S.now()} {msg}"
    print(line, flush=True)
    with open(OUT / "drill.log", "a") as f:
        f.write(line + "\n")


def step(name, args, expect=0):
    with open(OUT / f"{name}.stdout.log", "w") as f:
        code = subprocess.run(args, cwd=S.ROOT, stdout=f, stderr=subprocess.STDOUT).returncode
    steps.append({"step": name, "command": " ".join(args), "exit": code, "expected": expect})
    log(f"step {name}: exit={code} (expected {expect}) :: {' '.join(args)}")
    assert code == expect, f"step {name} exit {code}"


def prod_snapshot(tag):
    text = urllib.request.urlopen("http://192.168.1.198:8081/metrics", timeout=10).read().decode()
    prod = call("GET", f"{D}/containers/md-server/json")
    snap = {"tag": tag, "at_utc": S.now(), "process_start_time_seconds": S.metric(text, "process_start_time_seconds"),
            "md_ws_active_clients": S.metric(text, "md_ws_active_clients"), "image": prod["Image"],
            "restart_count": prod["RestartCount"], "health": prod["State"].get("Health", {}).get("Status")}
    with open(OUT / "prod-readonly.jsonl", "a") as f:
        f.write(json.dumps(snap) + "\n")
    return snap


before = None
try:
    guard()
    before = prod_snapshot("before")
    log(f"prod (read-only) before: {before}")
    leftovers = [n for n in S.containers_by_name() if n.startswith("md-exp-120-drill")]
    assert not leftovers, f"演练容器残留: {leftovers}"

    step("0-build-drill-image", ["python3", f"{TOOLS}/build_release_image.py", "--drill"])

    body = {"Image": S.OLD_IMAGE_TAG, "Env": ["RUST_LOG=warn", "TZ=Asia/Shanghai"],
            "ExposedPorts": {"8081/tcp": {}, "50051/tcp": {}}, "Labels": {"purpose": LABEL},
            "HostConfig": {"PortBindings": {"8081/tcp": [{"HostPort": "28125"}], "50051/tcp": [{"HostPort": "50125"}]},
                           "RestartPolicy": {"Name": "unless-stopped"}, "Memory": 256 * 2**20, "MemorySwap": 256 * 2**20,
                           "NanoCpus": 2_000_000_000, "NetworkMode": "bridge",
                           "LogConfig": {"Type": "json-file", "Config": {"max-size": "20m", "max-file": "3"}}}}
    created = call("POST", f"{D}/containers/create?name={T.check_name(T.current)}", body)
    call("POST", f"{D}/containers/{created['Id']}/start")
    assert S.wait_health(T, 60) is not None, "演练 1.1.1 容器未就绪"
    (OUT / "prod.subscriptions.json").write_bytes(urllib.request.urlopen("http://192.168.1.198:8081/api/v1/subscriptions", timeout=10).read())
    step("1-restore-into-drill-111", ["node", "scripts/compat/restore_subs.mjs", str(OUT / "prod.subscriptions.json"), T.url(""), "--apply"])
    log(f"drill 1.1.1 container {created['Id'][:12]} up on {T.url('')}")

    step("2-swap-1", ["python3", f"{TOOLS}/execute_swap.py", "--drill", str(OUT / "swap-1")])
    for i, (key, value) in enumerate([("idle_unsubscribe", "false"), ("idle_unsubscribe", "true"),
                                      ("on_demand_quote", "true"), ("on_demand_quote", "false")], 1):
        step(f"3-toggle-{i}-{key}-{value}", ["python3", f"{TOOLS}/runtime_toggle.py", "--drill", key, value,
                                             str(OUT / f"toggle-{i}-{key}-{value}")])
    step("4-ten-minute-check-logic", ["python3", f"{TOOLS}/check_after_swap.py", "--drill", str(OUT / "swap-1")])
    step("5-rollback-normal", ["python3", f"{TOOLS}/rollback_to_111.py", "--drill", str(OUT / "rollback-normal"),
                               str(OUT / "swap-1/old-container.inspect.json")])
    step("6a-swap-2", ["python3", f"{TOOLS}/execute_swap.py", "--drill", str(OUT / "swap-2")])
    bak = call("GET", f"{D}/containers/{T.check_name(T.bak)}/json")
    call("DELETE", f"{D}/containers/{T.bak}?force=1&v=1")
    log(f"step 6b: deleted {T.bak} {bak['Id'][:12]} to simulate the backup container being cleaned up")
    steps.append({"step": "6b-delete-bak", "deleted": T.bak, "id": bak["Id"][:12]})
    step("6c-rollback-recreate", ["python3", f"{TOOLS}/rollback_to_111.py", "--drill", str(OUT / "rollback-recreate"),
                                  str(OUT / "swap-2/old-container.inspect.json")])
    recreate = json.loads((OUT / "rollback-recreate/rollback-result.json").read_text())
    assert recreate["bak_status"] == "recreated", recreate

    step("7a-swap-3", ["python3", f"{TOOLS}/execute_swap.py", "--drill", str(OUT / "swap-3")])
    old3 = json.loads((OUT / "swap-3/old-container.inspect.json").read_text())
    call("DELETE", f"{D}/containers/{T.check_name(T.bak)}?force=1&v=1")
    weak = S.spec_from_inspect(old3, S.OLD_IMAGE_TAG, labels={"purpose": LABEL}, restart={"Name": "no"})
    weak["HostConfig"].update(Memory=6 * 2**20, MemorySwap=6 * 2**20)
    weak_id = call("POST", f"{D}/containers/create?name={T.bak}", weak)["Id"]
    log(f"step 7b: replaced {T.bak} with {weak_id[:12]} (md-server:1.1.1, same cmd/user/env/ports, memory 6 MiB) "
        f"so the 1.1.1 container cannot become healthy")
    steps.append({"step": "7b-weak-bak", "id": weak_id[:12], "memory_bytes": 6 * 2**20})
    step("7c-rollback-put-back", ["python3", f"{TOOLS}/rollback_to_111.py", "--drill", str(OUT / "rollback-put-back"),
                                  str(OUT / "swap-3/old-container.inspect.json")], expect=1)
    put_back = json.loads((OUT / "rollback-put-back/rollback-result.json").read_text())
    now = call("GET", f"{D}/containers/{T.current}/json")
    archived = json.loads((OUT / "rollback-put-back/subscriptions.archived.json").read_text())
    subs_now = json.loads(S.http_get(T, "/api/v1/subscriptions"))
    weak_now = call("GET", f"{D}/containers/{weak_id}/json")
    check = {"status_previous_restored": put_back["status"] == "rollback_failed_previous_restored",
             "restore_subs_ok": put_back["restore_subs_exit"] == 0,
             "current_is_new_image": now["Image"] == json.loads((OUT / "image-manifest.json").read_text())["id"],
             "current_running_healthy": now["State"]["Running"] and S.wait_health(T, 30) is not None,
             "process_restarted_by_put_back": put_back["process_start_time_after"] != put_back["process_start_time_before"],
             "subscriptions_restored": len(subs_now) == len(archived),
             "weak_bak_back_under_bak_name_and_stopped": weak_now["Name"] == f"/{T.bak}" and not weak_now["State"]["Running"]}
    (OUT / "rollback-put-back/drill-check.json").write_text(json.dumps(check, indent=2) + "\n")
    log(f"step 7 check: {check}")
    assert all(check.values()), check
finally:
    removed = []
    for name, c in S.containers_by_name().items():
        if name.startswith("md-exp-120-drill"):
            T.check_name(name)
            labels = call("GET", f"{D}/containers/{name}/json")["Config"]["Labels"] or {}
            assert labels.get("purpose") == LABEL, f"{name} 没有演练标签，拒绝删除"
            call("DELETE", f"{D}/containers/{name}?force=1&v=1")
            removed.append(name)
    images = [t for i in call("GET", f"{D}/images/json") for t in (i.get("RepoTags") or [])]
    if IMAGE in images:
        call("DELETE", f"{D}/images/{urllib.parse.quote(IMAGE, safe='')}")
    left_c = [n for n in S.containers_by_name() if n.startswith("md-exp-120-drill")]
    left_i = [t for i in call("GET", f"{D}/images/json") for t in (i.get("RepoTags") or []) if "exp-120" in t]
    after = prod_snapshot("after")
    summary = {"tools_commit": TOOLS_COMMIT, "steps": steps, "removed_containers": removed, "removed_image": IMAGE,
               "leftover_containers": left_c, "leftover_images": left_i, "prod_before": before,
               "prod_after": after}
    (OUT / "drill-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    log(f"cleanup: removed {removed}; leftovers containers={left_c} images={left_i}; prod after: {after}")
