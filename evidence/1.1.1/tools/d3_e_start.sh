#!/usr/bin/env bash
# D3 补充实验（E 组）：
#  根因调查发现首轮 7 只旁路容器（1 核）与现网（NanoCpus=2 → tokio 2 个工作线程）不等价，
#  且"停读客户端"（服务端 send 永久阻塞、broadcast 环滞留）是 D2 的另一条候选路径，故补充：
#   c10 = v3(1.1.1+jemalloc) 2 核  + 20min 后停读客户端 30min 再放开   <- D1/D2：滞留的是 allocated 还是 resident
#   c11 = v1(1.1.1,musl 默认分配器) 2 核 + 同样的停读客户端          <- 释放后 RSS 是否回落（碎片）
#   c12 = v0(1.1.0 网关逻辑) 2 核 抖动+重连                          <- 现网等价（2 工作线程）的正对照
#   c13 = v1(1.1.1) 2 核 抖动+重连                                   <- 候选
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-14400}"; STALL_AFTER="${2:-1200}"; STALL_HOLD="${3:-1800}"
OUT=evidence/1.1.1/d3-load; SUBS=$OUT/prod-subscriptions-39.json; HOST=192.168.1.198
FORCE="--env MD_DEBUG_FORCE_RECONNECT_SECS=1200 --env MD_DEBUG_FORCE_RECONNECT_ONLY=okx"
mkdir -p $OUT/logs
spec=(
 "c10-v3-stall   exp-v3 28120 50120 $FORCE stall"
 "c11-v1-stall   exp-v1 28121 50121 $FORCE stall"
 "c12-v0-2cpu    exp-v0 28122 50122 $FORCE none"
 "c13-v1-2cpu    exp-v1 28123 50123 $FORCE none"
)
# SKIP_CREATE=1：容器与订阅已存在，只重新挂采样器/消费者（第一次从带 & 的前台命令启动时被工具回收，故补此开关）
if [ "${SKIP_CREATE:-0}" != 1 ]; then
for s in "${spec[@]}"; do
  read -r name img port grpc rest <<<"$s"
  extra=$(echo "$rest" | sed -E 's/ (stall|none)$//')
  python3 evidence/1.1.1/tools/pt_exp.py run "md-exp-$name" "md-server:$img" $port $grpc $extra --cpus 2
  sleep 6   # 错开交易所建连
done
sleep 15
for s in "${spec[@]}"; do
  read -r name img port rest <<<"$s"
  curl -s -m 5 "http://$HOST:$port/health" >/dev/null || echo "WARN $name :$port unhealthy"
  node scripts/compat/restore_subs.mjs $SUBS "http://$HOST:$port" --apply | tail -1
done
fi
date -u +%FT%TZ > $OUT/e_started_at_utc.txt
for s in "${spec[@]}"; do
  read -r name img port grpc rest <<<"$s"
  mode=${rest##* }
  short=${name%%-*}
  node scripts/compat/sample_metrics.mjs "http://$HOST:$port" "$SECS" 60 "$OUT/$short.csv" --snapshot-every-min 60 > $OUT/logs/$short.sampler.log 2>&1 &
  node scripts/compat/load_consumers.mjs "ws://$HOST:$port/ws/v1/data" $SUBS "$SECS" "$OUT/logs/$short.consumers.log" --churn-period 90 --churn-n 4 --churn-off 20 > /dev/null 2>&1 &
  if [ "$mode" = stall ]; then
    ( sleep "$STALL_AFTER"; node scripts/compat/stall_client.mjs "$HOST:$port" $SUBS --hold "$STALL_HOLD" --log "$OUT/logs/$short.stall.log" > /dev/null 2>&1 ) &
  fi
done
wait
date -u +%FT%TZ > $OUT/e_finished_at_utc.txt
echo "E group done"
