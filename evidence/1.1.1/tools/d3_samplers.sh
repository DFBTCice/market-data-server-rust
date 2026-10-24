#!/usr/bin/env bash
# 重启 D3 采样器（仅采样，不动容器/消费者）。用法：d3_samplers.sh <seconds>
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-25200}"; OUT=evidence/1.1.1/d3-load; HOST=192.168.1.198
for pair in c0:28101 c1:28102 c2:28103 c3:28104 c4:28105 c5:28106 c6:28107; do
  n=${pair%%:*}; p=${pair##*:}
  node scripts/compat/sample_metrics.mjs "http://$HOST:$p" "$SECS" 60 "$OUT/$n.csv" --snapshot-every-min 60 > $OUT/logs/$n.sampler.log 2>&1 &
done
wait
