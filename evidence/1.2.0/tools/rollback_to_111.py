#!/usr/bin/env python3
"""二级回退：切回 1.1.1。
步骤：GET 当前订阅存档 → swaplib.rollback（先确认 / 按旧容器 inspect 重建回退容器，不通过则不停当前容器）→
restore_subs.mjs <刚存的订阅> → 核对 process_start_time_seconds 已变化、md_runtime_subscriptions* 指标消失、镜像为 1.1.1。
用法：
  现网：APPROVED="<批准人与时间>" python3 evidence/1.2.0/tools/rollback_to_111.py [输出目录]     # inspect 取 nas-198/swap/old-container.inspect.json
  演练：python3 evidence/1.2.0/tools/rollback_to_111.py --drill <输出目录> <旧容器 inspect.json>
"""
import json
import os
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import swaplib as S  # noqa: E402
from swaplib import D, call, guard  # noqa: E402

DRILL = "--drill" in sys.argv
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
T = S.DRILL if DRILL else S.PROD
NAS = S.ROOT / "evidence/1.2.0/nas-198"
OUT = pathlib.Path(ARGS[0]) if ARGS else NAS / "swap" / f"rollback-{S.now()[:19].replace(':', '')}"
INSPECT = pathlib.Path(ARGS[1]) if DRILL else NAS / "swap/old-container.inspect.json"
OUT.mkdir(parents=True, exist_ok=True)
if not DRILL:
    assert os.environ.get("APPROVED", "").strip(), "现网二级回退须设置 APPROVED=<批准人与时间>"


def log(msg):
    line = f"{S.now()} {msg}"
    print(line, flush=True)
    with open(OUT / "rollback.log", "a") as f:
        f.write(line + "\n")


guard()
assert INSPECT.exists(), f"缺少旧容器 inspect：{INSPECT}"
before_metrics = S.http_get(T, "/metrics").decode()
subs = S.http_get(T, "/api/v1/subscriptions")
(OUT / "subscriptions.archived.json").write_bytes(subs)
(OUT / "metrics.before.txt").write_text(before_metrics)
start_before = S.metric(before_metrics, "process_start_time_seconds")
log(f"archived {len(json.loads(subs))} subscriptions from {T.current}; process_start_time_seconds={start_before}")


def write_record(record):
    (OUT / "rollback-result.json").write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n")
    log(f"rollback result: {json.dumps(record, ensure_ascii=False)}")


def restore_archived():
    """把刚存档的订阅补到现在占着名字的容器上（回退成功是 1.1.1；回退失败、原容器放回时是放回的容器）"""
    with open(OUT / "restore.log", "w") as f:
        return subprocess.run(["node", "scripts/compat/restore_subs.mjs", str(OUT / "subscriptions.archived.json"),
                               T.url(""), "--apply"], cwd=S.ROOT, stdout=f, stderr=subprocess.STDOUT, timeout=60).returncode


def safe(fn, default=None):
    try:
        return fn()
    except Exception:
        return default


try:
    result = S.rollback(T, INSPECT, log)
except S.RollbackFailed as exc:
    restore_code = safe(restore_archived, "error") if exc.restored else None
    after_metrics = safe(lambda: S.http_get(T, "/metrics").decode(), "")
    write_record({"mode": T.mode, "at_utc": S.now(), "status": "rollback_failed_previous_restored" if exc.restored
                  else "rollback_failed_manual_recovery_needed", "error": str(exc),
                  "restore_subs_exit": restore_code, "process_start_time_before": start_before,
                  "process_start_time_after": S.metric(after_metrics, "process_start_time_seconds"),
                  "healthy_now": safe(lambda: S.wait_health(T, 5)) is not None, "pass": False})
    raise
except BaseException as exc:
    write_record({"mode": T.mode, "at_utc": S.now(), "status": "refused_before_touching_current", "error": repr(exc),
                  "process_start_time_before": start_before, "pass": False})
    raise
restore_code = restore_archived()
after_metrics = S.http_get(T, "/metrics").decode()
(OUT / "metrics.after.txt").write_text(after_metrics)
current = call("GET", f"{D}/containers/{T.current}/json")
checks = {
    "image_is_111": current["Image"] == S.OLD_IMAGE_ID,
    "process_start_time_changed": S.metric(after_metrics, "process_start_time_seconds") != start_before,
    "runtime_metrics_gone": "md_runtime_subscriptions" not in after_metrics,
    "restore_subs_ok": restore_code == 0,
    "healthy_now": S.wait_health(T, 5) is not None,
}
record = {"mode": T.mode, "at_utc": S.now(), "status": "rolled_back", **result, "process_start_time_before": start_before,
          "process_start_time_after": S.metric(after_metrics, "process_start_time_seconds"),
          "subscriptions_after": len(json.loads(S.http_get(T, "/api/v1/subscriptions"))), "checks": checks,
          "pass": all(checks.values())}
write_record(record)
sys.exit(0 if record["pass"] else 1)
