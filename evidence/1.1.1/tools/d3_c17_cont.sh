#!/usr/bin/env bash
# c17 续跑：c17 的原采样/负载进程（SECS=19000）在 ≈16:08Z 结束，而任务书 §四(2) 要求 ≥6h，
# 因此等原进程退出后，对**同一个容器**接着跑同样的负载与注入，采样写入 c17b.csv（分析时与 c17.csv 按时间拼接）。
# 不重建容器（容器状态/RSS 连续）。
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-3000}"; INJECT_EVERY="${2:-1200}"; INJECT_HOLD="${3:-60}"
OUT=evidence/1.1.1/d3-load; SUBS=$OUT/prod-subscriptions-39.json; HOST=192.168.1.198; P=28127
ORIG_SAMPLER_PID="${ORIG_SAMPLER_PID:-58827}"
while kill -0 "$ORIG_SAMPLER_PID" 2>/dev/null; do sleep 5; done
date -u +%FT%TZ > $OUT/c17b_started_at_utc.txt
node scripts/compat/sample_metrics.mjs "http://$HOST:$P" "$SECS" 60 "$OUT/c17b.csv" --snapshot-every-min 60 > $OUT/logs/c17b.sampler.log 2>&1 &
node scripts/compat/load_consumers.mjs "ws://$HOST:$P/ws/v1/data" $SUBS "$SECS" "$OUT/logs/c17b.consumers.log" --churn-period 90 --churn-n 4 --churn-off 20 > /dev/null 2>&1 &
( end=$(( $(date +%s) + SECS )); sleep 120
  while [ $(date +%s) -lt $end ]; do
    node scripts/compat/stall_client.mjs "$HOST:$P" $SUBS --hold "$INJECT_HOLD" --log "$OUT/logs/c17b.stall.$(date -u +%H%M%S).log" > /dev/null 2>&1
    sleep "$INJECT_EVERY"
  done ) &
wait
