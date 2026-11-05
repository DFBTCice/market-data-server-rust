#!/usr/bin/env bash
# c17 = v5 (A1+A2+A5 + jemalloc) 2 核，与 c15/c16 同样的负载（含每 20 分钟 60 秒停读注入）。补充对照，随 F 组一起结束。
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-19000}"; INJECT_EVERY="${2:-1200}"; INJECT_HOLD="${3:-60}"
OUT=evidence/1.1.1/d3-load; SUBS=$OUT/prod-subscriptions-39.json; HOST=192.168.1.198; P=28127
FORCE="--env MD_DEBUG_FORCE_RECONNECT_SECS=1200 --env MD_DEBUG_FORCE_RECONNECT_ONLY=okx"
python3 evidence/1.1.1/tools/pt_exp.py run md-exp-c17-v5-jem md-server:exp-v5 $P 50127 $FORCE --cpus 2
sleep 25
node scripts/compat/restore_subs.mjs $SUBS "http://$HOST:$P" --apply | tail -1
date -u +%FT%TZ > $OUT/c17_started_at_utc.txt
node scripts/compat/sample_metrics.mjs "http://$HOST:$P" "$SECS" 60 "$OUT/c17.csv" --snapshot-every-min 60 > $OUT/logs/c17.sampler.log 2>&1 &
node scripts/compat/load_consumers.mjs "ws://$HOST:$P/ws/v1/data" $SUBS "$SECS" "$OUT/logs/c17.consumers.log" --churn-period 90 --churn-n 4 --churn-off 20 > /dev/null 2>&1 &
( end=$(( $(date +%s) + SECS )); sleep 300
  while [ $(date +%s) -lt $end ]; do
    node scripts/compat/stall_client.mjs "$HOST:$P" $SUBS --hold "$INJECT_HOLD" --log "$OUT/logs/c17.stall.$(date -u +%H%M%S).log" > /dev/null 2>&1
    sleep "$INJECT_EVERY"
  done ) &
wait
