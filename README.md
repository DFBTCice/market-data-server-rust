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
| GET | `/api/v1/data/latest/quote/{exchange}/{symbol}` | 获取最新 Quote（买一卖一，1.1.0 新增，需开启 quote 开关） |
| GET | `/api/v1/subscriptions` | 获取当前订阅列表（开启 quote 后会多出 `data_type: 3` 的条目） |
| POST | `/api/v1/subscriptions` | 添加订阅，`type` 取 `TICK` / `KLINE` / `QUOTE`（`QUOTE` 为 1.1.0 新增） |
| DELETE | `/api/v1/subscriptions` | 移除订阅，`type` 同上 |
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

> 1.1.0 **没有新增任何 gRPC RPC**。`admin.proto` 的 `DataType` 枚举仅**追加**了 `QUOTE = 3`
> （既有 `UNKNOWN=0 / TICK=1 / KLINE=2` 取值与全部消息字段号不变），供 AdminService 的订阅增删/列表使用。

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

> **Legacy `/ws` 永远不会推送 quote**，订阅 `quote.*` 会被明确拒绝
> （`{"error":"quote topics are not supported on legacy /ws, use /ws/v1/data: quote.…"}`）。
> 旧消费者无法按主题过滤，新增消息类型会破坏它们，因此 quote 只在 Gateway（`/ws/v1/data`）提供。

## Quote 报价主题（1.1.0 新增）

买一卖一（盘口最优报价）作为一个**独立的新主题**提供，与 `tick.*` / `kline.*` 完全并行、互不影响。
默认**关闭**；不改配置时，行为与 1.0.0 完全一致。

### 主题与消息

- 主题：`quote.<交易所小写>.<交易对大写>`，例如 `quote.binance.BNBUSDT`、`quote.okx.BNB-USDT-SWAP`
- 仅 Gateway WebSocket（`/ws/v1/data`）与 REST 提供
- 推送（数值一律为字符串，时间戳为毫秒整数）：

```json
{"type":"quote","topic":"quote.binance.BNBUSDT","data":{"exchange":"binance","symbol":"BNBUSDT","bid_price":"743.90","bid_qty":"12.3","ask_price":"743.91","ask_qty":"8.1","last_price":"","timestamp":1791531605419,"connector_receive_ts":1791531605962}}
```

| 字段 | 说明 |
|------|------|
| `bid_price` / `bid_qty` | 买一价 / 买一量 |
| `ask_price` / `ask_qty` | 卖一价 / 卖一量 |
| `last_price` | 最新成交价。**币安 bookTicker 不含该字段 → 恒为空字符串 `""`（字段仍会输出）**；OKX 取 `tickers.last` |
| `timestamp` | 交易所事件时间（ms） |
| `connector_receive_ts` | 本服务收到该条消息的时间（ms） |

> quote 的 JSON **不使用 omitempty**：空值字段也会输出，消费者无需判断字段是否存在。

数据源：币安 U 本位合约 `<symbol>@bookTicker`（Public 类流，独立连接
`wss://fstream.binance.com/public/stream`——bookTicker 在 `/market/stream` 上收不到数据）；
OKX 公共频道 `tickers`（独立连接）。quote 使用**独立的 WebSocket 连接**，
tick/kline 的既有连接不受影响。

### 限频与慢客户端

- 同一 quote 主题对外推送**不超过 10 条/秒**（相邻两次 ≥100ms；窗口内的多次更新合并，最新值一定会被推出，REST 缓存始终是最新值）。
- 慢客户端处理沿用既有机制（连续 3 次 broadcast Lagged 即踢出），不单独设计。

### 开启方式（配置，全部可选，默认值如下）

```yaml
processor:
  quote_channel_buffer: 1000        # 默认 1000

connectors:
  binance:
    quotes_enabled: false           # 默认 false：不创建任何 quote 连接
    stream_base_url_public: "wss://fstream.binance.com/public/stream"  # 默认值
    subscribe_quotes: ["BNBUSDT"]   # 默认空
  okx:
    quotes_enabled: false           # 默认 false
    subscribe_quotes: ["BNB-USDT-SWAP"]
```

- 旧版 `config.yaml`（不含以上任何键）原样可用，启动日志也不会多出任何告警。
- `quotes_enabled=true` 但 `subscribe_quotes` 为空时，quote 连接器不会启动（并给出告警）：
  OKX 会关闭长时间没有订阅的连接，空连接只会产生重连噪声。

### 订阅示例

```bash
# WebSocket（Gateway）
{"action":"subscribe","streams":["quote.binance.BNBUSDT","quote.okx.BNB-USDT-SWAP"]}
{"action":"unsubscribe","streams":["quote.binance.BNBUSDT"]}

# REST：最新一条（未开启或还没收到数据时 404）
curl http://localhost:8081/api/v1/data/latest/quote/binance/BNBUSDT

# REST：动态增删订阅（要求对应交易所的 quote 连接器正在运行，否则 404 "quote connector not enabled: <exchange>"）
curl -X POST   http://localhost:8081/api/v1/subscriptions -H 'Content-Type: application/json' \
     -d '{"exchange":"binance","type":"QUOTE","symbols":["ETHUSDT"]}'
curl -X DELETE http://localhost:8081/api/v1/subscriptions -H 'Content-Type: application/json' \
     -d '{"exchange":"binance","type":"QUOTE","symbols":["ETHUSDT"]}'
```

> 动态订阅与 tick/kline 一样**不会写回配置文件**，服务重启后丢失，以 `config.yaml` 为准。

### 热重载范围（SIGHUP）

| 配置项 | 热更 | 说明 |
|--------|------|------|
| `subscribe_quotes` 增删 | ✅ | 前提：对应 quote 连接器已在运行；否则日志提示"需要重启" |
| `quotes_enabled` | ❌ | 需重启（日志会提示） |
| `binance.stream_base_url_public` | ❌ | 需重启 |
| `processor.quote_channel_buffer` | ❌ | 需重启 |

### 监控指标（只新增，旧指标名与标签不变）

- `md_quotes_processed` / `md_quotes_dropped`
- `md_ingestion_latency_ms{type="quote",...}`（新增的 `type` 取值）
- `md_ws_messages_sent_total{kind="quote"}`、`md_broadcast_lagged_total{topic_kind="quote"}`（新增的标签取值）
- `md_connector_*{exchange="binance-quote"|"okx-quote"}`（quote 连接器自己的连接状态/重连/订阅失败）
- `md_gateway_internal_latency_ms{topic="quote.…"}`

### 兼容性（只增不改）

开关关闭时，与 1.0.0 相比：tick/kline 的推送字段、字段顺序、数值格式、频率不变；
REST、`/api/v1/subscriptions`（`data_type` TICK=1 / KLINE=2）不变；`/metrics` 只增不减；gRPC 不变。
开关开启后，Legacy `/ws` 仍不含 quote，`/api/v1/subscriptions` 仅**多出** `data_type: 3` 的条目。
证据与验证方法见 `evidence/1.1.0/` 与 `scripts/compat/`。

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
# 2. 抓取 JSON 快照（可用 SNAPSHOT_REST / SNAPSHOT_WS 指向任意实例）
bash scripts/capture-snapshots.sh

# 3. 运行兼容性测试（--nocapture 可看到每条用例的输出；快照缺失/为空会直接失败，不再静默跳过）
cargo test -p md-tests -- --nocapture
```

1.1.0 起新增的新旧版本对比工具（抓取 WebSocket 推送原文 → 逐字节比较）：

```bash
node scripts/compat/capture_ws.mjs gateway ws://HOST:8081/ws/v1/data 60 out/old tick.binance.BNBUSDT
node scripts/compat/compare.mjs push out/old out/new tick.binance.BNBUSDT   # 去掉 connector_receive_ts 后逐字节比较
node scripts/compat/compare.mjs noquote out/new-legacy                       # 断言旧 /ws 没有 quote
node scripts/compat/compare.mjs subs old.json new.json [--allow-data-type=3]
node scripts/compat/compare.mjs metrics old.txt new.txt                      # 只允许新增
node scripts/compat/compare.mjs selftest                                     # 正对照：证明工具真的能发现差异
node scripts/compat/analyze_quote.mjs out/new quote.okx.BNB-USDT-SWAP --control tick.okx.BNB-USDT-SWAP   # quote 验收（间隔/节流/非空/字段序/正对照）
node scripts/compat/restore_subs.mjs before-subscriptions.json http://HOST:8081 [--apply]                # 重启后补回丢失的动态订阅（默认 dry-run）
scripts/compat/sample_process.sh http://HOST:8081 600 30 out/process.tsv                                  # 10 分钟进程指标采样
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
- `md_quotes_processed` / `md_quotes_dropped` — quote 处理/丢弃数（1.1.0 新增）
- `md_ingestion_latency_ms{exchange,type,symbol,interval}` — 多维入库延迟直方图
- `md_gateway_internal_latency_ms{topic}` — 网关内部转发延迟直方图
- `md_ws_active_clients` / `md_ws_kicked_lagged_total` — WebSocket 活跃连接 / 慢客户端踢出
- `md_ws_subscriptions_active` — 全部 WS 连接当前持有的 broadcast 接收端总数（1.1.1 新增；退订/断线后应回落，只增不减即泄漏）
- `md_ws_kicked_write_timeout_total` — Gateway 客户端因写超时（`api_gateway.ws_write_wait`）被断开的累计数（1.1.1 新增）
- `md_jemalloc_{allocated,active,resident,retained,mapped,metadata}_bytes` — 仅 `--features md-server/jemalloc-stats` 构建（jemalloc 分配器）暴露的分配器统计（1.1.1 新增）
- `md_tokio_alive_tasks` / `md_tokio_workers` / `md_tokio_global_queue_depth` — tokio 运行时存活 task / 工作线程 / 全局队列（1.1.1 新增，排查任务泄漏用）
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
├── scripts/compat/            # 兼容性对比与验收工具（capture_ws / compare / analyze_quote / restore_subs / sample_process）
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
