#!/usr/bin/env bash
# 换版后 24 小时每分钟只读采样（GET /metrics 与 /api/v1/subscriptions），写 evidence/1.2.0/nas-198/post-swap-24h/。
# caffeinate 阻止本机休眠（1.1.1 的教训：本机休眠会中断采样）。采样列见 scripts/compat/sample_metrics.mjs（含运行期订阅与各 quote 入站累计）。
# 用法：bash evidence/1.2.0/tools/observe_24h.sh        # 后台启动，PID 写入 sampler.pid
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="$ROOT/evidence/1.2.0/nas-198/post-swap-24h"
mkdir -p "$OUT"
cd "$ROOT"
date -u +%FT%TZ > "$OUT/started-at.txt"
nohup caffeinate -i node scripts/compat/sample_metrics.mjs http://192.168.1.198:8081 86460 60 "$OUT/prod.csv" --snapshot-every-min 60 \
  > "$OUT/sampler.log" 2>&1 &
echo $! > "$OUT/sampler.pid"
echo "sampler pid $(cat "$OUT/sampler.pid"); output $OUT/prod.csv"
