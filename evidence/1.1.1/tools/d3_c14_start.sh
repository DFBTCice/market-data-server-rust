#!/usr/bin/env bash
# c14 = v4(1.1.1 + 写超时 A5，musl 默认分配器) 2 核，与 c11（v1，同构建但无写超时）使用完全相同的停读负载。
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-10800}"; STALL_AFTER="${2:-900}"; STALL_HOLD="${3:-1800}"
OUT=evidence/1.1.1/d3-load; SUBS=$OUT/prod-subscriptions-39.json; HOST=192.168.1.198; P=28124
FORCE="--env MD_DEBUG_FORCE_RECONNECT_SECS=1200 --env MD_DEBUG_FORCE_RECONNECT_ONLY=okx"
python3 evidence/1.1.1/tools/pt_exp.py run md-exp-c14-v4-stall md-server:exp-v4 $P 50124 $FORCE --cpus 2
sleep 20
node scripts/compat/restore_subs.mjs $SUBS "http://$HOST:$P" --apply | tail -1
date -u +%FT%TZ > $OUT/c14_started_at_utc.txt
node scripts/compat/sample_metrics.mjs "http://$HOST:$P" "$SECS" 60 "$OUT/c14.csv" --snapshot-every-min 60 > $OUT/logs/c14.sampler.log 2>&1 &
node scripts/compat/load_consumers.mjs "ws://$HOST:$P/ws/v1/data" $SUBS "$SECS" "$OUT/logs/c14.consumers.log" --churn-period 90 --churn-n 4 --churn-off 20 > /dev/null 2>&1 &
( sleep "$STALL_AFTER"; node scripts/compat/stall_client.mjs "$HOST:$P" $SUBS --hold "$STALL_HOLD" --log "$OUT/logs/c14.stall.log" > /dev/null 2>&1 ) &
wait
