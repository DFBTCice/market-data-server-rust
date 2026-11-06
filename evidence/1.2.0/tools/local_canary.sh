#!/usr/bin/env bash
# 开关打开的金丝雀（本机版；198 版见 canary_198.py，流程相同）：现网配置 + runtime_subscriptions，连真实交易所，
# restore_subs.mjs 灌入 198 订阅，再用 canary_consumers.mjs 模拟 OMS / 1stbotv3 / 报价三类消费者。报价补订关闭（现网决定）。
#   MODE=short（默认）：闲置 2 分钟、检查 10 秒、保护期 1 分钟，总 420 秒；结束后再验证不重启的 SIGHUP 开关（OPS_GUIDE §8.6）
#   MODE=prod        ：现网参数（evidence/1.2.0/nas-198/config.release.yaml：闲置 10、保护 10、检查 60 秒），总 900 秒
# 用法：MODE=short|prod bash evidence/1.2.0/tools/local_canary.sh [提交=HEAD] [输出目录]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
MODE="${MODE:-short}"
REV="${1:-HEAD}"
OUT="${2:-$ROOT/evidence/1.2.0/canary-local-switch-on-$MODE}"
TGT=/tmp/md120-canary-target; WT=/tmp/md120-canary-$MODE
mkdir -p "$OUT"
cd "$ROOT"
case "$MODE" in
  short) OFF=3000; FLAGS="--idle-sec=120 --botd-hold-sec=90 --total-sec=420"
         { cat evidence/1.1.1/final-artifact/config.final.yaml; printf '\n# 1.2.0 金丝雀试跑：开关打开、参数调短、报价补订关闭\nruntime_subscriptions:\n  idle_unsubscribe: true\n  idle_minutes: 2\n  check_interval_seconds: 10\n  startup_grace_minutes: 1\n  max_runtime_subscriptions: 200\n  on_demand_quote: false\n'; } > "$OUT/config.used.yaml" ;;
  prod)  OFF=3500; FLAGS="--idle-sec=600 --botd-hold-sec=60 --total-sec=900"
         cp evidence/1.2.0/nas-198/config.release.yaml "$OUT/config.used.yaml" ;;
  *) echo "MODE must be short or prod"; exit 2 ;;
esac
HTTP=$((8081 + OFF))
git worktree remove --force "$WT" >/dev/null 2>&1 || true
git worktree add --detach "$WT" "$REV" >/dev/null 2>&1
(cd "$WT" && CARGO_TARGET_DIR="$TGT-$MODE" cargo build --release -p md-server) > "$OUT/build.log" 2>&1
echo "$(git -C "$WT" rev-parse HEAD) $(shasum -a 256 "$TGT-$MODE/release/md-server" | cut -d' ' -f1)" | tee "$OUT/binary.txt"
git worktree remove --force "$WT" >/dev/null 2>&1

RUST_LOG=warn,md_runtime_subs=info "$TGT-$MODE/release/md-server" --config "$OUT/config.used.yaml" --port-offset $OFF > "$OUT/server.log" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null' EXIT
sleep 10
date -u +%FT%TZ > "$OUT/started_at_utc.txt"
node scripts/compat/restore_subs.mjs evidence/1.2.0/preflight/198.subscriptions.json "http://127.0.0.1:$HTTP" --apply > "$OUT/restore.log" 2>&1
tail -1 "$OUT/restore.log"
# shellcheck disable=SC2086
node evidence/1.2.0/tools/canary_consumers.mjs "http://127.0.0.1:$HTTP" "$OUT" $FLAGS > "$OUT/consumers.stdout.log" 2>&1
code=$?
curl -s "http://127.0.0.1:$HTTP/api/v1/subscriptions" > "$OUT/final.subscriptions.json"
curl -s "http://127.0.0.1:$HTTP/metrics" > "$OUT/final.metrics.txt"

if [ "$MODE" = short ]; then
  enabled() { curl -s "http://127.0.0.1:$HTTP/metrics" | awk '/^md_runtime_subscriptions_enabled /{print $2}'; }
  start_ts() { curl -s "http://127.0.0.1:$HTTP/metrics" | awk '/^process_start_time_seconds /{print $2}'; }
  nsubs() { curl -s "http://127.0.0.1:$HTTP/api/v1/subscriptions" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).length))'; }
  {
    echo "before: enabled=$(enabled) start=$(start_ts) subs=$(nsubs)"
    sed -i '' 's/^\( *idle_unsubscribe: *\)true/\1false/' "$OUT/config.used.yaml"; kill -HUP $PID; sleep 2
    echo "after HUP (false): enabled=$(enabled) start=$(start_ts) subs=$(nsubs)"
    sed -i '' 's/^\( *idle_unsubscribe: *\)false/\1true/' "$OUT/config.used.yaml"; kill -HUP $PID; sleep 2
    echo "after HUP (true): enabled=$(enabled) start=$(start_ts) subs=$(nsubs)"
  } | tee "$OUT/sighup-toggle.txt"
fi
grep -E 'md_runtime_subs|runtime_subscriptions changed' "$OUT/server.log" > "$OUT/runtime-subs.log" || true
tail -1 "$OUT/consumers.log"
echo "canary($MODE) exit=$code"
exit $code
