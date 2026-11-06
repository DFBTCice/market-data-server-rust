#!/usr/bin/env python3
"""一级回退：不重启，经 Portainer API 改容器内配置的一个开关后发 SIGHUP（OPS_GUIDE §8.6 的 API 版）。
步骤：guard → 读回配置 → exec（root）sed 只改该键 → 读回配置断言只有这一行变化 → kill?signal=HUP → 读回指标与日志确认、进程未重启。
用法：
  现网：APPROVED="<批准人与时间>" python3 evidence/1.2.0/tools/runtime_toggle.py <idle_unsubscribe|on_demand_quote> <true|false> [输出目录]
  演练：python3 evidence/1.2.0/tools/runtime_toggle.py --drill <键> <值> [输出目录]      # 只操作 md-exp-120-drill
"""
import difflib
import io
import json
import os
import pathlib
import re
import sys
import tarfile
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import swaplib as S  # noqa: E402
from swaplib import D, call, demux, guard  # noqa: E402

DRILL = "--drill" in sys.argv
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
key, value = ARGS[0], ARGS[1]
assert key in ("idle_unsubscribe", "on_demand_quote") and value in ("true", "false"), "只允许改 idle_unsubscribe / on_demand_quote 为 true / false"
T = S.DRILL if DRILL else S.PROD
OUT = pathlib.Path(ARGS[2]) if len(ARGS) > 2 else S.ROOT / "evidence/1.2.0/nas-198" / ("drill" if DRILL else "swap") / f"toggle-{key}-{value}"
OUT.mkdir(parents=True, exist_ok=True)
CONFIG = "/etc/md-server/config.yaml"
if not DRILL:
    assert os.environ.get("APPROVED", "").strip(), "现网一级回退须设置 APPROVED=<批准人与时间>"


def exec_root(name, cmd):
    ex = call("POST", f"{D}/containers/{name}/exec", {"AttachStdout": True, "AttachStderr": True, "User": "root", "Cmd": cmd})
    raw = call("POST", f"{D}/exec/{ex['Id']}/start", {"Detach": False, "Tty": False}, want_raw=True)
    info = call("GET", f"{D}/exec/{ex['Id']}/json")
    return info["ExitCode"], demux(raw)


def put_config(name, data):
    """把配置原样写回（root:root 0644），用于 SIGHUP 之前任何一步失败时撤销 sed 的改动"""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        entry = tarfile.TarInfo(pathlib.PurePosixPath(CONFIG).name)
        entry.size, entry.mode, entry.uid, entry.gid = len(data), 0o644, 0, 0
        tar.addfile(entry, io.BytesIO(data))
    call("PUT", f"{D}/containers/{name}/archive?path={pathlib.PurePosixPath(CONFIG).parent}",
         raw_body=buf.getvalue(), ctype="application/x-tar")
    assert S.read_file(name, CONFIG) == data, "写回原配置后读回不一致"


def expected_diff(cfg, key, value):
    m = re.search(rf"^([ \t]*{key}:[ \t]*)(true|false)[ \t]*$", cfg, re.M)
    return [] if m.group(2) == value else [f"-{m.group(0)}", f"+{m.group(1)}{value}"]


guard()
name = T.check_name(T.current)
before_inspect = call("GET", f"{D}/containers/{name}/json")
assert before_inspect["State"]["Running"], f"{name} 没在运行"
metrics_before = S.http_get(T, "/metrics").decode()
start_before = S.metric(metrics_before, "process_start_time_seconds")
cfg_before = S.read_file(name, CONFIG).decode()
assert len(re.findall(rf"^[ \t]*{key}:[ \t]*(?:true|false)[ \t]*$", cfg_before, re.M)) == 1, f"配置里 {key} 行不是恰好一行"
expected = expected_diff(cfg_before, key, value)
since = int(time.time()) - 1
try:
    code, out = exec_root(name, ["sed", "-i", rf"s/^\([ \t]*{key}:[ \t]*\)\(true\|false\)[ \t]*$/\1{value}/", CONFIG])
    assert code == 0, f"sed 失败 exit={code}: {out}"
    cfg_after = S.read_file(name, CONFIG).decode()
    diff = [l for l in difflib.unified_diff(cfg_before.splitlines(), cfg_after.splitlines(), lineterm="", n=0)
            if l.startswith(("+", "-")) and not l.startswith(("+++", "---"))]
    assert diff == expected, f"配置改动与预期不符：{diff} != {expected}"
except BaseException:
    put_config(name, cfg_before.encode())
    print("config restored to its previous content; no SIGHUP sent", file=sys.stderr)
    raise
call("POST", f"{D}/containers/{name}/kill?signal=HUP")
time.sleep(3)
metrics_after = S.http_get(T, "/metrics").decode()
after_inspect = call("GET", f"{D}/containers/{name}/json")
raw_logs = call("GET", f"{D}/containers/{name}/logs?stdout=1&stderr=1&timestamps=1&since={since}", want_raw=True)
logs = [re.sub(r"\x1b\[[0-9;]*m", "", l) for l in demux(raw_logs).splitlines() if "md_runtime_subs" in l or "runtime_subscriptions" in l]
enabled = S.metric(metrics_after, "md_runtime_subscriptions_enabled")
checks = {
    "config_only_that_line_changed": True,
    "process_not_restarted": S.metric(metrics_after, "process_start_time_seconds") == start_before
                             and after_inspect["RestartCount"] == before_inspect["RestartCount"],
    "enabled_metric_matches": key != "idle_unsubscribe" or enabled == (1 if value == "true" else 0),
    "module_log_confirms": key != "idle_unsubscribe" or any(("ENABLED" if value == "true" else "DISABLED") in l for l in logs)
                           or not expected,
    "settings_log_confirms": key != "on_demand_quote" or any(f"on_demand_quote={value}" in l for l in logs) or not expected,
}
record = {"mode": T.mode, "container": name, "key": key, "value": value, "at_utc": S.now(), "config_diff": diff,
          "md_runtime_subscriptions_enabled": enabled, "process_start_time_seconds": start_before,
          "restart_count": after_inspect["RestartCount"], "log_lines": logs, "checks": checks, "pass": all(checks.values())}
(OUT / "toggle-result.json").write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n")
print(json.dumps(record, indent=1, ensure_ascii=False))
sys.exit(0 if record["pass"] else 1)
