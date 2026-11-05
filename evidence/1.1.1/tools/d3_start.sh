#!/usr/bin/env bash
# D3：在 198 上起 7 只旁路容器（各 ≤256MiB / 1 核 / 无 swap），装入现网 39 条订阅，挂两类模拟消费者，逐分钟采样。
#   c0 = v0(1.1.0 网关逻辑+仪表) 抖动+周期重连   <- 正向对照（预期增长）
#   c1 = v1(1.1.1 修复)          抖动+周期重连   <- 候选（预期持平）
#   c2 = v2(1.1.0+jemalloc)      抖动+周期重连   <- D1：allocated vs resident
#   c3 = v3(1.1.1+jemalloc)      抖动+周期重连
#   c4 = v0                       仅抖动（无强制重连）  <- 归因
#   c5 = v0                       仅周期重连（无抖动）  <- 归因
#   c6 = v0                       都没有（安静基线）    <- 归因
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-25200}"
OUT=evidence/1.1.1/d3-load
SUBS=$OUT/prod-subscriptions-39.json
HOST=192.168.1.198
FORCE="--env MD_DEBUG_FORCE_RECONNECT_SECS=1200 --env MD_DEBUG_FORCE_RECONNECT_ONLY=okx"
mkdir -p $OUT/logs
# name image port grpc extra-env churn-n
spec=(
 "c0-v0-both     exp-v0 28101 50101 $FORCE 4"
 "c1-v1-both     exp-v1 28102 50102 $FORCE 4"
 "c2-v2-jemold   exp-v2 28103 50103 $FORCE 4"
 "c3-v3-jemnew   exp-v3 28104 50104 $FORCE 4"
 "c4-v0-churn    exp-v0 28105 50105 NONE 4"
 "c5-v0-reconn   exp-v0 28106 50106 $FORCE 0"
 "c6-v0-quiet    exp-v0 28107 50107 NONE 0"
)
for s in "${spec[@]}"; do
  read -r name img port grpc rest <<<"$s"
  extra=$(echo "$rest" | sed -E 's/ [0-9]+$//')
  [ "$extra" = "NONE" ] && extra=""
  python3 evidence/1.1.1/tools/pt_exp.py run "md-exp-$name" "md-server:$img" $port $grpc $extra
done
sleep 15
for s in "${spec[@]}"; do
  read -r name img port rest <<<"$s"
  curl -s -m 5 "http://$HOST:$port/health" >/dev/null || echo "WARN $name :$port unhealthy"
  node scripts/compat/restore_subs.mjs $SUBS "http://$HOST:$port" --apply | tail -1
done
date -u +%FT%TZ > $OUT/started_at_utc.txt
for s in "${spec[@]}"; do
  read -r name img port grpc rest <<<"$s"
  churn_n=$(echo "$rest" | grep -oE '[0-9]+$')
  short=${name%%-*}
  node scripts/compat/sample_metrics.mjs "http://$HOST:$port" "$SECS" 60 "$OUT/$short.csv" --snapshot-every-min 60 > $OUT/logs/$short.sampler.log 2>&1 &
  node scripts/compat/load_consumers.mjs "ws://$HOST:$port/ws/v1/data" $SUBS "$SECS" "$OUT/logs/$short.consumers.log" --churn-period 90 --churn-n $churn_n --churn-off 20 > /dev/null 2>&1 &
done
wait
date -u +%FT%TZ > $OUT/finished_at_utc.txt
echo "D3 done"
