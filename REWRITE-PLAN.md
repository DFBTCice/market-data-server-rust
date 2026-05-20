# Market Data Server -- Rust 重写计划

目标：用 Rust 100% 重写 Go 版 market-data-server，保持所有外部接口字节级兼容，仅做内部性能优化。

---

## 1. 项目结构（Cargo Workspace）

```
rest重构/
├── Cargo.toml                    # workspace 根
├── config.yaml                   # 与 Go 版完全相同的配置文件
├── proto/                        # protobuf 定义（从 Go 项目复制 .proto）
│   ├── marketdata.proto
│   └── admin.proto
├── crates/
│   ├── md-proto/                 # protobuf 生成代码 + 领域类型
│   │   ├── build.rs              # tonic-build 编译 proto
│   │   └── src/lib.rs            # 生成的 gRPC 代码 + DataType 枚举
│   │
│   ├── md-config/                # 配置加载（对标 internal/config）
│   │   └── src/lib.rs            # serde YAML 解析 + 环境变量覆盖
│   │
│   ├── md-domain/                # 领域核心（零 I/O）
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── topic.rs          # Topic 格式解析/构建 + symbol 归一化
│   │       ├── types.rs          # Tick, Kline 内部领域类型（非 protobuf）
│   │       └── subscription.rs   # SubscriptionTarget, DataType
│   │
│   ├── md-connector/             # 交易所连接器（对标 internal/connector）
│   │   └── src/
│   │       ├── lib.rs            # Connector trait 定义
│   │       ├── base.rs           # BaseConnector 公共逻辑
│   │       ├── binance.rs        # Binance 实现 ✅
│   │       └── okx.rs            # OKX 实现 ✅
│   │
│   ├── md-processor/             # 数据处理（对标 internal/processor）
│   │   └── src/
│   │       ├── lib.rs            # Processor 组合入口
│   │       ├── cache.rs          # LatestValueCache trait + 实现
│   │       ├── pubsub.rs         # PubSub trait + 实现
│   │       └── metrics.rs        # 指标收集
│   │
│   ├── md-grpc/                  # gRPC 服务（对标 internal/server/grpc + admin）
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── market_data.rs    # MarketDataService 实现
│   │       └── admin.rs          # AdminService 实现
│   │
│   ├── md-gateway/               # API Gateway（对标 internal/apigateway）
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── rest.rs           # REST handlers
│   │       └── ws.rs             # WebSocket hub + client
│   │
│   └── md-server/                # 二进制入口（对标 cmd/dataserver）
│       ├── Cargo.toml            # 依赖所有 crate
│       └── src/
│           └── main.rs           # 启动编排、信号处理、热重载
│
└── tests/                        # 集成测试
    ├── compatibility/            # 兼容性测试（对比 Go 输出）
    │   ├── json_snapshots.rs     # JSON 字段顺序/精度快照
    │   ├── grpc_echo.rs          # gRPC 请求-响应对比
    │   └── ws_format.rs          # WebSocket 推送格式对比
    └── performance/              # 性能对比测试
        └── bench.rs
```

### Crate 依赖关系

```
md-server
  ├── md-grpc       ──→ md-processor, md-proto, md-domain
  ├── md-gateway    ──→ md-proto, md-domain (REST 层直调 processor 接口)
  ├── md-connector  ──→ md-domain, md-proto
  ├── md-processor  ──→ md-domain
  ├── md-config
  └── md-proto

md-domain (零外部依赖，仅 serde)
md-proto  (tonic, prost)
md-config (serde, serde_yaml, log)
```

---

## 2. Top 3 深度化机会的 Rust 设计

### 2.1 Processor 拆分 -- Cache / PubSub / Metrics

Go 版问题：Processor 是上帝对象，同时承担缓存、pub/sub、channel broker、指标收集。

Rust 设计：拆为三个独立 trait + 实现，通过组合器聚合。

```rust
// ---- md-domain/src/types.rs ----
/// 内部领域类型，与 protobuf 解耦
/// 价格和数量用 String 保持与 Go 版 JSON 输出一致（避免浮点精度问题）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    pub exchange: String,
    pub symbol: String,
    pub timestamp: i64,
    pub price: String,
    pub quantity: String,
    pub trade_id: i64,
    pub is_buyer_maker: bool,
    pub best_bid_price: String,
    pub best_bid_quantity: String,
    pub best_ask_price: String,
    pub best_ask_quantity: String,
    pub exchange_event_ts: i64,
    pub connector_receive_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    pub exchange: String,
    pub symbol: String,
    pub interval: String,
    pub open_time: i64,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
    pub close_time: i64,
    pub quote_asset_volume: String,
    pub number_of_trades: i64,
    pub is_closed: bool,
    pub exchange_event_ts: i64,
    pub connector_receive_ts: i64,
}

// ---- md-processor/src/cache.rs ----
use std::sync::Arc;

pub trait LatestCache: Send + Sync + 'static {
    fn update_tick(&self, tick: Tick);
    fn update_kline(&self, kline: Kline);
    fn get_tick(&self, exchange: &str, symbol: &str) -> Option<Tick>;
    fn get_kline(&self, exchange: &str, symbol: &str, interval: &str) -> Option<Kline>;
}

/// 基于 dashmap 的实现，支持并发读写
pub struct DashMapCache {
    ticks: dashmap::DashMap<String, Tick>,     // key: "exchange:symbol"
    klines: dashmap::DashMap<String, Kline>,   // key: "exchange:symbol:interval"
}

// ---- md-processor/src/pubsub.rs ----
pub trait PubSub: Send + Sync + 'static {
    fn subscribe(&self, topics: Vec<String>) -> Result<Subscription, Error>;
    fn unsubscribe(&self, id: &str);
    fn publish_tick(&self, tick: &Tick);
    fn publish_kline(&self, kline: &Kline);
}

pub struct Subscription {
    pub id: String,
    pub receiver: tokio::sync::broadcast::Receiver<DataEvent>,
}

pub enum DataEvent {
    Tick(Arc<Tick>),
    Kline(Arc<Kline>),
}

/// 基于 tokio::broadcast 的实现
pub struct BroadcastPubSub {
    topics: dashmap::DashMap<String, tokio::sync::broadcast::Sender<DataEvent>>,
    subscribers: dashmap::DashMap<String, ()>,
}

// ---- md-processor/src/metrics.rs ----
pub trait MetricsRecorder: Send + Sync + 'static {
    fn record_tick_received(&self, exchange: &str);
    fn record_kline_received(&self, exchange: &str);
    fn record_subscription_count(&self, count: usize);
}

/// Prometheus 指标实现
pub struct PrometheusMetrics {
    tick_counter: prometheus::CounterVec,
    kline_counter: prometheus::CounterVec,
    subscription_gauge: prometheus::Gauge,
}

// ---- md-processor/src/lib.rs ----
/// 组合器：持有三个 trait object，对外提供统一接口
pub struct Processor {
    cache: Arc<dyn LatestCache>,
    pubsub: Arc<dyn PubSub>,
    metrics: Arc<dyn MetricsRecorder>,
    tick_tx: tokio::sync::mpsc::Sender<Tick>,
    kline_tx: tokio::sync::mpsc::Sender<Kline>,
}

impl Processor {
    pub fn new(
        cache: Arc<dyn LatestCache>,
        pubsub: Arc<dyn PubSub>,
        metrics: Arc<dyn MetricsRecorder>,
        buffer_size: usize,
    ) -> Self { ... }

    /// 供 connector 调用的 channel
    pub fn tick_sender(&self) -> tokio::sync::mpsc::Sender<Tick> { ... }
    pub fn kline_sender(&self) -> tokio::sync::mpsc::Sender<Kline> { ... }

    /// 供 server 调用的查询接口
    pub fn get_latest_tick(&self, exchange: &str, symbol: &str) -> Option<Tick> { ... }
    pub fn get_latest_kline(&self, exchange: &str, symbol: &str, interval: &str) -> Option<Kline> { ... }
    pub fn subscribe(&self, topics: Vec<String>) -> Result<Subscription, Error> { ... }
    pub fn unsubscribe(&self, id: &str) { ... }
}
```

### 2.2 Connector 抽象 -- trait + BaseConnector

Go 版问题：Binance 和 OKX 各 1000 行，400-500 行重复的连接管理逻辑。

Rust 设计：`Connector` trait 定义外部接口，`BaseConnector` 封装公共 WebSocket 管理。

```rust
// ---- md-connector/src/lib.rs ----
use async_trait::async_trait;

/// 交易所连接器 trait -- 对标 Go 的 common.Connector
#[async_trait]
pub trait Connector: Send + Sync {
    /// 连接器名称（如 "binance", "okx"）
    fn name(&self) -> &str;

    /// 启动连接，开始向 channel 发送数据
    async fn start(&self) -> Result<(), ConnectorError>;

    /// 优雅停止
    async fn stop(&self) -> Result<(), ConnectorError>;

    /// 动态添加订阅
    async fn add_subscriptions(&self, targets: Vec<SubscriptionTarget>) -> Result<(), ConnectorError>;

    /// 动态移除订阅
    async fn remove_subscriptions(&self, targets: Vec<SubscriptionTarget>) -> Result<(), ConnectorError>;

    /// 获取当前订阅列表
    fn current_subscriptions(&self) -> Vec<SubscriptionTarget>;
}

/// 连接器需实现此 trait 来定义交易所特定逻辑
#[async_trait]
pub trait ExchangeAdapter: Send + Sync + 'static {
    /// WebSocket 连接 URL
    fn ws_url(&self) -> &str;

    /// 构建订阅消息（JSON）
    fn build_subscribe_msg(&self, streams: &[String]) -> String;

    /// 构建取消订阅消息
    fn build_unsubscribe_msg(&self, streams: &[String]) -> String;

    /// 解析收到的 WebSocket 消息，返回 Tick/Kline 事件
    fn parse_message(&self, raw: &[u8]) -> Result<Vec<DataEvent>, ParseError>;

    /// 将 SubscriptionTarget 映射为交易所特定的 stream key
    fn target_to_streams(&self, target: &SubscriptionTarget) -> Vec<String>;

    /// 心跳消息（如有）
    fn heartbeat_message(&self) -> Option<String>;

    /// 心跳间隔
    fn ping_interval(&self) -> Duration;
}

// ---- md-connector/src/base.rs ----
/// 公共 WebSocket 管理逻辑 -- 对标 Go 版两个连接器的重复代码
pub struct BaseConnector<A: ExchangeAdapter> {
    adapter: A,
    config: ConnectorConfig,
    state: Arc<ConnectorState>,
    tick_tx: tokio::sync::mpsc::Sender<Tick>,
    kline_tx: tokio::sync::mpsc::Sender<Kline>,
}

struct ConnectorState {
    desired_subs: RwLock<HashSet<SubscriptionTarget>>,
    active_subs: RwLock<HashSet<String>>,  // stream keys
    connection: RwLock<Option<WebSocketConnection>>,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl<A: ExchangeAdapter> BaseConnector<A> {
    /// 连接循环：连接 -> 读消息 -> 断开 -> 重连
    pub async fn run_connection_loop(&self) { ... }

    /// 读消息循环：调 adapter.parse_message -> 发送到 channel
    pub async fn read_messages(&self) { ... }

    /// 订阅同步：diff desired vs active -> 发送订阅/取消订阅
    pub async fn sync_subscriptions(&self) { ... }

    /// 心跳 goroutine
    pub async fn heartbeat_loop(&self) { ... }
}

impl<A: ExchangeAdapter> Connector for BaseConnector<A> {
    fn name(&self) -> &str { self.adapter.name() }
    async fn start(&self) -> Result<(), ConnectorError> { ... }
    async fn stop(&self) -> Result<(), ConnectorError> { ... }
    async fn add_subscriptions(&self, targets: Vec<SubscriptionTarget>) -> Result<(), ConnectorError> { ... }
    async fn remove_subscriptions(&self, targets: Vec<SubscriptionTarget>) -> Result<(), ConnectorError> { ... }
    fn current_subscriptions(&self) -> Vec<SubscriptionTarget> { ... }
}

// ---- md-connector/src/binance.rs ----
/// Binance 适配器 -- 只需实现消息解析和 stream 映射
pub struct BinanceAdapter { config: BinanceConfig }

#[async_trait]
impl ExchangeAdapter for BinanceAdapter {
    fn ws_url(&self) -> &str { &self.config.stream_base_url }

    fn parse_message(&self, raw: &[u8]) -> Result<Vec<DataEvent>, ParseError> {
        // 纯函数，可独立单元测试
        // 解析 aggTrade -> Tick, kline -> Kline
    }

    fn target_to_streams(&self, target: &SubscriptionTarget) -> Vec<String> {
        // TICK -> ["<symbol>@aggTrade"]
        // KLINE -> ["<symbol>@kline_<interval>"]
    }

    // ... 其他方法
}

// ---- md-connector/src/okx.rs ----
/// OKX 适配器 -- 管理两个连接（public + business）
pub struct OkxAdapter { config: OkxConfig }

#[async_trait]
impl ExchangeAdapter for OkxAdapter {
    fn ws_url(&self) -> &str { &self.config.stream_base_url_public }

    fn parse_message(&self, raw: &[u8]) -> Result<Vec<DataEvent>, ParseError> {
        // 解析 trades -> Tick, candle -> Kline
    }

    fn target_to_streams(&self, target: &SubscriptionTarget) -> Vec<String> {
        // TICK -> ["tickers:<SYMBOL>"]
        // KLINE -> ["candle<INTERVAL>:<SYMBOL>"]
    }

    // ... 其他方法
}
```

### 2.3 Topic 归一化

Go 版问题：Topic 格式在 4 处独立编码，symbol 归一化在 7 处散布。

Rust 设计：单一 `md-domain::topic` 模块，所有 crate 统一调用。

```rust
// ---- md-domain/src/topic.rs ----

/// Topic 格式常量（与 Go 版完全一致）
pub const TICK_PREFIX: &str = "tick";
pub const KLINE_PREFIX: &str = "kline";

/// 解析 topic 字符串
/// "tick.binance.BTCUSDT" -> Topic::Tick { exchange: "binance", symbol: "BTCUSDT" }
/// "kline.1m.binance.BTCUSDT" -> Topic::Kline { interval: "1m", exchange: "binance", symbol: "BTCUSDT" }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Topic {
    Tick { exchange: String, symbol: String },
    Kline { interval: String, exchange: String, symbol: String },
}

impl Topic {
    pub fn parse(s: &str) -> Result<Self, TopicError> { ... }
    pub fn format(&self) -> String { ... }
}

/// Symbol 归一化 -- 全局唯一入口
pub fn normalize_symbol(symbol: &str) -> String {
    symbol.to_uppercase()
}

/// Topic 格式构建 -- 全局唯一入口
pub fn tick_topic(exchange: &str, symbol: &str) -> String {
    format!("tick.{}.{}", exchange, normalize_symbol(symbol))
}

pub fn kline_topic(exchange: &str, symbol: &str, interval: &str) -> String {
    format!("kline.{}.{}.{}", interval, exchange, normalize_symbol(symbol))
}

/// Cache key 构建 -- 与 Go 版 getTickCacheKey / getKlineCacheKey 一致
pub fn tick_cache_key(exchange: &str, symbol: &str) -> String {
    format!("{}:{}", exchange.to_uppercase(), symbol.to_uppercase())
}

pub fn kline_cache_key(exchange: &str, symbol: &str, interval: &str) -> String {
    format!("{}:{}:{}", exchange.to_uppercase(), symbol.to_uppercase(), interval)
}
```

---

## 3. JSON 输出字节级兼容方案

### 3.1 核心原则

Go 版使用 `encoding/json` 默认行为：
- 字段顺序：按 struct 字段定义顺序
- 数值精度：int64 直接输出，价格/数量为 string 类型
- bool：`true`/`false`
- null vs 空数组：`resp.Subscriptions == nil` 时返回 `[]`

### 3.2 Rust 实现策略

```rust
// 使用 serde_json，通过 #[serde] 属性控制字段顺序和格式
// 字段顺序由 struct 定义顺序决定（与 Go 一致）

// 关键：Tick 和 Kline 的 protobuf 序列化格式
// Go 版 gRPC JSON 序列化使用 protojson，字段名为 camelCase
// REST API 返回的是 protobuf struct 的直接 JSON 序列化

// 策略 1：REST API 返回使用自定义序列化（与 Go protojson 输出一致）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]  // protojson 格式
pub struct TickJson {
    pub exchange: String,
    pub symbol: String,
    pub timestamp: i64,
    pub price: String,        // string 类型，不是数字
    pub quantity: String,
    pub trade_id: i64,
    pub is_buyer_maker: bool,
    pub best_bid_price: String,
    pub best_bid_quantity: String,
    pub best_ask_price: String,
    pub best_ask_quantity: String,
    pub exchange_event_ts: i64,
    pub connector_receive_ts: i64,
}

// 策略 2：gRPC 层直接使用 tonic/prost 生成的类型
// tonic 的 protobuf 序列化与 Go 的 protojson 格式一致

// 策略 3：WebSocket 推送格式 -- 与 Go 版 json.Marshal 输出完全一致
// 需要确认 Go 版 WebSocket 推送的是 protobuf JSON 还是自定义格式
```

### 3.3 兼容性测试方案

```rust
// tests/compatibility/json_snapshots.rs

/// 从 Go 版抓取的真实 JSON 快照
const GO_TICK_JSON: &str = r#"{"exchange":"binance","symbol":"BTCUSDT","timestamp":1234567890,...}"#;
const GO_KLINE_JSON: &str = r#"{"exchange":"binance","symbol":"BTCUSDT","interval":"1m",...}"#;

#[test]
fn tick_json_matches_go_output() {
    let tick = TickJson { /* 填充与快照相同的数据 */ };
    let rust_json = serde_json::to_string(&tick).unwrap();
    assert_eq!(rust_json, GO_TICK_JSON);
}

#[test]
fn kline_json_matches_go_output() {
    let kline = KlineJson { /* ... */ };
    let rust_json = serde_json::to_string(&kline).unwrap();
    assert_eq!(rust_json, GO_KLINE_JSON);
}
```

### 3.4 需要确认的兼容点

| 接口 | 需确认内容 |
|------|-----------|
| REST JSON | 字段顺序、null 处理、数字精度（int64 是否有引号） |
| gRPC binary | protobuf 编码天然兼容，无需额外处理 |
| gRPC JSON (grpcurl) | protojson 字段名 camelCase，需确认 Go 版是否启用 |
| WebSocket push | 确认推送格式是 protobuf JSON 还是自定义格式 |
| 错误响应 | `{"error": "..."}` 格式、HTTP status code 映射 |

---

## 4. 建议开发顺序

### Phase 1：最小可用版本（能启动、能连交易所）✅

| 步骤 | 模块 | 说明 | 依赖 |
|------|------|------|------|
| 1 | `md-proto` | 编译 proto，生成 tonic 代码 | 无 |
| 2 | `md-domain` | Topic、types、subscription | 无 |
| 3 | `md-config` | 加载 config.yaml，环境变量覆盖 | md-domain |
| 4 | `md-connector` | BaseConnector + BinanceAdapter | md-domain, md-proto |
| 5 | `md-server` | main.rs：加载配置、启动 Binance 连接器、打印收到的数据 | 全部 |

验证标准：`cargo run -- --config config.yaml` 能连接 Binance 并打印 Tick 数据。✅

### Phase 2：数据通路打通 ✅

| 步骤 | 模块 | 说明 |
|------|------|------|
| 6 | `md-processor` | Cache + PubSub + Metrics ✅ |
| 7 | `md-grpc::market_data` | MarketDataService gRPC 实现 ✅ |
| 8 | `md-grpc::admin` | AdminService gRPC 实现 ✅ |
| 9 | `md-gateway` | REST + WebSocket (Gateway + Legacy 格式) ✅ |

验证标准：`grpcurl` 能调用 `GetLatestTick` 返回数据。✅

### Phase 3：收尾 + 运维 ✅

| 步骤 | 模块 | 说明 |
|------|------|------|
| 10 | `md-connector::okx` | OKX 适配器 ✅ |
| 11 | `md-server` 完善 | SIGTERM 优雅关停、SIGHUP 热重载 ✅ |
| 12 | `/health` + `/metrics` | 健康检查 + Prometheus 指标 ✅ |
| 13 | 兼容性测试 | JSON 快照对比框架 ✅ |

验证标准：REST API 和 WebSocket 输出与 Go 版字节级一致。✅（需运行 capture-snapshots.sh）

---

## 5. 性能对比测试方案

### 5.1 测试指标

| 指标 | 测量方法 | 预期 Rust 优势 |
|------|----------|---------------|
| 内存占用 | RSS（`ps -o rss`） | 低 3-5x（无 GC） |
| CPU 使用率 | `top` / `pidstat` | 低 2-3x（零拷贝解析） |
| 消息延迟（P50/P99） | connector_receive_ts 到 server 发送时间 | 低 2-5x（无 GC 停顿） |
| 吞吐量 | 每秒处理 Tick/Kline 数 | 高 2-3x |
| 启动时间 | 进程启动到首条数据到达 | 快 5-10x |
| 二进制大小 | `ls -lh` | 类似或更小 |

### 5.2 测试工具

```bash
# 同时启动 Go 和 Rust 版本，订阅相同交易对
./market-data-server --config config-go.yaml &
cargo run -- --config config-rust.yaml &

# 使用 vegeta 或 hey 进行 REST API 负载测试
echo "GET http://localhost:8081/api/v1/data/latest/tick/binance/BTCUSDT" | \
  vegeta attack -duration=60s -rate=1000 | vegeta report

# 使用 ghz 进行 gRPC 负载测试
ghz --insecure --total 10000 --concurrency 100 \
  --call marketdata.MarketDataService/GetLatestTick \
  -d '{"exchange":"binance","symbol":"BTCUSDT"}' \
  localhost:50051

# 内存和 CPU 对比
pidstat -r -p $(pgrep market-data-server) 1 > go_mem.txt &
pidstat -r -p $(pgrep md-server) 1 > rust_mem.txt &
```

### 5.3 对比脚本结构

```
tests/performance/
├── README.md           # 测试步骤说明
├── config-go.yaml      # Go 版专用配置（不同端口）
├── config-rust.yaml    # Rust 版专用配置（不同端口）
├── bench_rest.sh       # REST API 负载测试
├── bench_grpc.sh       # gRPC 负载测试
├── bench_memory.sh     # 内存对比
└── compare.py          # 结果对比报告生成
```

---

## 6. 关键技术选型

| 组件 | Go 版 | Rust 版 | 说明 |
|------|-------|---------|------|
| 异步运行时 | goroutine | tokio | Rust 标准选择 |
| gRPC | google.golang.org/grpc | tonic + prost | Rust 生态最成熟 |
| WebSocket | gorilla/websocket + nhooyr | tokio-tungstenite | 统一为一个库 |
| HTTP | gorilla/mux | axum | tokio 生态，性能优秀 |
| 序列化 | encoding/json | serde_json | 必选 |
| 配置 | viper | serde_yaml + envy | YAML 解析 + 环境变量 |
| 日志 | logrus | tracing | 结构化日志，支持分布式追踪 |
| 指标 | prometheus-go | prometheus (rust) | 兼容现有 Grafana |
| 锁 | sync.RWMutex | tokio::sync::RwLock + dashmap | 异步锁 + 并发 map |
| 随机 ID | uuid | uuid | 相同 |

---

## 7. CONTEXT.md -- 领域术语

```markdown
# 领域术语表

- **Exchange（交易所）**: Binance、OKX 等提供市场数据的平台
- **Connector（连接器）**: 与单个交易所的 WebSocket 连接管理器
- **Tick**: 逐笔成交数据（aggTrade），包含价格、数量、买卖方向
- **Kline（K线）**: 聚合的 OHLCV 数据，按时间周期（1m, 5m, 1h 等）
- **Topic（主题）**: 数据订阅标识，格式为 `tick.<exchange>.<symbol>` 或 `kline.<interval>.<exchange>.<symbol>`
- **SubscriptionTarget（订阅目标）**: 交易所 + 数据类型 + 交易对的组合
- **PubSub（发布订阅）**: 消息分发机制，支持多消费者订阅同一 topic
- **Cache（缓存）**: 最新 Tick/Kline 的快照存储
- **Processor（处理器）**: 中央数据处理模块，组合 Cache + PubSub + Metrics
- **API Gateway**: HTTP/WebSocket 入口层，对外暴露 REST 和 WS 接口
- **Hot Reload（热重载）**: 通过 SIGHUP 信号重新加载配置，不停机更新连接器
```
