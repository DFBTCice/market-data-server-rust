#!/usr/bin/env bash
# 开关打开的金丝雀（本机版，任务书 §四 旁路金丝雀的同款流程；198 版见 canary_198.py）：
#   现网配置 + runtime_subscriptions（闲置 2 分钟、检查 10 秒、保护期 1 分钟），连真实交易所；
#   restore_subs.mjs 灌入 198 当前订阅，再用 canary_consumers.mjs 模拟 OMS 与 botd 两类消费者。
# 用法：bash evidence/1.2.0/tools/local_canary.sh [提交=HEAD] [输出目录] [总时长秒=360]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
REV="${1:-HEAD}"
OUT="${2:-$ROOT/evidence/1.2.0/canary-local-switch-on}"
TOTAL="${3:-360}"
WT=/tmp/md120-canary; TGT=/tmp/md120-new-target
OFF=3000; HTTP=$((8081 + OFF))
mkdir -p "$OUT"
cd "$ROOT"
git worktree remove --force $WT >/dev/null 2>&1 || true
git worktree add --detach $WT "$REV" >/dev/null 2>&1
(cd $WT && CARGO_TARGET_DIR=$TGT cargo build --release -p md-server) > "$OUT/build.log" 2>&1
echo "$(git -C $WT rev-parse HEAD) $(shasum -a 256 $TGT/release/md-server | cut -d' ' -f1)" | tee "$OUT/binary.txt"
git worktree remove --force $WT >/dev/null 2>&1

{ cat evidence/1.1.1/final-artifact/config.final.yaml; cat <<'YAML'

# 1.2.0 金丝雀：开关打开、参数调短
runtime_subscriptions:
  idle_unsubscribe: true
  idle_minutes: 2
  check_interval_seconds: 10
  startup_grace_minutes: 1
  max_runtime_subscriptions: 200
YAML
} > "$OUT/config.used.yaml"

RUST_LOG=warn,md_runtime_subs=info $TGT/release/md-server --config "$OUT/config.used.yaml" --port-offset $OFF > "$OUT/server.log" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null' EXIT
sleep 10
date -u +%FT%TZ > "$OUT/started_at_utc.txt"
node scripts/compat/restore_subs.mjs evidence/1.2.0/preflight/198.subscriptions.json "http://127.0.0.1:$HTTP" --apply > "$OUT/restore.log" 2>&1
tail -1 "$OUT/restore.log"
node evidence/1.2.0/tools/canary_consumers.mjs "http://127.0.0.1:$HTTP" "$OUT" --idle-sec=120 --botd-hold-sec=90 --total-sec="$TOTAL" > "$OUT/consumers.stdout.log" 2>&1
code=$?
curl -s "http://127.0.0.1:$HTTP/api/v1/subscriptions" > "$OUT/final.subscriptions.json"
curl -s "http://127.0.0.1:$HTTP/metrics" > "$OUT/final.metrics.txt"

# OPS_GUIDE §8.6 的不重启开关：改文件 + SIGHUP，确认开关立即翻转、进程不重启、订阅不丢
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
grep -E 'md_runtime_subs|runtime_subscriptions changed' "$OUT/server.log" > "$OUT/runtime-subs.log" || true
tail -1 "$OUT/consumers.log"
echo "canary exit=$code"
exit $code
