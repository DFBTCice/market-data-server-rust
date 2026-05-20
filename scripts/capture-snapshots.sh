#!/usr/bin/env bash
# capture-snapshots.sh
#
# 从运行中的 Go 版 market-data-server 抓取 JSON 快照。
# 用法：
#   1. 启动 Go 版：cd /path/to/go/project && go run ./cmd/dataserver --config config.yaml
#   2. 等待数据到达（约 5-10 秒）
#   3. 运行本脚本：bash scripts/capture-snapshots.sh
#
# 快照保存到 tests/compatibility/fixtures/

set -euo pipefail

FIXTURES_DIR="crates/md-tests/tests/fixtures"
GO_REST="http://localhost:8081"
GO_GRPC="localhost:50051"

mkdir -p "$FIXTURES_DIR"

echo "=== 从 Go 版抓取 JSON 快照 ==="

# ---- REST API 快照 ----

echo "[1/6] GET /api/v1/data/latest/tick/binance/BTCUSDT"
curl -sf "$GO_REST/api/v1/data/latest/tick/binance/BTCUSDT" \
  | python3 -m json.tool \
  > "$FIXTURES_DIR/tick_binance_btcusdt.json" 2>/dev/null || echo "  (跳过：服务未就绪)"

echo "[2/6] GET /api/v1/data/latest/tick/okx/BTC-USDT-SWAP"
curl -sf "$GO_REST/api/v1/data/latest/tick/okx/BTC-USDT-SWAP" \
  | python3 -m json.tool \
  > "$FIXTURES_DIR/tick_okx_btc-usdt-swap.json" 2>/dev/null || echo "  (跳过：服务未就绪)"

echo "[3/6] GET /api/v1/data/latest/kline/binance/BTCUSDT/1m"
curl -sf "$GO_REST/api/v1/data/latest/kline/binance/BTCUSDT/1m" \
  | python3 -m json.tool \
  > "$FIXTURES_DIR/kline_binance_btcusdt_1m.json" 2>/dev/null || echo "  (跳过：服务未就绪)"

echo "[4/6] GET /api/v1/subscriptions"
curl -sf "$GO_REST/api/v1/subscriptions" \
  | python3 -m json.tool \
  > "$FIXTURES_DIR/subscriptions.json" 2>/dev/null || echo "  (跳过：服务未就绪)"

# ---- 错误响应快照 ----

echo "[5/6] 错误响应：不存在的 exchange"
curl -sf "$GO_REST/api/v1/data/latest/tick/nonexistent/BTCUSDT" \
  | python3 -m json.tool \
  > "$FIXTURES_DIR/error_not_found.json" 2>/dev/null || echo "  (跳过：服务未就绪)"

echo "[6/6] 错误响应：缺少 symbol"
curl -sf "$GO_REST/api/v1/data/latest/tick/binance/" \
  | python3 -m json.tool \
  > "$FIXTURES_DIR/error_bad_request.json" 2>/dev/null || echo "  (跳过：服务未就绪)"

# ---- WebSocket 快照（需要 websocat 工具）----

if command -v websocat &>/dev/null; then
  echo "[WS] 抓取 Gateway WebSocket 推送格式"
  # 发送订阅请求，等待一条数据
  echo '{"action":"subscribe","streams":["tick.binance.BTCUSDT"]}' \
    | timeout 10 websocat "ws://localhost:8081/ws/v1/data" \
    | head -1 \
    > "$FIXTURES_DIR/ws_gateway_subscribe_response.json" 2>/dev/null || echo "  (跳过)"

  echo "[WS] 抓取 Gateway WebSocket 数据推送"
  echo '{"action":"subscribe","streams":["tick.binance.BTCUSDT"]}' \
    | timeout 15 websocat "ws://localhost:8081/ws/v1/data" \
    | tail -1 \
    > "$FIXTURES_DIR/ws_gateway_tick_push.json" 2>/dev/null || echo "  (跳过)"
else
  echo "[WS] 跳过 WebSocket 快照（需要安装 websocat: brew install websocat）"
fi

# ---- 统计 ----

echo ""
echo "=== 快照抓取完成 ==="
echo "保存位置: $FIXTURES_DIR/"
ls -la "$FIXTURES_DIR/"
echo ""
echo "注意：空文件或跳过的快照需要手动补充。"
echo "建议：在稳定网络环境下多次抓取，选择最典型的样本。"
