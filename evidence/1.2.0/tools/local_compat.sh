#!/usr/bin/env bash
# 开关关闭时与 1.1.1 同窗口逐字节对比（任务书 §四）：本机同时运行两个原生构建，连真实交易所，
#   old = v1.1.1（a06fe20，现网源码），new = 指定提交（默认 HEAD），两边用同一份现网配置（不含 runtime_subscriptions 段 = 开关关闭）。
# 两边补入同一份 198 订阅后同时抓 Gateway + Legacy 推送，再用 scripts/compat/compare.mjs 对比。
# 用法：bash evidence/1.2.0/tools/local_compat.sh [抓取秒数=240] [new 提交=HEAD] [输出目录]
#      COMPARE_ONLY=1 bash evidence/1.2.0/tools/local_compat.sh 240 HEAD <已有输出目录>   # 只对已抓数据重跑对比
# 判据与 1.1.1 一致：push-gateway 只比 tick/kline；quote 单独留证，按 docs/33 §4 裁定只允许 quote.binance.BNBUSDT 的逐字节项有数量差异。
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SECS="${1:-240}"
NEW_REV="${2:-HEAD}"
OUT="${3:-$ROOT/evidence/1.2.0/compat-local-switch-off}"
CFG="$ROOT/evidence/1.1.1/final-artifact/config.final.yaml"
SUBS="$ROOT/evidence/1.2.0/preflight/198.subscriptions.json"
OLD_WT=/tmp/md120-old-v111; NEW_WT=/tmp/md120-new
OLD_TGT=/tmp/md120-old-target; NEW_TGT=/tmp/md120-new-target
OLD_OFF=1000; NEW_OFF=2000
OLD_HTTP=$((8081 + OLD_OFF)); NEW_HTTP=$((8081 + NEW_OFF))
mkdir -p "$OUT"
cd "$ROOT"
T_GW="tick.binance.BTCUSDT tick.binance.ETHUSDT tick.binance.BNBUSDT tick.okx.BTC-USDT-SWAP tick.okx.ETH-USDT-SWAP tick.okx.BNB-USDT-SWAP kline.5m.binance.BTCUSDT kline.5m.binance.ETHUSDT kline.5m.okx.BTC-USDT-SWAP kline.5m.okx.ETH-USDT-SWAP quote.binance.BNBUSDT quote.okx.BNB-USDT-SWAP"
T_LG="tick.binance.BTCUSDT tick.binance.ETHUSDT tick.binance.BNBUSDT tick.okx.BTC-USDT-SWAP tick.okx.ETH-USDT-SWAP tick.okx.BNB-USDT-SWAP kline.5m.binance.BTCUSDT kline.5m.binance.ETHUSDT kline.5m.okx.BTC-USDT-SWAP kline.5m.okx.ETH-USDT-SWAP"
T_QUOTE="quote.binance.BNBUSDT quote.okx.BNB-USDT-SWAP"

compare_all() {
  local C=scripts/compat/compare.mjs
  node $C selftest > "$OUT/compare-selftest.log" 2>&1; echo "exit=$?" >> "$OUT/compare-selftest.log"
  # shellcheck disable=SC2086
  node $C push "$OUT/old-gateway" "$OUT/new-gateway" $T_LG > "$OUT/compare-push-gateway.log" 2>&1; echo "exit=$?" >> "$OUT/compare-push-gateway.log"
  # shellcheck disable=SC2086
  node $C push "$OUT/old-legacy" "$OUT/new-legacy" $T_LG > "$OUT/compare-push-legacy.log" 2>&1; echo "exit=$?" >> "$OUT/compare-push-legacy.log"
  node $C noquote "$OUT/new-legacy" > "$OUT/compare-noquote-legacy.log" 2>&1; echo "exit=$?" >> "$OUT/compare-noquote-legacy.log"
  node $C subs "$OUT/old.subscriptions.json" "$OUT/new.subscriptions.json" > "$OUT/compare-subs.log" 2>&1; echo "exit=$?" >> "$OUT/compare-subs.log"
  node $C metrics "$OUT/old.metrics.txt" "$OUT/new.metrics.txt" > "$OUT/compare-metrics.log" 2>&1; echo "exit=$?" >> "$OUT/compare-metrics.log"
  # shellcheck disable=SC2086
  node $C push "$OUT/old-gateway" "$OUT/new-gateway" $T_QUOTE > "$OUT/compare-quote.log" 2>&1; echo "exit=$?" >> "$OUT/compare-quote.log"
  # quote 只允许 quote.binance.BNBUSDT 的"FAIL 逐字节"（docs/33 §4 已裁定的数量配对噪声），其余任何 FAIL 都算失败
  local quote_verdict
  quote_verdict="$(awk '/^== /{s=substr($0,4)} /FAIL/ && !/RESULT:/ { if (!(s=="quote.binance.BNBUSDT" && $0 ~ /FAIL 逐字节:/)) bad=1 } END {print bad ? "QUOTE-FAIL (outside the ruled noise)" : "QUOTE-OK (only ruled quote.binance.BNBUSDT quantity noise, if any)"}' "$OUT/compare-quote.log")"
  echo "$quote_verdict" >> "$OUT/compare-quote.log"
  for f in selftest push-gateway push-legacy noquote-legacy subs metrics; do
    printf '%-16s %s | %s\n' "$f" "$(grep -E 'RESULT|selftest' "$OUT/compare-$f.log" | tail -1)" "$(tail -1 "$OUT/compare-$f.log")"
  done > "$OUT/gate-results.txt"
  printf '%-16s %s\n' quote "$quote_verdict" >> "$OUT/gate-results.txt"
  cat "$OUT/gate-results.txt"
}
if [ "${COMPARE_ONLY:-0}" = 1 ]; then compare_all; exit 0; fi

build() { # worktree rev target
  git worktree remove --force "$1" >/dev/null 2>&1 || true
  git worktree add --detach "$1" "$2" >/dev/null 2>&1
  (cd "$1" && CARGO_TARGET_DIR="$3" cargo build --release -p md-server) > "$OUT/build-$(basename "$1").log" 2>&1
  echo "$(git -C "$1" rev-parse HEAD) $(shasum -a 256 "$3/release/md-server" | cut -d' ' -f1)"
}
echo "old: $(build $OLD_WT v1.1.1 $OLD_TGT)" | tee "$OUT/binaries.txt"
echo "new: $(build $NEW_WT "$NEW_REV" $NEW_TGT)" | tee -a "$OUT/binaries.txt"
cp "$CFG" "$OUT/config.used.yaml"
grep -q runtime_subscriptions "$OUT/config.used.yaml" && { echo "config must not contain runtime_subscriptions"; exit 1; }

RUST_LOG=warn "$OLD_TGT/release/md-server" --config "$OUT/config.used.yaml" --port-offset $OLD_OFF > "$OUT/old.server.log" 2>&1 &
OLD_PID=$!
RUST_LOG=warn "$NEW_TGT/release/md-server" --config "$OUT/config.used.yaml" --port-offset $NEW_OFF > "$OUT/new.server.log" 2>&1 &
NEW_PID=$!
trap 'kill $OLD_PID $NEW_PID 2>/dev/null; wait $OLD_PID $NEW_PID 2>/dev/null' EXIT
sleep 15
for side in old:$OLD_HTTP new:$NEW_HTTP; do
  n=${side%%:*}; p=${side##*:}
  node scripts/compat/restore_subs.mjs "$SUBS" "http://127.0.0.1:$p" --apply > "$OUT/$n.restore.log" 2>&1
  tail -1 "$OUT/$n.restore.log"
done
sleep 20

date -u +%FT%TZ > "$OUT/captured_at_utc.txt"
CAP_PIDS=()
for side in old:$OLD_HTTP new:$NEW_HTTP; do
  n=${side%%:*}; p=${side##*:}
  # shellcheck disable=SC2086
  node scripts/compat/capture_ws.mjs gateway "ws://127.0.0.1:$p/ws/v1/data" "$SECS" "$OUT/$n-gateway" $T_GW > "$OUT/$n-gateway.log" 2>&1 &
  CAP_PIDS+=($!)
  # shellcheck disable=SC2086
  node scripts/compat/capture_ws.mjs legacy "ws://127.0.0.1:$p/ws" "$SECS" "$OUT/$n-legacy" $T_LG > "$OUT/$n-legacy.log" 2>&1 &
  CAP_PIDS+=($!)
done
wait "${CAP_PIDS[@]}"
for side in old:$OLD_HTTP new:$NEW_HTTP; do
  n=${side%%:*}; p=${side##*:}
  curl -s "http://127.0.0.1:$p/api/v1/subscriptions" > "$OUT/$n.subscriptions.json"
  curl -s "http://127.0.0.1:$p/metrics" > "$OUT/$n.metrics.txt"
done
compare_all
git worktree remove --force $OLD_WT >/dev/null 2>&1; git worktree remove --force $NEW_WT >/dev/null 2>&1
echo LOCAL_COMPAT_DONE
