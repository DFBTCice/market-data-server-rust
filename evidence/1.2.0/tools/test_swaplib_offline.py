#!/usr/bin/env python3
"""swaplib 回退逻辑与 10 分钟验收判据的离线测试（假 Portainer，不联网、不需要 JWT）。覆盖 198 演练不便制造的失败分支：
回退容器镜像 / 参数不对、正在运行 → 不动当前容器；回退容器不存在 → 先重建再切换，重建后核对不通过 → 删掉；
停容器的调用报错（容器实际已停 / 未停）→ 恢复重启策略、需要时启动；回退后的 1.1.1 起不来或不健康 → 把原容器放回；
放回也失败 → 报「须人工兜底」；两种模式的名字互不越界；复制标签时去掉镜像来源标签；固定期望 sha 与 dist-sha256.txt 一致；
10 分钟验收：配置订阅缺失、无法解释的 TICK / QUOTE 缺失判失败，无法解释的运行期 KLINE 单列待 botd 日志确认。
假 Docker 与真实 Docker + pt_exp 一致：停已停的容器、启动已在运行的容器返回 304，调用方不允许 304 时报错。
用法：python3 evidence/1.2.0/tools/test_swaplib_offline.py"""
import copy
import json
import pathlib
import sys
import tempfile
import traceback
import types

fake = types.ModuleType("pt_exp")
fake.D = "/endpoints/3/docker"
fake.demux = lambda raw: raw.decode() if isinstance(raw, bytes) else raw
fake.guard = lambda: None
fake.call = lambda *a, **k: (_ for _ in ()).throw(AssertionError("fake Portainer not installed"))
sys.modules["pt_exp"] = fake
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import swaplib as S  # noqa: E402

OLD, NEW = S.OLD_IMAGE_ID, "sha256:" + "1" * 64


class FakeDocker:
    def __init__(self, containers, healthy_images=(OLD, NEW)):
        self.c = {cid: dict(v, Id=cid) for cid, v in containers.items()}
        self.healthy_images = healthy_images
        self.ops = []
        self.seq = 0
        self.failures = []  # [(操作, 当时的容器名, "before" | "after")]，按顺序各用一次
        self.create_env = None  # 模拟重建出来的容器参数与请求不符

    def find(self, ref):
        for cid, v in self.c.items():
            if ref in (cid, v["Name"]):
                return v
        raise SystemExit(f"HTTP 404 {ref}")

    def inspect(self, v):
        return {"Id": v["Id"], "Image": v["Image"], "RestartCount": 0,
                "State": {"Running": v["Running"]},
                "Config": {"Env": v["Env"], "Cmd": v.get("Cmd", ["md-server"]), "Entrypoint": None,
                           "User": v.get("User", "md-user"), "ExposedPorts": {"8081/tcp": {}},
                           "Labels": copy.deepcopy(v["Labels"])},
                "HostConfig": copy.deepcopy(v["HostConfig"])}

    def injected(self, op, name):
        for i, (o, n, mode) in enumerate(self.failures):
            if o == op and n == name:
                del self.failures[i]
                return mode
        return None

    def call(self, method, path, body=None, ok=(200, 201, 204), **kw):
        p = path.replace(fake.D, "")
        self.ops.append((method, p))
        if method == "GET" and p.startswith("/containers/json"):
            return [{"Id": v["Id"], "Names": ["/" + v["Name"]]} for v in self.c.values()]
        if method == "GET" and p.startswith("/images/"):
            return {"Id": OLD}
        if method == "GET" and p.endswith("/json"):
            return self.inspect(self.find(p.split("/")[2]))
        if method == "POST" and p.startswith("/containers/create"):
            self.seq += 1
            cid = f"new{self.seq}"
            name = p.split("name=")[1]
            assert all(o["Name"] != name for o in self.c.values()), f"name {name} taken"
            image = OLD if body["Image"] == S.OLD_IMAGE_TAG else NEW
            self.c[cid] = {"Id": cid, "Name": name, "Image": image, "Running": False,
                           "Env": self.create_env or body["Env"], "Cmd": body["Cmd"], "User": body["User"],
                           "Labels": body["Labels"], "HostConfig": copy.deepcopy(body["HostConfig"])}
            return {"Id": cid}
        v = self.find(p.split("/")[2].split("?")[0])
        if method == "DELETE":
            del self.c[v["Id"]]
            return None
        op = next(o for o in ("start", "stop", "rename", "update") if f"/{o}" in p)
        mode = self.injected(op, v["Name"])
        if mode == "before":
            raise SystemExit(f"HTTP 500 {method} {p} (injected)")
        if op == "start":
            if v["Running"] and 304 not in ok:
                raise SystemExit(f"HTTP 304 {method} {p}")
            v["Running"] = True
        elif op == "stop":
            if not v["Running"] and 304 not in ok:
                raise SystemExit(f"HTTP 304 {method} {p}")
            v["Running"] = False
        elif op == "rename":
            new = p.split("name=")[1]
            assert all(o["Name"] != new for o in self.c.values()), f"name {new} taken"
            v["Name"] = new
        elif op == "update":
            v["HostConfig"]["RestartPolicy"] = body["RestartPolicy"]
        if mode == "after":
            raise SystemExit(f"HTTP 500 {method} {p} (injected after the change took effect)")
        return None

    def running(self, name):
        return next((v for v in self.c.values() if v["Name"] == name and v["Running"]), None)

    def touched_current(self):
        return any(m == "POST" and ("/stop" in p or "/rename" in p) for m, p in self.ops)


def install(fd, target):
    S.call = fd.call
    S.guard = lambda: None
    S.wait_health = lambda t, limit=60: 1.0 if (v := fd.running(target.current)) and v["Image"] in fd.healthy_images else None


HC = {"PortBindings": {"8081/tcp": [{"HostPort": "8081"}]}, "RestartPolicy": {"Name": "unless-stopped"},
      "Memory": 512, "MemorySwap": 1024, "NanoCpus": 2, "NetworkMode": "bridge", "LogConfig": {"Type": "json-file"}}
ENV = ["RUST_LOG=warn", "TZ=Asia/Shanghai", "PATH=/usr/bin"]


def world(bak_image=OLD, bak_running=False, with_bak=True, names=("md-server", "md-server-1.1.1-bak"), bak_cmd=None):
    cs = {"cur": {"Name": names[0], "Image": NEW, "Running": True, "Env": ENV, "Labels": {}, "HostConfig": copy.deepcopy(HC)}}
    if with_bak:
        cs["bak"] = {"Name": names[1], "Image": bak_image, "Running": bak_running, "Env": ENV,
                     "Cmd": bak_cmd or ["md-server"], "Labels": {},
                     "HostConfig": dict(copy.deepcopy(HC), RestartPolicy={"Name": "no"})}
    return FakeDocker(cs)


def inspect_file(tmp):
    old = {"Id": "oldcontainer123", "Image": OLD, "Config": {"Env": ENV, "Cmd": ["md-server"], "Entrypoint": None,
           "User": "md-user", "ExposedPorts": {"8081/tcp": {}},
           "Labels": {"md-server.binary.sha256": "3ffa…", "org.opencontainers.image.revision": "a06fe20", "keep": "yes"}},
           "HostConfig": copy.deepcopy(HC)}
    path = pathlib.Path(tmp) / "old.json"
    path.write_text(json.dumps(old))
    return path


results = {}
logs = []


def case(name):
    def run(fn):
        try:
            results[name] = bool(fn())
        except BaseException:
            traceback.print_exc(file=sys.stderr)
            results[name] = False
    return run


def current_back_in_place(fd):
    cur = fd.c["cur"]
    return (cur["Running"] and cur["Name"] == "md-server"
            and cur["HostConfig"]["RestartPolicy"] == {"Name": "unless-stopped"})


def raises(exc_type, fn):
    try:
        fn()
    except exc_type as exc:
        return exc
    return None


tmp = tempfile.TemporaryDirectory()
insp = inspect_file(tmp.name)


@case("bak_present_normal_path")
def _():
    fd = world(); install(fd, S.PROD)
    r = S.rollback(S.PROD, insp, logs.append)
    return (r["bak_status"] == "present" and fd.running("md-server")["Image"] == OLD
            and any(v["Name"].endswith("-failed") and v["Image"] == NEW for v in fd.c.values()))


for _name, _kwargs in {"bak_wrong_image_aborts_before_touching_current": {"bak_image": NEW},
                       "bak_running_aborts_before_touching_current": {"bak_running": True},
                       "bak_wrong_cmd_aborts_before_touching_current": {"bak_cmd": ["sh"]}}.items():
    @case(_name)
    def _(kwargs=_kwargs):
        fd = world(**kwargs); install(fd, S.PROD)
        exc = raises(AssertionError, lambda: S.rollback(S.PROD, insp, logs.append))
        return exc is not None and fd.c["cur"]["Running"] and fd.c["cur"]["Name"] == "md-server" and not fd.touched_current()


@case("bak_missing_recreated_then_switched")
def _():
    fd = world(with_bak=False); install(fd, S.PROD)
    r = S.rollback(S.PROD, insp, logs.append)
    recreated = next(v for v in fd.c.values() if v["Id"].startswith("new"))
    create_idx = next(i for i, (m, p) in enumerate(fd.ops) if p.startswith("/containers/create"))
    stop_idx = next(i for i, (m, p) in enumerate(fd.ops) if "/stop" in p)
    return (r["bak_status"] == "recreated" and fd.running("md-server")["Id"] == recreated["Id"]
            and recreated["Image"] == OLD and recreated["Env"] == ENV
            and "md-server.binary.sha256" not in recreated["Labels"] and recreated["Labels"].get("keep") == "yes"
            and recreated["HostConfig"]["RestartPolicy"] == {"Name": "unless-stopped"}
            and create_idx < stop_idx)


@case("recreated_bak_mismatch_is_deleted_and_current_untouched")
def _():
    fd = world(with_bak=False); fd.create_env = ["RUST_LOG=debug"]; install(fd, S.PROD)
    exc = raises(AssertionError, lambda: S.rollback(S.PROD, insp, logs.append))
    return (exc is not None and not any(v["Name"] == "md-server-1.1.1-bak" for v in fd.c.values())
            and ("DELETE", "/containers/new1?force=1") in fd.ops and fd.c["cur"]["Running"] and not fd.touched_current())


@case("unhealthy_111_puts_previous_back")
def _():
    fd = world(); fd.healthy_images = (NEW,); install(fd, S.PROD)
    exc = raises(S.RollbackFailed, lambda: S.rollback(S.PROD, insp, logs.append))
    return (exc is not None and exc.restored and current_back_in_place(fd)
            and fd.c["bak"]["Name"] == "md-server-1.1.1-bak" and not fd.c["bak"]["Running"])


@case("start_of_111_fails_puts_previous_back")
def _():
    fd = world(); fd.failures = [("start", "md-server", "before")]; install(fd, S.PROD)
    exc = raises(S.RollbackFailed, lambda: S.rollback(S.PROD, insp, logs.append))
    return (exc is not None and exc.restored and current_back_in_place(fd)
            and fd.c["bak"]["Name"] == "md-server-1.1.1-bak" and not fd.c["bak"]["Running"])


@case("stop_call_errors_after_stopping_restarts_current")
def _():
    fd = world(); fd.failures = [("stop", "md-server", "after")]; install(fd, S.PROD)
    exc = raises(S.RollbackFailed, lambda: S.rollback(S.PROD, insp, logs.append))
    return (exc is not None and exc.restored and current_back_in_place(fd)
            and fd.c["bak"]["Name"] == "md-server-1.1.1-bak" and not fd.c["bak"]["Running"])


@case("stop_call_errors_before_stopping_restores_restart_policy")
def _():
    fd = world(); fd.failures = [("stop", "md-server", "before")]; install(fd, S.PROD)
    exc = raises(S.RollbackFailed, lambda: S.rollback(S.PROD, insp, logs.append))
    return (exc is not None and exc.restored and current_back_in_place(fd)
            and not any(m == "POST" and p == "/containers/cur/start" for m, p in fd.ops))


@case("put_back_failure_reports_manual_recovery")
def _():
    fd = world(); fd.failures = [("start", "md-server", "before"), ("start", "md-server", "before")]; install(fd, S.PROD)
    exc = raises(S.RollbackFailed, lambda: S.rollback(S.PROD, insp, logs.append))
    return exc is not None and not exc.restored and "SWAP_PLAN §4.3" in str(exc)


@case("drill_mode_uses_only_drill_names")
def _():
    fd = world(names=("md-exp-120-drill", "md-exp-120-drill-bak")); install(fd, S.DRILL)
    r = S.rollback(S.DRILL, insp, logs.append)
    return (r["failed_container"].startswith("md-exp-120-drill-")
            and all(v["Name"].startswith("md-exp-120-drill") for v in fd.c.values()))


def refused(target, name):
    try:
        target.check_name(name)
        return False
    except AssertionError:
        return True


results["prod_refuses_drill_names"] = all(refused(S.PROD, n) for n in ("md-exp-120-drill", "md-exp-120-drill-bak", "md-exp-120-old"))
results["drill_refuses_prod_names"] = all(refused(S.DRILL, n) for n in ("md-server", "md-server-1.1.1-bak", "md-server-1-failed"))
results["prod_allows_its_names"] = not any(refused(S.PROD, n) for n in ("md-server", "md-server-1.1.1-bak", "md-server-1793-failed"))
results["strip_image_labels"] = S.strip_image_labels({"md-server.binary.sha256": "x", "md-server.config.sha256": "y",
    "org.opencontainers.image.revision": "z", "purpose": "p"}) == {"purpose": "p"}
results["expected_sha_matches_dist_record"] = S.expected_sha() == "cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf"

# 10 分钟验收的订阅判据，用 198 金丝雀复跑（short：闲置 2 分钟、确有闲置退订）的真实数据
import check_after_swap as C  # noqa: E402

RERUN = S.ROOT / "evidence/1.2.0/nas-198/canary-rerun"
before = json.loads((RERUN / "subscriptions.before.json").read_text())
final = json.loads((RERUN / "short.subscriptions.final.json").read_text())
lines = (RERUN / "short.container.log").read_text().splitlines()
metrics = (RERUN / "short.metrics.final.txt").read_text()
cfg = RERUN / "short.config.yaml"
real = C.judge_subscriptions(before, final, lines, metrics, cfg)
TICK_SNDK, KLINE_BNB_5M = ("binance", 1, "SNDKUSDT", ""), ("binance", 2, "BNBUSDT", "5m")


def without_idle_line(k):
    return [l for l in lines if not ("idle-unsubscribed" in l and f"type={C.TYPES[k[1]]} symbol={k[2]} " in l
                                    and f"interval={k[3] or '-'} " in l)]


def ok(j):
    return j["config_subscriptions_present"] and j["missing_tick_quote_explained_by_idle_unsubscribe"]


@case("ten_minute_real_canary_data_passes")
def _():
    return (ok(real) and len(real["missing_runtime"]) == 5 and real["missing_runtime_explained"] == real["missing_runtime"]
            and not real["needs_botd_confirmation"] and TICK_SNDK in real["missing_runtime"]
            and KLINE_BNB_5M in real["missing_runtime"])


@case("ten_minute_unexplained_tick_fails")
def _():
    j = C.judge_subscriptions(before, final, without_idle_line(TICK_SNDK), metrics, cfg)
    return not ok(j) and j["missing_runtime_unexplained"] == [TICK_SNDK] and not j["needs_botd_confirmation"]


@case("ten_minute_unexplained_quote_fails")
def _():
    quote = {"exchange": "binance", "data_type": 3, "symbol": "XRPUSDT", "kline_interval": None}
    j = C.judge_subscriptions(before + [quote], final, lines, metrics, cfg)
    return not ok(j) and j["missing_runtime_unexplained"] == [("binance", 3, "XRPUSDT", "")]


@case("ten_minute_unexplained_runtime_kline_needs_botd_not_fail")
def _():
    j = C.judge_subscriptions(before, final, without_idle_line(KLINE_BNB_5M), metrics, cfg)
    return ok(j) and j["needs_botd_confirmation"] == [KLINE_BNB_5M] and not j["missing_runtime_unexplained"]


@case("ten_minute_counter_below_explained_fails")
def _():
    low = metrics.replace(f"md_runtime_subscriptions_idle_unsubscribed_total {real['idle_unsubscribed_total']:g}",
                          "md_runtime_subscriptions_idle_unsubscribed_total 0")
    return not ok(C.judge_subscriptions(before, final, lines, low, cfg))


@case("ten_minute_missing_config_kline_fails")
def _():
    config_kline = sorted(k for k in C.config_keys(cfg) if k[1] == 2)[0]
    now = [s for s in final if C.key(s["exchange"], s["data_type"], s["symbol"], s.get("kline_interval")) != config_kline]
    j = C.judge_subscriptions(before, now, lines, metrics, cfg)
    return not j["config_subscriptions_present"] and config_kline not in j["needs_botd_confirmation"]


print(json.dumps({"real_canary_missing_runtime": real["missing_runtime"],
                  "idle_unsubscribed_total": real["idle_unsubscribed_total"]}), file=sys.stderr)
tmp.cleanup()
print(json.dumps(results, indent=1))
sys.exit(0 if all(results.values()) else 1)
