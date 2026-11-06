#!/usr/bin/env python3
"""1.2.0 换版 / 回退共用逻辑（只经 Portainer endpoint 3；JWT 由 pt_exp 从 /tmp/pt_jwt 读取）。

两种模式严格分开，每个工具只拿一个 Target，所有容器操作前都用 Target.check_name() 校验名字：
  PROD  ：现网 md-server、回退容器 md-server-1.1.1-bak、失败现场 md-server-<ts>-failed，端口 8081
  DRILL ：演练容器 md-exp-120-drill、md-exp-120-drill-bak、md-exp-120-drill-<ts>-failed，端口 28125；
          演练模式拒绝任何不以 md-exp-120-drill 开头的名字，现网模式拒绝任何 md-exp- 开头的名字。
"""
import datetime
import io
import json
import pathlib
import re
import sys
import tarfile
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "evidence/1.1.1/tools"))
from pt_exp import D, call, demux, guard  # noqa: E402,F401

OLD_IMAGE_TAG = "md-server:1.1.1"
OLD_IMAGE_ID = "sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010"
EXPECTED_SHA = "cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf"
ARTIFACT = ROOT / "dist/md-server-1.2.0-12192d9-linux-x86_64"
DIST_SHA_FILE = ROOT / "evidence/1.2.0/build-logs/dist-sha256.txt"
RELEASE_CONFIG = ROOT / "evidence/1.2.0/nas-198/config.release.yaml"
NEW_ENV = {"RUST_LOG": "warn,md_runtime_subs=info", "TZ": "Asia/Shanghai"}
HOST = "192.168.1.198"
# 镜像写入的来源标签：Docker 会把它们并入容器 Labels，复制旧容器标签时必须去掉，否则新容器显示旧镜像的 sha
IMAGE_LABEL_KEYS = ("md-server.binary.sha256", "md-server.config.sha256")
IMAGE_LABEL_PREFIX = "org.opencontainers.image."


class Target:
    def __init__(self, mode, current, bak, http_port):
        self.mode, self.current, self.bak, self.http_port = mode, current, bak, http_port

    def check_name(self, name):
        if self.mode == "drill":
            assert name.startswith("md-exp-120-drill"), f"演练模式拒绝操作 {name!r}"
        else:
            assert not name.startswith("md-exp-"), f"现网模式拒绝操作 {name!r}"
            assert name == "md-server" or (name.startswith("md-server-") and name.endswith(("-bak", "-failed"))), \
                f"现网模式拒绝操作 {name!r}"
        return name

    def url(self, path):
        return f"http://{HOST}:{self.http_port}{path}"


PROD = Target("prod", "md-server", "md-server-1.1.1-bak", 8081)
DRILL = Target("drill", "md-exp-120-drill", "md-exp-120-drill-bak", 28125)


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def http_get(target, path, timeout=10):
    return urllib.request.urlopen(target.url(path), timeout=timeout).read()


def metric(text, name):
    m = re.search(rf"^{re.escape(name)} (\S+)$", text, re.M)
    return float(m.group(1)) if m else None


def wait_health(target, limit=60):
    t0 = time.time()
    while time.time() - t0 < limit:
        try:
            if json.loads(http_get(target, "/health", 3)).get("status") == "ok":
                return round(time.time() - t0, 1)
        except Exception:
            pass
        time.sleep(0.5)
    return None


def read_file(name, path):
    raw = call("GET", f"{D}/containers/{name}/archive?path={path}", want_raw=True)
    with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
        return tar.extractfile(next(m for m in tar.getmembers() if m.isfile())).read()


def containers_by_name():
    return {n.lstrip("/"): c for c in call("GET", f"{D}/containers/json?all=1") for n in c["Names"]}


def expected_sha():
    """固定期望 sha：常量必须与 build-logs/dist-sha256.txt 里该产物的记录、以及本地产物文件一致"""
    recorded = {line.split()[1]: line.split()[0] for line in DIST_SHA_FILE.read_text().splitlines()
                if re.match(r"^[0-9a-f]{64}  ", line)}
    assert recorded.get("dist/md-server-1.2.0-12192d9-linux-x86_64") == EXPECTED_SHA, "dist-sha256.txt 与固定期望 sha 不符"
    return EXPECTED_SHA


def strip_image_labels(labels):
    return {k: v for k, v in (labels or {}).items()
            if k not in IMAGE_LABEL_KEYS and not k.startswith(IMAGE_LABEL_PREFIX)}


def spec_from_inspect(inspect, image, env=None, labels=None, restart=None):
    cfg, hc = inspect["Config"], inspect["HostConfig"]
    host = {k: hc[k] for k in ("PortBindings", "RestartPolicy", "Memory", "MemorySwap", "NanoCpus", "NetworkMode", "LogConfig")}
    if restart is not None:
        host["RestartPolicy"] = restart
    return {"Image": image, "Env": env if env is not None else cfg["Env"], "Cmd": cfg["Cmd"],
            "Entrypoint": cfg.get("Entrypoint"), "User": cfg.get("User"), "ExposedPorts": cfg["ExposedPorts"],
            "Labels": {**strip_image_labels(cfg.get("Labels")), **(labels or {})}, "HostConfig": host}


def ensure_bak(target, inspect_path, log):
    """二级回退前置检查：回退容器存在、是 1.1.1 镜像、处于停止状态；不存在则按旧容器 inspect 从 md-server:1.1.1 重建并核对。
    任何检查不通过都抛异常——调用方此时还没有动当前容器。返回 'present' 或 'recreated'。"""
    target.check_name(target.bak)
    names = containers_by_name()
    if target.bak in names:
        bak = call("GET", f"{D}/containers/{target.bak}/json")
        assert bak["Image"] == OLD_IMAGE_ID, f"{target.bak} 镜像 {bak['Image'][:19]} 不是 1.1.1"
        assert not bak["State"]["Running"], f"{target.bak} 正在运行"
        log(f"rollback pre-check: {target.bak} present, image {OLD_IMAGE_ID[:19]}, stopped")
        return "present"
    old = json.loads(pathlib.Path(inspect_path).read_text())
    assert old["Image"] == OLD_IMAGE_ID, "旧容器 inspect 的镜像不是 1.1.1，拒绝据此重建"
    image = call("GET", f"{D}/images/{OLD_IMAGE_TAG}/json")
    assert image["Id"] == OLD_IMAGE_ID, f"{OLD_IMAGE_TAG} 镜像 ID {image['Id'][:19]} 不符"
    spec = spec_from_inspect(old, OLD_IMAGE_TAG, labels={"md-server.recreated-from": old["Id"][:12]},
                             restart={"Name": "no"})
    created = call("POST", f"{D}/containers/create?name={target.bak}", spec)
    bak = call("GET", f"{D}/containers/{created['Id']}/json")
    assert bak["Image"] == OLD_IMAGE_ID and bak["Config"]["Env"] == old["Config"]["Env"]
    for field in ("PortBindings", "Memory", "MemorySwap", "NanoCpus", "NetworkMode", "LogConfig"):
        assert bak["HostConfig"][field] == old["HostConfig"][field], field
    log(f"rollback pre-check: {target.bak} was missing; recreated {created['Id'][:12]} from {OLD_IMAGE_TAG} "
        f"with the old container's parameters (verified image, env, ports, memory, cpu, network, logging)")
    return "recreated"


def rollback(target, inspect_path, log):
    """二级回退：先 ensure_bak（不通过则不动当前容器）→ 停并改名当前容器为 *-failed → 回退容器改名、恢复重启策略、启动 → 等健康。
    回退容器 60 秒内不健康时，把刚停掉的容器放回原位并重新启动，然后报错（不让服务空着）。"""
    guard()
    cur_name = target.check_name(target.current)
    cur = call("GET", f"{D}/containers/{cur_name}/json")
    bak_status = ensure_bak(target, inspect_path, log)
    restart = json.loads(pathlib.Path(inspect_path).read_text())["HostConfig"]["RestartPolicy"] or {"Name": "unless-stopped"}
    failed = target.check_name(f"{target.current}-{int(time.time())}-failed")
    t0 = time.time()
    stopped = False
    try:
        call("POST", f"{D}/containers/{cur['Id']}/update", {"RestartPolicy": {"Name": "no"}})
        call("POST", f"{D}/containers/{cur['Id']}/stop?t=15")
        stopped = True
        call("POST", f"{D}/containers/{cur['Id']}/rename?name={failed}")
        call("POST", f"{D}/containers/{target.bak}/rename?name={cur_name}")
        restored = call("GET", f"{D}/containers/{cur_name}/json")
        call("POST", f"{D}/containers/{restored['Id']}/update", {"RestartPolicy": restart})
        call("POST", f"{D}/containers/{restored['Id']}/start")
        up = wait_health(target, 60)
        if up is None:
            raise SystemExit("1.1.1 container not healthy within 60s")
    except BaseException as exc:
        if stopped:
            log(f"rollback: {exc}; putting the previous container back")
            _put_back(target, cur, cur_name)
            raise SystemExit(f"rollback failed ({exc}); previous container restored, health {wait_health(target, 60)}")
        raise
    result = {"bak_status": bak_status, "failed_container": failed, "restored_container_id": restored["Id"],
              "health_ready_seconds": up, "outage_seconds": round(time.time() - t0, 1)}
    log(f"rollback done: {result}")
    return result


def _put_back(target, cur, cur_name):
    """回退失败时把原来的容器放回：占着名字的（回退容器）停掉、改回 *-bak，原容器改回原名、恢复重启策略并启动"""
    names = containers_by_name()
    holder = names.get(cur_name)
    if holder and holder["Id"] != cur["Id"]:
        call("POST", f"{D}/containers/{holder['Id']}/update", {"RestartPolicy": {"Name": "no"}})
        call("POST", f"{D}/containers/{holder['Id']}/stop?t=15")
        call("POST", f"{D}/containers/{holder['Id']}/rename?name={target.bak}")
    if containers_by_name().get(cur_name, {}).get("Id") != cur["Id"]:
        call("POST", f"{D}/containers/{cur['Id']}/rename?name={cur_name}")
    call("POST", f"{D}/containers/{cur['Id']}/update", {"RestartPolicy": cur["HostConfig"]["RestartPolicy"]})
    call("POST", f"{D}/containers/{cur['Id']}/start")


def revert_after_failed_swap(target, old_id, inspect_path, log):
    """execute_swap 中途失败时的自动回退：新容器已建 → rollback（含 ensure_bak）；新容器没建成 → 把回退容器放回原位启动。"""
    names = containers_by_name()
    if target.current in names and names[target.current]["Id"] != old_id:
        return rollback(target, inspect_path, log)
    if target.current not in names and target.bak in names:
        call("POST", f"{D}/containers/{target.bak}/rename?name={target.current}")
    restart = json.loads(pathlib.Path(inspect_path).read_text())["HostConfig"]["RestartPolicy"]
    call("POST", f"{D}/containers/{old_id}/update", {"RestartPolicy": restart})
    if not call("GET", f"{D}/containers/{old_id}/json")["State"]["Running"]:
        call("POST", f"{D}/containers/{old_id}/start")
    up = wait_health(target, 60)
    log(f"revert: old container back in place, health {up}")
    return {"reverted_in_place": True, "health_ready_seconds": up}
