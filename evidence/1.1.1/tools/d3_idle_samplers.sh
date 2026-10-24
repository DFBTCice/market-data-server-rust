#!/usr/bin/env bash
# 旁路主实验结束后（消费者已撤、WS 客户端=0），对 c0–c14 继续逐分钟采样 SECS 秒：
# 观察"无任何消费者"时 RSS 是否仍会因入口突发/重连而上涨（musl 高水位是否与消费者无关）。
set -u
cd "$(dirname "$0")/../../.."
SECS="${1:-5400}"; OUT=evidence/1.1.1/d3-load/idle
for pair in c0:28101 c1:28102 c2:28103 c3:28104 c4:28105 c5:28106 c6:28107 c7:28111 c8:28112 c9:28113 c10:28120 c11:28121 c12:28122 c13:28123 c14:28124; do
  c=${pair%%:*}; p=${pair##*:}
  node scripts/compat/sample_metrics.mjs "http://192.168.1.198:$p" "$SECS" 60 "$OUT/$c.idle.csv" > "$OUT/$c.idle.sampler.log" 2>&1 &
done
wait
