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
  7 清理全部 md-exp-120-drill* 与演练镜像，核对现网未受影响
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
    summary = {"steps": steps, "removed_containers": removed, "removed_image": IMAGE,
               "leftover_containers": left_c, "leftover_images": left_i, "prod_before": before,
               "prod_after": after}
    (OUT / "drill-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    log(f"cleanup: removed {removed}; leftovers containers={left_c} images={left_i}; prod after: {after}")
