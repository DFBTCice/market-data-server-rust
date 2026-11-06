#!/usr/bin/env python3
"""换版后 10 分钟验收（只读：GET 现网接口 + Portainer 容器信息与日志）。
判据：同一容器、健康、RestartCount=0、配置订阅（从 config.release.yaml 独立解析）全部还在、
缺失的运行期 TICK / QUOTE 都能对上 idle-unsubscribed 日志且 md_runtime_subscriptions_idle_unsubscribed_total 不小于已解释数、
md_ws_active_clients >= 2、开关为 1、jemalloc 指标在。
缺失且对不上日志的运行期 KLINE 不判失败，单列 needs_botd_confirmation（结论 pass_needs_botd_confirmation，不回退，交主审对照 botd 日志）。
退出码：0 = pass / pass_needs_botd_confirmation，1 = fail。
只记录不判失败：写超时（为 0 或附日志说明）、交易所重连增量（辅助）、消费者口径（post-swap-24h/prod.csv 前 10 分钟 ws_clients 最小值与变化次数）。
用法：python3 evidence/1.2.0/tools/check_after_swap.py            # 现网
      python3 evidence/1.2.0/tools/check_after_swap.py --drill <换版输出目录>"""
import csv
import datetime
import json
import pathlib
import re
import sys

import yaml

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import swaplib as S  # noqa: E402
from swaplib import D, call, demux, guard  # noqa: E402

DRILL = "--drill" in sys.argv
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
T = S.DRILL if DRILL else S.PROD
OUT = pathlib.Path(ARGS[0]) if DRILL else S.ROOT / "evidence/1.2.0/nas-198/swap"
CSV = S.ROOT / "evidence/1.2.0/nas-198/post-swap-24h/prod.csv"
MIN_CLIENTS = 0 if DRILL else 2
TYPES = {1: "TICK", 2: "KLINE", 3: "QUOTE"}


def key(exchange, data_type, symbol, interval):
    return (exchange.lower(), int(data_type), symbol.upper(), interval or "")


def config_keys(path):
    c = yaml.safe_load(path.read_text())["connectors"]
    keys = set()
    for ex in ("binance", "okx"):
        e = c.get(ex) or {}
        keys |= {key(ex, 1, s, "") for s in e.get("subscribe_ticks") or []}
        keys |= {key(ex, 2, s, iv) for iv, syms in (e.get("subscribe_klines") or {}).items() for s in syms}
        keys |= {key(ex, 3, s, "") for s in e.get("subscribe_quotes") or []}
    return keys


def reconnects(text):
    return {m.group(1): float(m.group(2)) for m in re.finditer(r'^md_connector_reconnect_total\{exchange="([^"]+)"\} (\S+)$', text, re.M)}


def idle_unsubscribed_keys(log_lines):
    keys = set()
    for line in log_lines:
        m = re.search(r"runtime subscription idle-unsubscribed: exchange=(\S+) type=(\S+) symbol=(\S+) interval=(\S+)", line)
        if m:
            dt = {v: k for k, v in TYPES.items()}[m.group(2)]
            keys.add(key(m.group(1), dt, m.group(3), "" if m.group(4) == "-" else m.group(4)))
    return keys


def judge_subscriptions(before, now_subs, log_lines, metrics, config_path):
    """配置订阅必须全在。换前有、现在没有的运行期订阅：有 idle-unsubscribed 日志的算已解释（退订计数须不小于已解释数）；
    没有日志的 TICK / QUOTE 判失败；没有日志的 KLINE 不判失败，单列 needs_botd_confirmation——REST DELETE 不打日志，
    botd 在 K 线图关闭约 60 秒后会 DELETE 自己登记的 K 线，须由主审对照 botd 日志「已释放行情服务端 K 线订阅」确认。"""
    now_keys = {key(s["exchange"], s["data_type"], s["symbol"], s.get("kline_interval")) for s in now_subs}
    expected_config = config_keys(config_path)
    before_keys = {key(s["exchange"], s["data_type"], s["symbol"], s.get("kline_interval")) for s in before}
    missing_config = sorted(expected_config - now_keys)
    missing_runtime = sorted(before_keys - now_keys - expected_config)
    logged = idle_unsubscribed_keys(log_lines)
    explained = [k for k in missing_runtime if k in logged]
    needs_botd = [k for k in missing_runtime if k not in logged and k[1] == 2]
    unexplained = [k for k in missing_runtime if k not in logged and k[1] != 2]
    idle_total = S.metric(metrics, "md_runtime_subscriptions_idle_unsubscribed_total") or 0
    return {
        "config_subscriptions_expected": len(expected_config), "missing_config": missing_config,
        "missing_runtime": missing_runtime, "missing_runtime_explained": explained,
        "missing_runtime_unexplained": unexplained, "needs_botd_confirmation": needs_botd,
        "idle_unsubscribed_total": idle_total,
        "config_subscriptions_present": not missing_config,
        "missing_tick_quote_explained_by_idle_unsubscribe": not unexplained and idle_total >= len(explained),
    }


def botd_note(needs_botd):
    return [f"{TYPES[k[1]]} {k[0]} {k[2]} {k[3]}" for k in needs_botd]


def main():
    guard()
    result = json.loads((OUT / "swap-result.json").read_text())
    assert result["status"] == "immediate_checks_passed", result
    swap_epoch = int(datetime.datetime.fromisoformat(result["started_at_utc"]).timestamp())
    before = json.loads((OUT / "subscriptions.before.json").read_text())
    c = call("GET", f"{D}/containers/{T.check_name(T.current)}/json")
    metrics = S.http_get(T, "/metrics").decode()
    now_subs = json.loads(S.http_get(T, "/api/v1/subscriptions"))
    raw_logs = call("GET", f"{D}/containers/{T.current}/logs?stdout=1&stderr=1&timestamps=1&since={swap_epoch}", want_raw=True)
    log_lines = [re.sub(r"\x1b\[[0-9;]*m", "", l) for l in demux(raw_logs).splitlines()]
    subs = judge_subscriptions(before, now_subs, log_lines, metrics, S.RELEASE_CONFIG)
    clients = S.metric(metrics, "md_ws_active_clients") or 0
    first = (OUT / "metrics.after.txt").read_text()
    consumers = None
    if not DRILL and CSV.exists():
        rows = [r for r in csv.DictReader(open(CSV)) if r["ws_clients"] != ""][:10]
        vals = [int(float(r["ws_clients"])) for r in rows]
        consumers = {"first_10_samples": vals, "min": min(vals) if vals else None,
                     "changes": sum(1 for a, b in zip(vals, vals[1:]) if a != b)}
    write_timeouts = S.metric(metrics, "md_ws_kicked_write_timeout_total") or 0
    checks = {
        "same_container": c["Id"] == result["new_container_id"],
        "healthy": c["State"].get("Health", {}).get("Status") == "healthy",
        "restart_count_0": c["RestartCount"] == 0,
        "config_subscriptions_present": subs["config_subscriptions_present"],
        "missing_tick_quote_explained_by_idle_unsubscribe": subs["missing_tick_quote_explained_by_idle_unsubscribe"],
        "ws_active_clients_ok": clients >= MIN_CLIENTS,
        "runtime_switch_on": S.metric(metrics, "md_runtime_subscriptions_enabled") == 1,
        "jemalloc_metrics": "\nmd_jemalloc_allocated_bytes " in metrics,
    }
    passed = all(checks.values())
    verdict = "fail" if not passed else ("pass_needs_botd_confirmation" if subs["needs_botd_confirmation"] else "pass")
    record = {
        "mode": T.mode, "checked_at_utc": S.now(), "verdict": verdict, "checks": checks, "pass": passed,
        "needs_botd_confirmation": botd_note(subs["needs_botd_confirmation"]),
        "config_subscriptions_expected": subs["config_subscriptions_expected"], "missing_config": subs["missing_config"],
        "subscriptions_before": len(before), "subscriptions_now": len(now_subs),
        "missing_runtime": subs["missing_runtime"], "missing_runtime_explained": subs["missing_runtime_explained"],
        "missing_runtime_unexplained": subs["missing_runtime_unexplained"],
        "idle_unsubscribed_total": subs["idle_unsubscribed_total"], "ws_active_clients": clients,
        "recorded_only": {
            "write_timeout_total": write_timeouts,
            "write_timeout_log_lines": [l for l in log_lines if "write blocked longer than ws_write_wait" in l],
            "exchange_reconnect_deltas_since_swap": {k: v - reconnects(first).get(k, 0) for k, v in reconnects(metrics).items()},
            "consumers_from_post_swap_csv": consumers,
        },
    }
    (OUT / "ten-minute.json").write_text(json.dumps(record, indent=2) + "\n")
    (OUT / "metrics.ten-minute.txt").write_text(metrics)
    print(json.dumps(record, indent=2, ensure_ascii=False))
    if verdict == "pass_needs_botd_confirmation":
        print(f"VERDICT pass_needs_botd_confirmation: 不回退。以下运行期 K 线缺失、没有闲置退订日志，可能是 botd 释放了自己的订阅，"
              f"请把清单交业务方转主审对照 botd 日志「已释放行情服务端 K 线订阅」核对：{record['needs_botd_confirmation']}")
    else:
        print(f"VERDICT {verdict}")
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
