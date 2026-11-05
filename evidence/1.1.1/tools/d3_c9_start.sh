#!/usr/bin/env bash
# c9：原版 1.0.0 二进制（md-server:1.0.0-rollback）在同样的"抖动"负载下的对照，回答"1.1.0 为何比 1.0.0 涨得快"。
# 1.0.0 不支持 QUOTE：订阅集合里的 2 条 QUOTE 补不回（restore_subs 只补 TICK/KLINE），消费者加 --no-quote。
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-21000}"; OUT=evidence/1.1.1/d3-load; HOST=192.168.1.198; P=28113
node scripts/compat/sample_metrics.mjs "http://$HOST:$P" "$SECS" 60 "$OUT/c9.csv" --snapshot-every-min 60 > $OUT/logs/c9.sampler.log 2>&1 &
node scripts/compat/load_consumers.mjs "ws://$HOST:$P/ws/v1/data" $OUT/prod-subscriptions-39.json "$SECS" "$OUT/logs/c9.consumers.log" --churn-period 90 --churn-n 4 --churn-off 20 --no-quote > /dev/null 2>&1 &
wait
