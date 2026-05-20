#!/bin/bash
set -euo pipefail

# ============================================================
# Market Data Server (Rust) 生产部署脚本
# ============================================================
# 用法:
#   ./deploy.sh                    # 使用默认 latest 标签部署
#   ./deploy.sh v1.2.3             # 部署指定版本
#   ./deploy.sh v1.2.3 --local     # 使用本地构建的镜像
# ============================================================

VERSION="${1:-latest}"
LOCAL_BUILD=false
if [[ "${2:-}" == "--local" ]]; then
    LOCAL_BUILD=true
fi

REGISTRY="ghcr.io/dfbtcice"
IMAGE="market-data-server-rust"
COMPOSE_FILE="docker-compose.prod.yml"

echo "=========================================="
echo "  Market Data Server 部署脚本"
echo "  版本: $VERSION"
echo "  本地构建: $LOCAL_BUILD"
echo "=========================================="

# 检查依赖
command -v docker >/dev/null 2>&1 || { echo "错误: Docker 未安装"; exit 1; }
command -v docker-compose >/dev/null 2>&1 || { echo "错误: docker-compose 未安装"; exit 1; }

# 创建工作目录
DEPLOY_DIR="/opt/md-server"
mkdir -p "$DEPLOY_DIR"
cd "$DEPLOY_DIR"

echo "[1/6] 拉取/准备镜像..."
if [[ "$LOCAL_BUILD" == "true" ]]; then
    echo "使用本地构建的镜像..."
    # 假设镜像已在本地
else
    docker pull "${REGISTRY}/${IMAGE}:${VERSION}"
    # 如果版本不是 latest，打上 latest 标签便于 compose 使用
    if [[ "$VERSION" != "latest" ]]; then
        docker tag "${REGISTRY}/${IMAGE}:${VERSION}" "${REGISTRY}/${IMAGE}:latest"
    fi
fi

echo "[2/6] 备份当前配置..."
if [[ -f config.yaml ]]; then
    cp config.yaml "config.yaml.bak.$(date +%Y%m%d_%H%M%S)"
fi

echo "[3/6] 拉取最新 compose 和配置..."
# 如果是 GitHub 部署，可以从仓库拉取最新 compose 文件
# curl -L -o docker-compose.prod.yml "https://raw.githubusercontent.com/DFBTCice/market-data-server-rust/main/docker-compose.prod.yml"

# 确保配置文件存在
if [[ ! -f config.yaml ]]; then
    echo "创建默认配置文件..."
    cat > config.yaml <<'EOF'
log_level: "warn"

grpc_server:
  enabled: true
  listen_address: ":50051"

processor:
  tick_channel_buffer: 1000
  kline_channel_buffer: 1000
  broadcast_capacity: 4096

connectors:
  binance:
    enabled: true
    stream_base_url: "wss://fstream.binance.com/market/stream"
    rest_base_url: "https://fapi.binance.com"
    subscribe_ticks:
      - "BTCUSDT"
      - "ETHUSDT"
    subscribe_klines:
      "5m":
        - "BTCUSDT"
        - "ETHUSDT"
    reconnect_delay: "5s"
    ping_interval: "3m"

  okx:
    enabled: true
    stream_base_url_public: "wss://ws.okx.com:8443/ws/v5/public"
    stream_base_url_business: "wss://ws.okx.com:8443/ws/v5/business"
    rest_base_url: "https://www.okx.com"
    subscribe_ticks:
      - "BTC-USDT-SWAP"
      - "ETH-USDT-SWAP"
    subscribe_klines:
      "5m":
        - "BTC-USDT-SWAP"
        - "ETH-USDT-SWAP"
    reconnect_delay: "10s"
    ping_interval: "25s"

api_gateway:
  enabled: true
  listen_address: ":8081"
  market_data_grpc_target: "localhost:50051"
  admin_grpc_target: "localhost:50052"
  ws_ping_period: "30s"
  ws_write_wait: "10s"
  ws_max_message_size: 1024
EOF
fi

echo "[4/6] 停止旧容器..."
docker-compose -f "$COMPOSE_FILE" down --timeout 30 || true

echo "[5/6] 启动新容器..."
docker-compose -f "$COMPOSE_FILE" up -d

echo "[6/6] 验证部署..."
sleep 5

# 健康检查
HEALTH_STATUS=$(docker inspect --format='{{.State.Health.Status}}' md-server 2>/dev/null || echo "unknown")
echo "  md-server 健康状态: $HEALTH_STATUS"

if curl -sf http://localhost:8081/health >/dev/null 2>&1; then
    echo "  ✓ /health 检查通过"
else
    echo "  ✗ /health 检查失败，查看日志:"
    docker-compose -f "$COMPOSE_FILE" logs --tail=50 md-server
    exit 1
fi

if curl -sf http://localhost:8081/metrics >/dev/null 2>&1; then
    echo "  ✓ /metrics 检查通过"
else
    echo "  ✗ /metrics 检查失败"
fi

echo ""
echo "=========================================="
echo "  部署完成!"
echo "=========================================="
echo "  REST API:    http://localhost:8081"
echo "  gRPC:        localhost:50051"
echo "  Prometheus:  http://localhost:9090"
echo "  Grafana:     http://localhost:3000"
echo ""
echo "  常用命令:"
echo "    查看日志:   docker-compose -f $COMPOSE_FILE logs -f md-server"
echo "    查看状态:   docker-compose -f $COMPOSE_FILE ps"
echo "    热重载:     kill -HUP \$(docker inspect --format='{{.State.Pid}}' md-server)"
echo "    回滚:       docker-compose -f $COMPOSE_FILE down && docker pull ${REGISTRY}/${IMAGE}:<旧版本> && ./deploy.sh <旧版本>"
echo "=========================================="
