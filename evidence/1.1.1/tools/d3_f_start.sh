#!/usr/bin/env bash
# F 组（最终候选 ≥6h 对照）：同一负载下"有写超时 A5"与"无写超时"各一只，2 核（与现网 NanoCpus=2 等价）：
#   c15 = v4 (1.1.1: A1+A5)      c16 = v1 (A1 only，无写超时)  <- 对照（预期增长）
# 负载 = 39 条订阅 + 抖动消费者 + OKX 每 20 分钟强制重连 + **停读注入**（每 20 分钟一条订阅全部主题的客户端停读 60 秒后断开，
# 模拟消费者端卡顿；本机偶发的整机卡顿 ~1–2 分钟也会造成同类事件，见 INVESTIGATION_REPORT D1）。
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-23400}"; INJECT_EVERY="${2:-1200}"; INJECT_HOLD="${3:-60}"
OUT=evidence/1.1.1/d3-load; SUBS=$OUT/prod-subscriptions-39.json; HOST=192.168.1.198
FORCE="--env MD_DEBUG_FORCE_RECONNECT_SECS=1200 --env MD_DEBUG_FORCE_RECONNECT_ONLY=okx"
spec=( "c15-v4-final exp-v4 28125 50125" "c16-v1-ctrl exp-v1 28126 50126" )
for s in "${spec[@]}"; do
  read -r name img port grpc <<<"$s"
  python3 evidence/1.1.1/tools/pt_exp.py run "md-exp-$name" "md-server:$img" $port $grpc $FORCE --cpus 2
  sleep 6
done
sleep 20
for s in "${spec[@]}"; do
  read -r name img port grpc <<<"$s"
  node scripts/compat/restore_subs.mjs $SUBS "http://$HOST:$port" --apply | tail -1
done
date -u +%FT%TZ > $OUT/f_started_at_utc.txt
for s in "${spec[@]}"; do
  read -r name img port grpc <<<"$s"; short=${name%%-*}
  node scripts/compat/sample_metrics.mjs "http://$HOST:$port" "$SECS" 60 "$OUT/$short.csv" --snapshot-every-min 60 > $OUT/logs/$short.sampler.log 2>&1 &
  node scripts/compat/load_consumers.mjs "ws://$HOST:$port/ws/v1/data" $SUBS "$SECS" "$OUT/logs/$short.consumers.log" --churn-period 90 --churn-n 4 --churn-off 20 > /dev/null 2>&1 &
  ( end=$(( $(date +%s) + SECS )); sleep 300
    while [ $(date +%s) -lt $end ]; do
      node scripts/compat/stall_client.mjs "$HOST:$port" $SUBS --hold "$INJECT_HOLD" --log "$OUT/logs/$short.stall.$(date -u +%H%M%S).log" > /dev/null 2>&1
      sleep "$INJECT_EVERY"
    done ) &
done
wait
date -u +%FT%TZ > $OUT/f_finished_at_utc.txt
