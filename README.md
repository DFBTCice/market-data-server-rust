# Market Data Server (Rust)

Go 版 `market-data-server` 的 Rust 1:1 重写。聚合 Binance、OKX 实时行情数据，对外提供 gRPC、REST API、WebSocket 三种接口，保持所有输出格式与 Go 版**字节级兼容**。

## 架构

```
                    ┌─────────────────────────────────────────┐
                    │              md-server                   │
                    │  (CLI, 信号处理, 启动编排)                │
                    └───┬──────┬──────┬──────┬────────────────┘
                        │      │      │      │
           ┌────────────┘      │      │      └────────────┐
           ▼                   ▼      ▼                   ▼
   ┌──────────────┐   ┌────────────┐ ┌──────────┐  ┌──────────┐
   │ md-connector │   │md-processor│ │ md-grpc  │  │md-gateway│
   │ Binance+OKX  │──▶│Cache+PubSub│ │  gRPC    │  │REST + WS │
   │  WebSocket   │   │ +Metrics   │ │ 服务层   │  │  接口层  │
   └──────────────┘   └────────────┘ └──────────┘  └──────────┘
                             │
                        ┌────┴────┐
                        ▼         ▼
                   md-domain   md-proto
                   (类型)     (protobuf)
```

### Crate 依赖

| Crate | 职责 | 测试数 |
|-------|------|--------|
| `md-domain` | Tick/Kline 类型、Topic 格式、serde helpers | 23 |
| `md-config` | YAML 配置加载、环境变量覆盖、校验 | 7 |
| `md-proto` | protobuf 编译、gRPC 生成代码 | 9 |
| `md-connector` | Binance/OKX WebSocket 连接器 | 26 |
| `md-processor` | DashMap 缓存 + broadcast PubSub + Metrics | 12 |
| `md-grpc` | MarketDataService + AdminService | 19 |
| `md-gateway` | REST API + WebSocket (Gateway/Legacy 格式) | 9 |
| `md-tests` | 兼容性测试（Go 快照对比） | 4 |
| **总计** | | **109** |

## 快速开始

### 编译

```bash
# 需要 Rust 1.75+
cargo build --release
```

#### 交叉编译静态 Linux 二进制（macOS → x86_64 Linux，非 Docker）

在 macOS（Apple Silicon）上直接产出**全静态、零依赖**的 x86_64 Linux 可执行文件，
可拷到任意 x86_64 Linux 上 `./md-server --config config.yaml` 直接运行（无需 glibc/openssl）：

```bash
# 一次性准备工具链
brew install zig
cargo install cargo-zigbuild
rustup target add x86_64-unknown-linux-musl

# 编译（vendored-tls 让 OpenSSL 随源码静态编译，仅交叉编译时启用）
cargo zigbuild -p md-server --release \
  --target x86_64-unknown-linux-musl \
  --features md-connector/vendored-tls

# 产物：target/x86_64-unknown-linux-musl/release/md-server
#   ELF 64-bit x86-64, statically linked, stripped
```

### Docker 部署

```bash
# 标准构建（可直连 Docker Hub / crates.io 的环境）
docker build -t md-server:latest .

# 国内/内网环境：基础镜像走 daocloud、apt 走阿里云、crates 走 rsproxy
docker build -t md-server:latest -f Dockerfile.cn .
```

详见 [DEPLOYMENT.md §4](DEPLOYMENT.md#4-docker-部署)（含 Portainer API 一键替换部署流程）。

### 运行

```bash
# 使用默认配置（config.yaml）
cargo run --release -- --config config.yaml

# 使用环境变量覆盖端口
GRPC_SERVER_LISTEN_ADDRESS=":50051" \
API_GATEWAY_LISTEN_ADDRESS=":8081" \
cargo run --release -- --config config.yaml
```

### 命令行参数

```
Usage: md-server [OPTIONS]

Options:
  -c, --config <CONFIG>            配置文件路径 [default: config.yaml]
      --port-offset <PORT_OFFSET>  端口偏移量（所有端口 += offset） [default: 0]
  -h, --help
```

`--port-offset` 方便与 Go 版并行运行：

```bash
# Go 版使用默认端口（:50051, :8081）
# Rust 版偏移 10（:50061, :8091）
cargo run --release -- --config config.yaml --port-offset 10
```

## 对外接口

### REST API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/data/latest/tick/{exchange}/{symbol}` | 获取最新 Tick |
| GET | `/api/v1/data/latest/kline/{exchange}/{symbol}/{interval}` | 获取最新 Kline |
| GET | `/api/v1/subscriptions` | 获取当前订阅列表 |
| POST | `/api/v1/subscriptions` | 添加订阅 |
| DELETE | `/api/v1/subscriptions` | 移除订阅 |
| GET | `/health` | 健康检查 |
| GET | `/metrics` | Prometheus 指标（与 Go dashboard 等价 + 标准 process_*） |

> 已附 Grafana dashboard 模板：[`dashboards/md-server-rust.json`](dashboards/md-server-rust.json)，与 Go 版 dashboard 一一对应（含按交易所分维度延迟、网关内部延迟、慢客户端踢出、CPU/RSS/FD 等）。详见 [DEPLOYMENT.md §7](DEPLOYMENT.md#72-prometheus-指标全集)。

### gRPC

| 服务 | 方法 | 说明 |
|------|------|------|
| MarketDataService | `SubscribeTicks` | 订阅实时 Tick（server streaming） |
| MarketDataService | `SubscribeKlines` | 订阅实时 Kline（server streaming） |
| MarketDataService | `GetLatestTick` | 获取最新 Tick |
| MarketDataService | `GetLatestKline` | 获取最新 Kline |
| AdminService | `AddSubscription` | 动态添加订阅 |
| AdminService | `RemoveSubscription` | 动态移除订阅 |
| AdminService | `GetSubscriptions` | 获取订阅列表 |

### WebSocket

**Gateway 格式** (`/ws/v1/data`):

```json
// 订阅
{"action":"subscribe","streams":["tick.binance.BTCUSDT"]}

// 推送
{"type":"tick","topic":"tick.binance.BTCUSDT","data":{...}}
```

**Legacy 格式** (`/ws`):

```json
// 订阅
{"op":"subscribe","args":["tick.binance.BTCUSDT"]}

// 推送
{"topic":"tick.binance.BTCUSDT","data":{...}}
```

## 与 Go 版并行对比

```bash
# 终端 1：启动 Go 版（默认端口）
cd /path/to/go/market-data-server
./market-data-server --config config.yaml

# 终端 2：启动 Rust 版（端口偏移 10）
cd /path/to/rust/rest重构
cargo run --release -- --config config.yaml --port-offset 10

# 终端 3：对比 REST API 输出
diff <(curl -s localhost:8081/api/v1/data/latest/tick/binance/BTCUSDT | python3 -m json.tool) \
     <(curl -s localhost:8091/api/v1/data/latest/tick/binance/BTCUSDT | python3 -m json.tool)

# 对比 Prometheus 指标
curl localhost:8091/metrics
```

## 性能对比

### 测量方法

```bash
# 内存对比（RSS）
ps -o rss,pid -p $(pgrep market-data-server)
ps -o rss,pid -p $(pgrep md-server)

# CPU 对比
pidstat -p $(pgrep market-data-server) 1
pidstat -p $(pgrep md-server) 1

# REST API 负载测试
hey -z 60s -q 1000 http://localhost:8081/api/v1/data/latest/tick/binance/BTCUSDT
hey -z 60s -q 1000 http://localhost:8091/api/v1/data/latest/tick/binance/BTCUSDT

# gRPC 负载测试
ghz --insecure --total 10000 --concurrency 100 \
  --call marketdata.MarketDataService/GetLatestTick \
  -d '{"exchange":"binance","symbol":"BTCUSDT"}' \
  localhost:50051

ghz --insecure --total 10000 --concurrency 100 \
  --call marketdata.MarketDataService/GetLatestTick \
  -d '{"exchange":"binance","symbol":"BTCUSDT"}' \
  localhost:50061
```

### 预期指标

| 指标 | Go 版 | Rust 版 | 原因 |
|------|-------|---------|------|
| 内存 (RSS) | ~50MB | ~10-15MB | 无 GC，DashMap 零拷贝 |
| CPU | 基准 | 低 2-3x | 零拷贝 JSON 解析 |
| P99 延迟 | 基准 | 低 2-5x | 无 GC 停顿 |
| 启动时间 | ~500ms | ~50ms | 无运行时初始化 |

## 兼容性验证

```bash
# 1. 启动 Go 版，等待数据到达
# 2. 抓取 Go 版 JSON 快照
bash scripts/capture-snapshots.sh

# 3. 运行兼容性测试
cargo test -p md-tests
```

测试验证：
- Tick JSON 字段名、顺序、精度与 Go 版一致
- Kline JSON 字段名、顺序、精度与 Go 版一致
- WebSocket Gateway 推送格式（type/topic/data）
- 错误响应格式（`{"error":"..."}`）

## 运维

### 信号处理

| 信号 | 行为 |
|------|------|
| SIGINT (Ctrl+C) | 优雅关停 |
| SIGTERM | 优雅关停 |
| SIGHUP | 热重载配置（重新加载 config.yaml） |

### 日志

```bash
# 默认 info 级别
cargo run -- --config config.yaml

# 调试级别
RUST_LOG=debug cargo run -- --config config.yaml

# 仅看 connector 日志
RUST_LOG=md_connector=debug,info cargo run -- --config config.yaml
```

### Prometheus 集成

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'md-server-rust'
    static_configs:
      - targets: ['localhost:8091']
    metrics_path: '/metrics'
```

主要指标（完整列表见 [DEPLOYMENT.md §7.2](DEPLOYMENT.md#72-prometheus-指标全集)）：
- `md_ticks_processed` / `md_klines_processed` — 已处理 Tick/Kline 总数
- `md_ticks_dropped` / `md_klines_dropped` — 丢弃数（channel 满）
- `md_ingestion_latency_ms{exchange,type,symbol,interval}` — 多维入库延迟直方图
- `md_gateway_internal_latency_ms{topic}` — 网关内部转发延迟直方图
- `md_ws_active_clients` / `md_ws_kicked_lagged_total` — WebSocket 活跃连接 / 慢客户端踢出
- `process_resident_memory_bytes` / `process_cpu_seconds_total` / `process_open_fds` … — 标准进程指标

## 已知差异

| 项目 | Go 版 | Rust 版 | 影响 |
|------|-------|---------|------|
| WebSocket 心跳 | gorilla ping | tungstenite ping | 协议层兼容，应用层无差异 |
| OKX candle close_time | Go 版可能不同 | 估算 (ts + 59999ms) | 仅影响未闭合 Kline |
| OKX number_of_trades | Go 版可能有值 | 0（OKX 不提供） | 仅影响 OKX candle |
| 热重载 | 完整重连 | 仅重新加载配置 | 连接器不重启（后续优化） |

## 项目结构

```
rest重构/
├── Cargo.toml
├── config.yaml
├── README.md
├── TDD-PHASE1.md / TDD-PHASE2.md
├── REWRITE-PLAN.md
├── scripts/capture-snapshots.sh
└── crates/
    ├── md-domain/        # 领域类型
    ├── md-config/        # 配置加载
    ├── md-proto/         # protobuf
    ├── md-connector/     # Binance + OKX 连接器
    ├── md-processor/     # 缓存 + PubSub
    ├── md-grpc/          # gRPC 服务
    ├── md-gateway/       # REST + WebSocket
    ├── md-server/        # 二进制入口
    └── md-tests/         # 兼容性测试
```

## License

与原 Go 项目保持一致。
