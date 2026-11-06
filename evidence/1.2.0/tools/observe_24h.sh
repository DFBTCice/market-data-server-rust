#!/usr/bin/env bash
# 换版后 24 小时每分钟只读采样（GET /metrics 与 /api/v1/subscriptions），写 evidence/1.2.0/nas-198/post-swap-24h/。
# 采样机：业务方这台 MacBook Pro（在 192.168.1.x 内网）。休眠会中断采样（1.1.1 的教训），所以：
#   caffeinate -i -s 阻止空闲休眠与系统休眠，但 -s 只在接电源时有效；合盖且无外接显示器仍会强制休眠。
#   采样期间必须接电源、不合盖；电源与断言状态写入 host.txt，结束后用 analyze_24h.py 标出采样缺口。
# 采样列见 scripts/compat/sample_metrics.mjs（含运行期订阅与各 quote 入站累计）。
# 用法：bash evidence/1.2.0/tools/observe_24h.sh        # 后台启动，PID 写入 sampler.pid
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="$ROOT/evidence/1.2.0/nas-198/post-swap-24h"
mkdir -p "$OUT"
cd "$ROOT"
if [ -f "$OUT/sampler.pid" ] && kill -0 "$(cat "$OUT/sampler.pid")" 2>/dev/null; then
  echo "sampler already running: pid $(cat "$OUT/sampler.pid")" >&2; exit 1
fi
power="$(pmset -g batt | head -1)"
case "$power" in
  *"AC Power"*) ;;
  *) echo "WARNING: not on AC power ($power); caffeinate -s will not prevent system sleep" >&2 ;;
esac
date -u +%FT%TZ > "$OUT/started-at.txt"
nohup caffeinate -i -s node scripts/compat/sample_metrics.mjs http://192.168.1.198:8081 86460 60 "$OUT/prod.csv" --snapshot-every-min 60 \
  > "$OUT/sampler.log" 2>&1 &
echo $! > "$OUT/sampler.pid"
sleep 2
{
  echo "started_at_utc=$(cat "$OUT/started-at.txt")"
  echo "host=$(scutil --get ComputerName 2>/dev/null || hostname)"
  echo "model=$(sysctl -n hw.model)"
  echo "power=$power"
  echo "caffeinate=caffeinate -i -s (pid $(cat "$OUT/sampler.pid"))"
  echo "--- pmset -g assertions (caffeinate lines)"
  pmset -g assertions | grep -i caffeinate || echo "no caffeinate assertion found"
} > "$OUT/host.txt"
cat "$OUT/host.txt"
echo "sampler pid $(cat "$OUT/sampler.pid"); output $OUT/prod.csv"
