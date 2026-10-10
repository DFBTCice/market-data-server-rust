#!/usr/bin/env bash
# capture-snapshots.sh
#
# 从运行中的 market-data-server 抓取 JSON 快照，供 crates/md-tests 做兼容性测试。
#
# 用法：
#   SNAPSHOT_REST=http://HOST:8081 SNAPSHOT_WS=ws://HOST:8081 \
#     bash scripts/capture-snapshots.sh
#
#   默认 SNAPSHOT_REST=http://localhost:8081、SNAPSHOT_WS=ws://localhost:8081。
#   可选 SNAPSHOT_TICK_SYMBOL / SNAPSHOT_OKX_SYMBOL 指定要抓的品种（该服务必须已订阅）；
#   可选 SNAPSHOT_FIXTURES_DIR 指定输出目录（默认直接覆盖 md-tests 的快照）。
#
# 快照保存到 crates/md-tests/tests/fixtures/
#
# 1.1.0 起的改动（修复"抓取失败却悄悄留下空文件"的问题）：
#   - 不再使用 `curl -f`：-f 会在 HTTP 4xx 时丢弃响应体，导致 error_*.json 永远是 0 字节；
#   - 先写临时文件，校验"非空且是合法 JSON"后才覆盖正式快照；
#   - 任何一步失败都会让脚本以非 0 退出，不再打印"跳过"后继续；
#   - WebSocket 快照改用 Node(>=22) 内置 WebSocket（不依赖 websocat 与 GNU timeout，macOS 可用）。

set -euo pipefail

FIXTURES_DIR="${SNAPSHOT_FIXTURES_DIR:-crates/md-tests/tests/fixtures}"
REST="${SNAPSHOT_REST:-http://localhost:8081}"
WS="${SNAPSHOT_WS:-ws://localhost:8081}"
TICK_SYMBOL="${SNAPSHOT_TICK_SYMBOL:-BTCUSDT}"
OKX_SYMBOL="${SNAPSHOT_OKX_SYMBOL:-BTC-USDT-SWAP}"

mkdir -p "$FIXTURES_DIR"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# capture <名称> <期望HTTP状态码> <curl 其余参数...>
capture() {
  local name="$1" want="$2"; shift 2
  local body="$TMP/$name.body"
  local code
  code="$(curl -s -m 15 -o "$body" -w '%{http_code}' "$@")" || { echo "  ✗ ${name}: curl failed" >&2; exit 1; }
  if [ "$code" != "$want" ]; then
    echo "  ✗ ${name}: HTTP ${code} (expected ${want})" >&2
    cat "$body" >&2 || true
    exit 1
  fi
  python3 - "$body" "$FIXTURES_DIR/$name" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
raw = open(src, encoding="utf-8").read()
if not raw.strip():
    sys.exit("响应体为空，拒绝写入 " + dst)
obj = json.loads(raw)  # 非法 JSON 会直接抛错
with open(dst, "w", encoding="utf-8") as f:
    f.write(json.dumps(obj, indent=4, ensure_ascii=False) + "\n")
PY
  echo "  ✓ $name ($(wc -c < "$FIXTURES_DIR/$name") bytes)"
}

echo "=== 从 $REST 抓取 JSON 快照 ==="

echo "[1/6] GET /api/v1/data/latest/tick/binance/$TICK_SYMBOL"
capture tick_binance_btcusdt.json 200 "$REST/api/v1/data/latest/tick/binance/$TICK_SYMBOL"

echo "[2/6] GET /api/v1/data/latest/tick/okx/$OKX_SYMBOL"
capture tick_okx_btc-usdt-swap.json 200 "$REST/api/v1/data/latest/tick/okx/$OKX_SYMBOL"

echo "[3/6] GET /api/v1/data/latest/kline/binance/$TICK_SYMBOL/1m"
capture kline_binance_btcusdt_1m.json 200 "$REST/api/v1/data/latest/kline/binance/$TICK_SYMBOL/1m"

echo "[4/6] GET /api/v1/subscriptions"
capture subscriptions.json 200 "$REST/api/v1/subscriptions"

# ---- 错误响应快照（期望 4xx；不会改变服务状态）----
echo "[5/6] 404：不存在的 exchange"
capture error_not_found.json 404 "$REST/api/v1/data/latest/tick/nonexistent/$TICK_SYMBOL"

echo "[6/6] 400：非法订阅类型（被校验拒绝，不产生订阅）"
capture error_bad_request.json 400 -X POST -H 'Content-Type: application/json' \
  -d '{"exchange":"binance","type":"DEPTH","symbols":["'"$TICK_SYMBOL"'"]}' \
  "$REST/api/v1/subscriptions"

# ---- WebSocket 快照 ----
echo "[WS] 抓取 Gateway WebSocket 订阅回执与数据推送"
command -v node >/dev/null || { echo "  ✗ 需要 Node >= 22（内置 WebSocket）" >&2; exit 1; }
WS_OUT="$TMP/ws"
node scripts/compat/capture_ws.mjs gateway "$WS/ws/v1/data" 20 "$WS_OUT" "tick.binance.$TICK_SYMBOL" >/dev/null
python3 - "$WS_OUT" "$FIXTURES_DIR" "tick.binance.$TICK_SYMBOL" <<'PY'
import json, sys
out, fx, topic = sys.argv[1], sys.argv[2], sys.argv[3]
acks = [l for l in open(f"{out}/_acks.jsonl", encoding="utf-8") if l.strip()]
pushes = [l for l in open(f"{out}/{topic}.raw.jsonl", encoding="utf-8") if l.strip()]
if not acks or not pushes:
    sys.exit(f"WS 抓取失败：acks={len(acks)} pushes={len(pushes)}")
def dump(name, line):
    with open(f"{fx}/{name}", "w", encoding="utf-8") as f:
        f.write(json.dumps(json.loads(line), indent=4, ensure_ascii=False) + "\n")
dump("ws_gateway_subscribe_response.json", acks[0])
dump("ws_gateway_tick_push.json", pushes[0])
print("  ✓ ws_gateway_subscribe_response.json / ws_gateway_tick_push.json")
PY

# ---- 校验：不允许存在空快照 ----
EMPTY="$(find "$FIXTURES_DIR" -name '*.json' -size 0 | tr '\n' ' ')"
if [ -n "$EMPTY" ]; then
  echo "✗ 存在空快照: $EMPTY" >&2
  exit 1
fi

echo ""
echo "=== 快照抓取完成 ==="
ls -la "$FIXTURES_DIR/"
