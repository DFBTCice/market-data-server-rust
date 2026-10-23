#!/usr/bin/env bash
# D7：ping_interval 15s 对照组（c7/c8，1.1.1 二进制，无强制重连、无 churn）；基线组 = D3 的 c4/c6（25s，自然重连）。
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-23000}"; OUT=evidence/1.1.1/d3-load; HOST=192.168.1.198
SUBS=$OUT/prod-subscriptions-39.json
for pair in c7:28111 c8:28112; do
  n=${pair%%:*}; p=${pair##*:}
  node scripts/compat/sample_metrics.mjs "http://$HOST:$p" "$SECS" 60 "$OUT/$n.csv" --snapshot-every-min 60 > $OUT/logs/$n.sampler.log 2>&1 &
  node scripts/compat/load_consumers.mjs "ws://$HOST:$p/ws/v1/data" $SUBS "$SECS" "$OUT/logs/$n.consumers.log" --churn-n 0 > /dev/null 2>&1 &
done
wait
