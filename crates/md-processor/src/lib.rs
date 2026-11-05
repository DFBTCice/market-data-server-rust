use dashmap::DashMap;
use md_domain::topic;
use md_domain::types::{Kline, Quote, Tick};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};
use uuid::Uuid;

// ---- Metrics ----

/// 入库延迟直方图桶边界（毫秒）。
///
/// 1~50ms 段加密：真实到达延迟（币安 tick≈3ms、OKX tick≈30ms、币安 kline≈8ms）都落在低区，
/// 粗桶会让 histogram_quantile 在桶内线性插值时把 P50/P99 系统性高估
/// （例：旧桶下 OKX 真实 30ms 会被插值成 ~37-50ms，P99 被钉在 (100,250] 桶恒显 ~240ms）。
/// 细分后分位数贴近真实值，同时保留到 5s 的尾部覆盖。
pub const BUCKETS_MS: &[u64] = &[
    1, 2, 3, 5, 8, 12, 16, 20, 25, 30, 40, 50, 75, 100, 150, 250, 500, 1000, 2500, 5000,
];

/// 网关转发延迟直方图桶边界（毫秒）-- 网关内部链路应在亚毫秒级
pub const GATEWAY_LATENCY_BUCKETS_MS: &[u64] = &[1, 2, 5, 10, 25, 50, 100, 250, 500];

/// 固定桶 histogram 实现（零额外依赖）
///
/// 说明：每个桶记录"恰好落入该桶"的计数（非累积），导出 Prometheus 时再累积。
/// 桶边界由构造时的 boundaries 切片决定，最后一个桶是 +Inf（>所有边界）。
#[derive(Debug)]
pub struct Histogram {
    boundaries: Vec<u64>,
    /// 桶计数（长度 = boundaries.len() + 1，最后一个是 +Inf）
    pub buckets: Vec<AtomicU64>,
    /// 总和
    pub sum: AtomicU64,
    /// 总计数
    pub count: AtomicU64,
}

impl Histogram {
    pub fn new(bucket_boundaries: &[u64]) -> Self {
        let mut buckets = Vec::with_capacity(bucket_boundaries.len() + 1);
        for _ in 0..=bucket_boundaries.len() {
            buckets.push(AtomicU64::new(0));
        }
        Self {
            boundaries: bucket_boundaries.to_vec(),
            buckets,
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    /// 桶边界
    pub fn boundaries(&self) -> &[u64] {
        &self.boundaries
    }

    /// 记录一个观测值
    pub fn observe(&self, value: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value, Ordering::Relaxed);
        for (i, &boundary) in self.boundaries.iter().enumerate() {
            if value <= boundary {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        // 超出所有桶边界，放入 +Inf 桶
        let inf_idx = self.boundaries.len();
        self.buckets[inf_idx].fetch_add(1, Ordering::Relaxed);
    }

    /// 快照
    pub fn snapshot(&self) -> HistogramSnapshot {
        HistogramSnapshot {
            boundaries: self.boundaries.clone(),
            buckets: self
                .buckets
                .iter()
                .map(|b| b.load(Ordering::Relaxed))
                .collect(),
            sum: self.sum.load(Ordering::Relaxed),
            count: self.count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistogramSnapshot {
    pub boundaries: Vec<u64>,
    pub buckets: Vec<u64>,
    pub sum: u64,
    pub count: u64,
}

/// 连接器级别的指标（每个连接器一份，通过 Arc 共享）
#[derive(Debug)]
pub struct ConnectorMetrics {
    /// 连接状态 gauge (0/1)
    pub connected: Arc<AtomicU64>,
    /// 重连次数
    pub reconnect_total: Arc<AtomicU64>,
    /// 订阅失败次数
    pub subscribe_failed_total: Arc<AtomicU64>,
}

impl ConnectorMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            connected: Arc::new(AtomicU64::new(0)),
            reconnect_total: Arc::new(AtomicU64::new(0)),
            subscribe_failed_total: Arc::new(AtomicU64::new(0)),
        })
    }
}

/// 入库指标的序列维度 key
///
/// 完全对标 Go 版 `marketdata_processor_ingestion_latency_ms` 的标签集
/// `{exchange, type, symbol, interval}`。一个 histogram 即可同时支撑：
/// - 按交易所聚合：`by (exchange)`
/// - 按交易对（标的）聚合：`by (exchange, symbol)`
/// - 按交易对 + 周期聚合：`by (exchange, symbol, interval)`（Kline）
/// - 吞吐量：用 histogram 的 `_count`
///
/// 说明：本系统只订阅自有策略所需的少量标的，基数可控（交易所 × 标的 × 周期 = 数十条）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SeriesKey {
    pub exchange: String,
    /// "tick"、"kline" 或 "quote"（1.1.0 新增）
    pub kind: &'static str,
    pub symbol: String,
    /// Kline 周期（如 "1m"）；Tick 恒为空字符串
    pub interval: String,
}

/// 全局 Processor 指标（无锁原子计数器 + 多维 series map）
#[derive(Debug)]
pub struct ProcessorMetrics {
    // ---- 全局聚合（顶部 stat + 兼容老查询） ----
    pub ticks_processed: AtomicU64,
    pub klines_processed: AtomicU64,
    pub ticks_dropped: Arc<AtomicU64>,
    pub klines_dropped: Arc<AtomicU64>,
    /// broadcast lagged 计数 (tick)
    pub broadcast_lagged_tick: AtomicU64,
    /// broadcast lagged 计数 (kline)
    pub broadcast_lagged_kline: AtomicU64,
    /// broadcast lagged 计数 (quote)（1.1.0 新增）
    pub broadcast_lagged_quote: AtomicU64,
    /// quote 处理总数 / mpsc 满丢弃数（1.1.0 新增）
    pub quotes_processed: AtomicU64,
    pub quotes_dropped: Arc<AtomicU64>,

    // ---- 入库延迟/吞吐：按 (exchange, type, symbol, interval) 维度 ----
    /// 到达延迟 = connector_receive_ts − 事件推送时间 E。这是"交易所推送 → 本机接收"的真实链路延迟
    /// （币安 tick≈3ms、OKX tick≈30ms），不含交易所内部聚合延迟。对标 Go 版 ingestion_latency_ms。
    pub ingestion_latency: DashMap<SeriesKey, Arc<Histogram>>,
    /// 端到端延迟 = connector_receive_ts − 成交时间 T（仅 tick）。币安含其内部聚合/分发延迟(E−T)，
    /// 与到达延迟对照可判断"高延迟是我们的还是交易所的"；OKX 因只有 ts，与到达延迟相等。
    pub ingestion_e2e_latency: DashMap<SeriesKey, Arc<Histogram>>,

    // ---- 网关 / WebSocket 指标 ----
    /// 当前活跃 WebSocket 连接数（gateway + legacy 合计）
    pub ws_active_clients: AtomicU64,
    /// 当前所有 WS 连接（gateway + legacy）合计持有的 broadcast 接收端数量（1.1.1 新增）。
    /// 正常情况下应等于"各连接已订阅 stream 数之和"；退订/断线后必须回落——
    /// 若只增不减即说明存在接收端泄漏。
    pub ws_subscriptions_active: AtomicU64,
    /// 因 broadcast lagged 累计达到阈值被踢出的客户端总数
    pub ws_kicked_lagged_total: AtomicU64,
    /// 1.1.1：因写超时（`ws_write_wait`）被踢出的 Gateway 客户端累计数
    pub ws_kicked_write_timeout_total: AtomicU64,
    /// 网关推送总条数（每条 WS 消息 +1，按 topic_kind 区分）
    pub ws_messages_sent_tick: AtomicU64,
    pub ws_messages_sent_kline: AtomicU64,
    pub ws_messages_sent_quote: AtomicU64,
    /// 网关内部转发延迟（处理器 publish → 客户端 send 完成，毫秒），按 topic 维度
    /// 对标 Go 版 marketdata_gateway_internal_latency_ms{topic}，dashboard 由此聚合出
    /// 总体延迟（by le）与按 topic 的推送吞吐 TOP10（_count by topic）
    pub gateway_forward_latency: DashMap<String, Arc<Histogram>>,
}

impl Default for ProcessorMetrics {
    fn default() -> Self {
        Self {
            ticks_processed: AtomicU64::new(0),
            klines_processed: AtomicU64::new(0),
            ticks_dropped: Arc::new(AtomicU64::new(0)),
            klines_dropped: Arc::new(AtomicU64::new(0)),
            broadcast_lagged_tick: AtomicU64::new(0),
            broadcast_lagged_kline: AtomicU64::new(0),
            broadcast_lagged_quote: AtomicU64::new(0),
            quotes_processed: AtomicU64::new(0),
            quotes_dropped: Arc::new(AtomicU64::new(0)),
            ingestion_latency: DashMap::new(),
            ingestion_e2e_latency: DashMap::new(),
            ws_active_clients: AtomicU64::new(0),
            ws_subscriptions_active: AtomicU64::new(0),
            ws_kicked_lagged_total: AtomicU64::new(0),
            ws_kicked_write_timeout_total: AtomicU64::new(0),
            ws_messages_sent_tick: AtomicU64::new(0),
            ws_messages_sent_kline: AtomicU64::new(0),
            ws_messages_sent_quote: AtomicU64::new(0),
            gateway_forward_latency: DashMap::new(),
        }
    }
}

impl ProcessorMetrics {
    /// 获取（或惰性创建）某 series 的入库延迟 histogram（到达延迟，recv−E）
    fn ingestion_hist(&self, key: SeriesKey) -> Arc<Histogram> {
        if let Some(h) = self.ingestion_latency.get(&key) {
            return h.clone();
        }
        self.ingestion_latency
            .entry(key)
            .or_insert_with(|| Arc::new(Histogram::new(BUCKETS_MS)))
            .clone()
    }

    /// 获取（或惰性创建）某 series 的端到端延迟 histogram（recv−T）
    fn ingestion_e2e_hist(&self, key: SeriesKey) -> Arc<Histogram> {
        if let Some(h) = self.ingestion_e2e_latency.get(&key) {
            return h.clone();
        }
        self.ingestion_e2e_latency
            .entry(key)
            .or_insert_with(|| Arc::new(Histogram::new(BUCKETS_MS)))
            .clone()
    }

    /// 获取（或惰性创建）某 topic 的网关转发延迟 histogram
    fn gateway_hist(&self, topic: &str) -> Arc<Histogram> {
        if let Some(h) = self.gateway_forward_latency.get(topic) {
            return h.clone();
        }
        self.gateway_forward_latency
            .entry(topic.to_string())
            .or_insert_with(|| Arc::new(Histogram::new(GATEWAY_LATENCY_BUCKETS_MS)))
            .clone()
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let ingestion_series: Vec<SeriesSnapshot> = self
            .ingestion_latency
            .iter()
            .map(|entry| SeriesSnapshot {
                exchange: entry.key().exchange.clone(),
                kind: entry.key().kind,
                symbol: entry.key().symbol.clone(),
                interval: entry.key().interval.clone(),
                latency: entry.value().snapshot(),
            })
            .collect();

        let ingestion_e2e_series: Vec<SeriesSnapshot> = self
            .ingestion_e2e_latency
            .iter()
            .map(|entry| SeriesSnapshot {
                exchange: entry.key().exchange.clone(),
                kind: entry.key().kind,
                symbol: entry.key().symbol.clone(),
                interval: entry.key().interval.clone(),
                latency: entry.value().snapshot(),
            })
            .collect();

        let gateway_series: Vec<GatewaySeriesSnapshot> = self
            .gateway_forward_latency
            .iter()
            .map(|entry| GatewaySeriesSnapshot {
                topic: entry.key().clone(),
                latency: entry.value().snapshot(),
            })
            .collect();

        MetricsSnapshot {
            ticks_processed: self.ticks_processed.load(Ordering::Relaxed),
            klines_processed: self.klines_processed.load(Ordering::Relaxed),
            ticks_dropped: (*self.ticks_dropped).load(Ordering::Relaxed),
            klines_dropped: (*self.klines_dropped).load(Ordering::Relaxed),
            broadcast_lagged_tick: self.broadcast_lagged_tick.load(Ordering::Relaxed),
            broadcast_lagged_kline: self.broadcast_lagged_kline.load(Ordering::Relaxed),
            broadcast_lagged_quote: self.broadcast_lagged_quote.load(Ordering::Relaxed),
            quotes_processed: self.quotes_processed.load(Ordering::Relaxed),
            quotes_dropped: (*self.quotes_dropped).load(Ordering::Relaxed),
            ingestion_series,
            ingestion_e2e_series,
            ws_active_clients: self.ws_active_clients.load(Ordering::Relaxed),
            ws_subscriptions_active: self.ws_subscriptions_active.load(Ordering::Relaxed),
            ws_kicked_lagged_total: self.ws_kicked_lagged_total.load(Ordering::Relaxed),
            ws_kicked_write_timeout_total: self
                .ws_kicked_write_timeout_total
                .load(Ordering::Relaxed),
            ws_messages_sent_tick: self.ws_messages_sent_tick.load(Ordering::Relaxed),
            ws_messages_sent_kline: self.ws_messages_sent_kline.load(Ordering::Relaxed),
            ws_messages_sent_quote: self.ws_messages_sent_quote.load(Ordering::Relaxed),
            gateway_series,
        }
    }

    /// 记录 Tick 入库延迟（毫秒）-- 维度 (exchange, tick, symbol)
    ///
    /// - `arrival_ms` = recv − E：到达延迟（我们这条链路）
    /// - `e2e_ms`     = recv − T：端到端延迟（含交易所内部聚合，OKX 与 arrival 相等）
    pub fn record_tick_ingestion(
        &self,
        exchange: &str,
        symbol: &str,
        arrival_ms: i64,
        e2e_ms: i64,
    ) {
        let key = SeriesKey {
            exchange: exchange.to_string(),
            kind: "tick",
            symbol: symbol.to_string(),
            interval: String::new(),
        };
        self.ingestion_hist(key.clone())
            .observe(arrival_ms.max(0) as u64);
        self.ingestion_e2e_hist(key).observe(e2e_ms.max(0) as u64);
    }

    /// 记录 Kline 入库延迟（毫秒）-- 维度 (exchange, kline, symbol, interval)
    pub fn record_kline_ingestion(
        &self,
        exchange: &str,
        symbol: &str,
        interval: &str,
        latency_ms: i64,
    ) {
        let clamped = latency_ms.max(0) as u64;
        let key = SeriesKey {
            exchange: exchange.to_string(),
            kind: "kline",
            symbol: symbol.to_string(),
            interval: interval.to_string(),
        };
        self.ingestion_hist(key).observe(clamped);
    }

    /// 记录 Quote 入库延迟（毫秒）-- 维度 (exchange, quote, symbol)
    ///
    /// 到达延迟 = connector_receive_ts − 交易所事件时间（币安 E / OKX ts）。
    pub fn record_quote_ingestion(&self, exchange: &str, symbol: &str, latency_ms: i64) {
        let key = SeriesKey {
            exchange: exchange.to_string(),
            kind: "quote",
            symbol: symbol.to_string(),
            interval: String::new(),
        };
        self.ingestion_hist(key).observe(latency_ms.max(0) as u64);
    }

    /// 记录 broadcast lagged
    pub fn record_broadcast_lagged(&self, kind: &str) {
        match kind {
            "tick" => {
                self.broadcast_lagged_tick.fetch_add(1, Ordering::Relaxed);
            }
            "kline" => {
                self.broadcast_lagged_kline.fetch_add(1, Ordering::Relaxed);
            }
            "quote" => {
                self.broadcast_lagged_quote.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// WS 连接进入
    pub fn ws_client_connected(&self) {
        self.ws_active_clients.fetch_add(1, Ordering::Relaxed);
    }

    /// WS 连接离开（包括正常关闭 + 踢出）
    pub fn ws_client_disconnected(&self) {
        // 防止下溢（如果只调一次 disconnected 没对应 connected）
        let _ = self
            .ws_active_clients
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                if v == 0 {
                    None
                } else {
                    Some(v - 1)
                }
            });
    }

    /// 因 broadcast 持续 lagged 主动踢出客户端
    pub fn ws_client_kicked_lagged(&self) {
        self.ws_kicked_lagged_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 因写超时（客户端停读、发送缓冲塞满）主动踢出客户端
    pub fn ws_client_kicked_write_timeout(&self) {
        self.ws_kicked_write_timeout_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 网关成功推送一条消息
    pub fn ws_message_sent(&self, kind: &str) {
        match kind {
            "tick" => {
                self.ws_messages_sent_tick.fetch_add(1, Ordering::Relaxed);
            }
            "kline" => {
                self.ws_messages_sent_kline.fetch_add(1, Ordering::Relaxed);
            }
            "quote" => {
                self.ws_messages_sent_quote.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// 记录网关内部转发延迟（毫秒），按 topic 维度
    pub fn record_gateway_forward_latency_ms(&self, topic: &str, latency_ms: u64) {
        self.gateway_hist(topic).observe(latency_ms);
    }
}

/// 单条入库延迟序列的快照（exchange/type/symbol/interval）
#[derive(Debug, Clone)]
pub struct SeriesSnapshot {
    pub exchange: String,
    pub kind: &'static str,
    pub symbol: String,
    pub interval: String,
    pub latency: HistogramSnapshot,
}

/// 单个 topic 的网关转发延迟快照
#[derive(Debug, Clone)]
pub struct GatewaySeriesSnapshot {
    pub topic: String,
    pub latency: HistogramSnapshot,
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub ticks_processed: u64,
    pub klines_processed: u64,
    pub ticks_dropped: u64,
    pub klines_dropped: u64,
    pub broadcast_lagged_tick: u64,
    pub broadcast_lagged_kline: u64,
    pub broadcast_lagged_quote: u64,
    pub quotes_processed: u64,
    pub quotes_dropped: u64,
    /// 到达延迟序列（recv−E）
    pub ingestion_series: Vec<SeriesSnapshot>,
    /// 端到端延迟序列（recv−T，仅 tick）
    pub ingestion_e2e_series: Vec<SeriesSnapshot>,
    pub ws_active_clients: u64,
    pub ws_subscriptions_active: u64,
    pub ws_kicked_lagged_total: u64,
    pub ws_kicked_write_timeout_total: u64,
    pub ws_messages_sent_tick: u64,
    pub ws_messages_sent_kline: u64,
    pub ws_messages_sent_quote: u64,
    pub gateway_series: Vec<GatewaySeriesSnapshot>,
}

/// 单个 WS 连接对 `md_ws_subscriptions_active` 的份额守卫（RAII）。
///
/// 连接在每次订阅集合变化后调用 [`WsSubscriptionGuard::sync`] 报告自己当前持有的接收端数；
/// 无论连接以何种方式结束（正常关闭 / 被踢 / 任务被取消 / panic），`Drop` 都会把份额扣回，
/// 因此全局 gauge 不会因异常路径而漂移。
pub struct WsSubscriptionGuard {
    metrics: Arc<ProcessorMetrics>,
    held: u64,
}

impl WsSubscriptionGuard {
    pub fn new(metrics: Arc<ProcessorMetrics>) -> Self {
        Self { metrics, held: 0 }
    }

    /// 报告本连接当前持有的接收端总数（绝对值），内部按差值更新全局 gauge
    pub fn sync(&mut self, now: usize) {
        let now = now as u64;
        if now > self.held {
            self.metrics
                .ws_subscriptions_active
                .fetch_add(now - self.held, Ordering::Relaxed);
        } else if now < self.held {
            let delta = self.held - now;
            // 饱和减法，防止异常调用造成下溢
            let _ = self.metrics.ws_subscriptions_active.fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |v| Some(v.saturating_sub(delta)),
            );
        }
        self.held = now;
    }

    /// 本连接当前持有的接收端数
    pub fn held(&self) -> u64 {
        self.held
    }
}

impl Drop for WsSubscriptionGuard {
    fn drop(&mut self) {
        self.sync(0);
    }
}

// ---- Subscriber ----

/// PubSub 订阅者 -- 对标 Go 版 processor.Subscriber
#[derive(Debug)]
pub struct Subscriber {
    pub id: String,
    pub rx: broadcast::Receiver<BroadcastEvent>,
}

/// 广播事件
///
/// 第二个字段是事件 publish 时间（单调时钟 Instant），用于：
/// - 网关在 send 给客户端时计算 publish→send 耗时（监控网关内部延迟）
/// - 这是 Copy 类型，跟随 BroadcastEvent::Clone 不带来额外分配
#[derive(Debug, Clone)]
pub enum BroadcastEvent {
    Tick(Arc<Tick>, Instant),
    Kline(Arc<Kline>, Instant),
    /// 买一卖一报价（1.1.0 新增，独立 quote.* 主题）
    Quote(Arc<Quote>, Instant),
}

impl BroadcastEvent {
    /// emit 时刻（用于计算转发延迟）
    pub fn emit_instant(&self) -> Instant {
        match self {
            BroadcastEvent::Tick(_, ts) => *ts,
            BroadcastEvent::Kline(_, ts) => *ts,
            BroadcastEvent::Quote(_, ts) => *ts,
        }
    }

    /// topic kind 字符串："tick"、"kline" 或 "quote"
    pub fn kind(&self) -> &'static str {
        match self {
            BroadcastEvent::Tick(..) => "tick",
            BroadcastEvent::Kline(..) => "kline",
            BroadcastEvent::Quote(..) => "quote",
        }
    }
}

// ---- Processor ----

/// quote 入口 channel 默认容量（与 md-config 的 `processor.quote_channel_buffer` 默认值一致）
pub const DEFAULT_QUOTE_BUFFER: usize = 1000;

/// 同一 quote 主题两次发布之间的最小间隔：100ms，即 ≤ 10 次/秒（任务书 §二 节流要求）
pub const QUOTE_MIN_PUBLISH_INTERVAL: Duration = Duration::from_millis(100);

/// 挂起报价的冲刷检查周期。仅当存在挂起报价时才会被轮询（见 `run`），
/// 无 quote 流量（含开关关闭）时不产生任何定时唤醒。
const QUOTE_FLUSH_TICK: Duration = Duration::from_millis(20);

/// 单个 quote 主题的节流状态
#[derive(Debug, Default)]
struct QuoteThrottleState {
    /// 上次实际发布时刻
    last_publish: Option<Instant>,
    /// 节流窗口内收到、尚未发布的最新报价（只保留最新一条，合并中间值）
    pending: Option<Arc<Quote>>,
}

/// 数据处理器 -- 对标 Go 版 processor.Processor
///
/// 职责：
/// 1. 从 connector 接收 Tick/Kline/Quote（通过 mpsc channel）
/// 2. 缓存最新数据（DashMap）
/// 3. 发布到 PubSub（broadcast channel）
/// 4. 记录 metrics
pub struct Processor {
    tick_tx: mpsc::Sender<Tick>,
    kline_tx: mpsc::Sender<Kline>,
    /// quote 独立入口 channel（1.1.0 新增，容量由 `processor.quote_channel_buffer` 决定）
    quote_tx: mpsc::Sender<Quote>,
    tick_rx: Mutex<mpsc::Receiver<Tick>>,
    kline_rx: Mutex<mpsc::Receiver<Kline>>,
    quote_rx: Mutex<mpsc::Receiver<Quote>>,

    /// 最新 Tick 缓存 -- key: "exchange:SYMBOL"
    latest_ticks: DashMap<String, Arc<Tick>>,
    /// 最新 Kline 缓存 -- key: "exchange:SYMBOL:interval"
    latest_klines: DashMap<String, Arc<Kline>>,
    /// 最新 Quote 缓存 -- key: "EXCHANGE:SYMBOL"（缓存始终是最新值，不受发布节流影响）
    latest_quotes: DashMap<String, Arc<Quote>>,
    /// quote 发布节流状态 -- key: topic
    quote_throttle: DashMap<String, QuoteThrottleState>,
    /// 当前挂起（已收到但因节流尚未发布）的 quote 主题数
    quote_pending: AtomicUsize,

    /// PubSub 广播器 -- key: topic
    broadcasters: DashMap<String, broadcast::Sender<BroadcastEvent>>,

    pub metrics: Arc<ProcessorMetrics>,
    shutdown_tx: broadcast::Sender<()>,
    /// broadcast channel 容量
    broadcast_capacity: usize,
}

use tokio::sync::Mutex;

impl Processor {
    pub fn new(tick_buffer: usize, kline_buffer: usize) -> Self {
        Self::new_with_broadcast_capacity(tick_buffer, kline_buffer, 4096)
    }

    pub fn new_with_broadcast_capacity(
        tick_buffer: usize,
        kline_buffer: usize,
        broadcast_capacity: usize,
    ) -> Self {
        Self::new_full(
            tick_buffer,
            kline_buffer,
            broadcast_capacity,
            DEFAULT_QUOTE_BUFFER,
        )
    }

    /// 完整构造：额外指定 quote 入口 channel 容量（`processor.quote_channel_buffer`）
    pub fn new_full(
        tick_buffer: usize,
        kline_buffer: usize,
        broadcast_capacity: usize,
        quote_buffer: usize,
    ) -> Self {
        let (tick_tx, tick_rx) = mpsc::channel(tick_buffer);
        let (kline_tx, kline_rx) = mpsc::channel(kline_buffer);
        let (quote_tx, quote_rx) = mpsc::channel(quote_buffer);
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            tick_tx,
            kline_tx,
            quote_tx,
            tick_rx: Mutex::new(tick_rx),
            kline_rx: Mutex::new(kline_rx),
            quote_rx: Mutex::new(quote_rx),
            latest_ticks: DashMap::new(),
            latest_klines: DashMap::new(),
            latest_quotes: DashMap::new(),
            quote_throttle: DashMap::new(),
            quote_pending: AtomicUsize::new(0),
            broadcasters: DashMap::new(),
            metrics: Arc::new(ProcessorMetrics::default()),
            shutdown_tx,
            broadcast_capacity,
        }
    }

    /// 获取 Tick 发送端（供 connector 使用）
    pub fn tick_tx(&self) -> mpsc::Sender<Tick> {
        self.tick_tx.clone()
    }

    /// 获取 Kline 发送端（供 connector 使用）
    pub fn kline_tx(&self) -> mpsc::Sender<Kline> {
        self.kline_tx.clone()
    }

    /// 获取 Quote 发送端（供 quote 专用 connector 使用）
    pub fn quote_tx(&self) -> mpsc::Sender<Quote> {
        self.quote_tx.clone()
    }

    /// 订阅指定 topic 的数据
    /// topic 格式: "tick.{exchange}.{SYMBOL}"、"kline.{interval}.{exchange}.{SYMBOL}"
    /// 或 "quote.{exchange}.{SYMBOL}"
    pub fn subscribe(&self, topic: &str) -> Subscriber {
        let capacity = self.broadcast_capacity;
        let tx = self
            .broadcasters
            .entry(topic.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(capacity);
                tx
            })
            .clone();

        let id = Uuid::new_v4().to_string();
        let rx = tx.subscribe();
        Subscriber { id, rx }
    }

    /// 获取最新 Tick
    pub fn get_latest_tick(&self, exchange: &str, symbol: &str) -> Option<Arc<Tick>> {
        let key = topic::tick_cache_key(exchange, symbol);
        self.latest_ticks.get(&key).map(|v| v.clone())
    }

    /// 获取最新 Quote
    pub fn get_latest_quote(&self, exchange: &str, symbol: &str) -> Option<Arc<Quote>> {
        let key = topic::quote_cache_key(exchange, symbol);
        self.latest_quotes.get(&key).map(|v| v.clone())
    }

    /// 获取最新 Kline
    pub fn get_latest_kline(
        &self,
        exchange: &str,
        symbol: &str,
        interval: &str,
    ) -> Option<Arc<Kline>> {
        let key = topic::kline_cache_key(exchange, symbol, interval);
        self.latest_klines.get(&key).map(|v| v.clone())
    }

    /// 启动 dispatch loop
    pub async fn run(&self) {
        let mut tick_rx = self.tick_rx.lock().await;
        let mut kline_rx = self.kline_rx.lock().await;
        let mut quote_rx = self.quote_rx.lock().await;
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        // 挂起 quote 的冲刷定时器：仅当 quote_pending > 0 时才被 select 轮询，
        // 因此开关关闭 / 无 quote 流量时不会产生任何定时唤醒。
        let mut quote_flush_timer = tokio::time::interval(QUOTE_FLUSH_TICK);
        quote_flush_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        info!("processor dispatch loop started");

        loop {
            tokio::select! {
                tick = tick_rx.recv() => {
                    match tick {
                        Some(tick) => self.handle_tick(tick),
                        None => {
                            warn!("tick channel closed");
                            break;
                        }
                    }
                }
                kline = kline_rx.recv() => {
                    match kline {
                        Some(kline) => self.handle_kline(kline),
                        None => {
                            warn!("kline channel closed");
                            break;
                        }
                    }
                }
                quote = quote_rx.recv() => {
                    match quote {
                        Some(quote) => self.handle_quote(quote),
                        None => {
                            warn!("quote channel closed");
                            break;
                        }
                    }
                }
                _ = quote_flush_timer.tick(), if self.quote_pending.load(Ordering::Relaxed) > 0 => {
                    self.flush_pending_quotes_at(Instant::now());
                }
                _ = shutdown_rx.recv() => {
                    info!("processor shutdown signal received");
                    break;
                }
            }
        }

        info!("processor dispatch loop stopped");
    }

    /// 关闭处理器
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    pub fn handle_tick(&self, tick: Tick) {
        self.metrics.ticks_processed.fetch_add(1, Ordering::Relaxed);

        // 记录入库延迟（saturating_sub 防 NTP 回拨导致负值），维度 (exchange, tick, symbol)
        // 时间戳缺失时记 0，保证 histogram _count 仍反映真实吞吐（对标 Go 用 _count 算吞吐）
        // 到达延迟 = recv − E（事件推送时间），即"交易所推送 → 本机接收"的真实链路延迟
        let arrival_ms = if tick.connector_receive_ts > 0 && tick.exchange_event_ts > 0 {
            tick.connector_receive_ts
                .saturating_sub(tick.exchange_event_ts)
        } else {
            0
        };
        // 端到端延迟 = recv − T（成交时间）。币安 T 早于 E，差值即币安内部聚合延迟；
        // 缺 T 时回退为到达延迟（OKX 的 T 即 ts，与 E 相等）
        let e2e_ms = if tick.connector_receive_ts > 0 && tick.exchange_trade_ts > 0 {
            tick.connector_receive_ts
                .saturating_sub(tick.exchange_trade_ts)
        } else {
            arrival_ms
        };
        self.metrics
            .record_tick_ingestion(&tick.exchange, &tick.symbol, arrival_ms, e2e_ms);

        // 缓存（使用统一的 cache key 归一化）
        let key = topic::tick_cache_key(&tick.exchange, &tick.symbol);
        let tick = Arc::new(tick);
        self.latest_ticks.insert(key, tick.clone());

        // 发布
        let topic = topic::tick_topic(&tick.exchange, &tick.symbol);
        self.publish(&topic, BroadcastEvent::Tick(tick, Instant::now()));
    }

    pub fn handle_kline(&self, kline: Kline) {
        self.metrics
            .klines_processed
            .fetch_add(1, Ordering::Relaxed);

        // 记录入库延迟，维度 (exchange, kline, symbol, interval)
        let latency_ms = if kline.connector_receive_ts > 0 && kline.exchange_event_ts > 0 {
            kline
                .connector_receive_ts
                .saturating_sub(kline.exchange_event_ts)
        } else {
            0
        };
        self.metrics.record_kline_ingestion(
            &kline.exchange,
            &kline.symbol,
            &kline.interval,
            latency_ms,
        );

        // 缓存（使用统一的 cache key 归一化）
        let key = topic::kline_cache_key(&kline.exchange, &kline.symbol, &kline.interval);
        let kline = Arc::new(kline);
        self.latest_klines.insert(key, kline.clone());

        // 发布
        let topic = topic::kline_topic(&kline.exchange, &kline.symbol, &kline.interval);
        self.publish(&topic, BroadcastEvent::Kline(kline, Instant::now()));
    }

    /// 处理一条 Quote：记录入库延迟 → 更新缓存（始终最新）→ 按主题节流后发布。
    pub fn handle_quote(&self, quote: Quote) {
        self.handle_quote_at(quote, Instant::now());
    }

    /// `handle_quote` 的可注入时钟版本（便于确定性测试节流）。
    ///
    /// 节流语义（每个 quote 主题独立）：
    /// - 距上次发布 ≥ `QUOTE_MIN_PUBLISH_INTERVAL`（或从未发布）→ 立即发布（leading edge）；
    /// - 否则只保留最新一条为 pending，待窗口结束由 `flush_pending_quotes_at` 发布（trailing edge），
    ///   保证"最后一次变动"一定会送达，不会因节流而让订阅者长期停留在旧报价上。
    pub fn handle_quote_at(&self, quote: Quote, now: Instant) {
        self.metrics
            .quotes_processed
            .fetch_add(1, Ordering::Relaxed);

        // 入库延迟 = recv − 交易所事件时间；时间戳缺失记 0，保证 _count 仍反映真实吞吐
        let latency_ms = if quote.connector_receive_ts > 0 && quote.timestamp > 0 {
            quote.connector_receive_ts.saturating_sub(quote.timestamp)
        } else {
            0
        };
        self.metrics
            .record_quote_ingestion(&quote.exchange, &quote.symbol, latency_ms);

        // 缓存（始终保存最新值，REST latest 不受节流影响）
        let key = topic::quote_cache_key(&quote.exchange, &quote.symbol);
        let quote = Arc::new(quote);
        self.latest_quotes.insert(key, quote.clone());

        let topic = topic::quote_topic(&quote.exchange, &quote.symbol);

        // 节流判定（持有 DashMap 条目锁的范围尽量小，发布在锁外进行）
        let publish_now = {
            let mut st = self.quote_throttle.entry(topic.clone()).or_default();
            let due = st
                .last_publish
                .is_none_or(|t| now.saturating_duration_since(t) >= QUOTE_MIN_PUBLISH_INTERVAL);
            if due {
                st.last_publish = Some(now);
                if st.pending.take().is_some() {
                    self.quote_pending.fetch_sub(1, Ordering::Relaxed);
                }
                Some(quote)
            } else {
                if st.pending.replace(quote).is_none() {
                    self.quote_pending.fetch_add(1, Ordering::Relaxed);
                }
                None
            }
        };

        if let Some(q) = publish_now {
            self.publish(&topic, BroadcastEvent::Quote(q, Instant::now()));
        }
    }

    /// 冲刷已到期的挂起 quote（trailing edge）。由 `run` 的定时器驱动；测试可直接调用。
    pub fn flush_pending_quotes_at(&self, now: Instant) {
        if self.quote_pending.load(Ordering::Relaxed) == 0 {
            return;
        }
        let mut due: Vec<(String, Arc<Quote>)> = Vec::new();
        for mut entry in self.quote_throttle.iter_mut() {
            let topic = entry.key().clone();
            let st = entry.value_mut();
            if st.pending.is_none() {
                continue;
            }
            let ready = st
                .last_publish
                .is_none_or(|t| now.saturating_duration_since(t) >= QUOTE_MIN_PUBLISH_INTERVAL);
            if ready {
                if let Some(q) = st.pending.take() {
                    st.last_publish = Some(now);
                    self.quote_pending.fetch_sub(1, Ordering::Relaxed);
                    due.push((topic, q));
                }
            }
        }
        for (topic, q) in due {
            self.publish(&topic, BroadcastEvent::Quote(q, Instant::now()));
        }
    }

    fn publish(&self, topic: &str, event: BroadcastEvent) {
        if let Some(tx) = self.broadcasters.get(topic) {
            if tx.send(event).is_err() {
                // 没有接收者了，清理 broadcaster（BUG-8 修复）
                drop(tx); // 释放 DashMap Ref，避免死锁
                self.broadcasters.remove(topic);
            }
        }
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_tick(exchange: &str, symbol: &str, price: &str) -> Tick {
        Tick {
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            price: price.to_string(),
            timestamp: 1711929600000,
            quantity: "1.0".to_string(),
            trade_id: 12345,
            is_buyer_maker: true,
            ..Default::default()
        }
    }

    fn make_kline(exchange: &str, symbol: &str, interval: &str, close: &str) -> Kline {
        Kline {
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            interval: interval.to_string(),
            close: close.to_string(),
            open: "100.0".to_string(),
            high: "110.0".to_string(),
            low: "90.0".to_string(),
            volume: "500.0".to_string(),
            open_time: 1711929600000,
            close_time: 1711929659999,
            number_of_trades: 100,
            is_closed: true,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn cache_latest_tick() {
        let proc = Processor::new(100, 100);
        let tick = make_tick("binance", "BTCUSDT", "67000.50");

        // 手动调用 handle_tick 模拟数据到达
        proc.handle_tick(tick);

        let cached = proc.get_latest_tick("binance", "BTCUSDT").unwrap();
        assert_eq!(cached.exchange, "binance");
        assert_eq!(cached.symbol, "BTCUSDT");
        assert_eq!(cached.price, "67000.50");
    }

    #[tokio::test]
    async fn cache_latest_tick_overwrites() {
        let proc = Processor::new(100, 100);

        proc.handle_tick(make_tick("binance", "BTCUSDT", "67000.00"));
        proc.handle_tick(make_tick("binance", "BTCUSDT", "68000.00"));

        let cached = proc.get_latest_tick("binance", "BTCUSDT").unwrap();
        assert_eq!(cached.price, "68000.00");
    }

    #[tokio::test]
    async fn cache_latest_tick_case_insensitive() {
        let proc = Processor::new(100, 100);
        proc.handle_tick(make_tick("binance", "btcusdt", "67000.00"));

        // 查询时大小写不敏感
        assert!(proc.get_latest_tick("binance", "BTCUSDT").is_some());
        assert!(proc.get_latest_tick("binance", "btcusdt").is_some());
    }

    #[tokio::test]
    async fn cache_latest_kline() {
        let proc = Processor::new(100, 100);
        let kline = make_kline("binance", "BTCUSDT", "1m", "67050.00");

        proc.handle_kline(kline);

        let cached = proc.get_latest_kline("binance", "BTCUSDT", "1m").unwrap();
        assert_eq!(cached.exchange, "binance");
        assert_eq!(cached.symbol, "BTCUSDT");
        assert_eq!(cached.interval, "1m");
        assert_eq!(cached.close, "67050.00");
    }

    #[tokio::test]
    async fn cache_returns_none_for_missing() {
        let proc = Processor::new(100, 100);
        assert!(proc.get_latest_tick("binance", "NONEXIST").is_none());
        assert!(proc.get_latest_kline("binance", "NONEXIST", "1m").is_none());
    }

    #[tokio::test]
    async fn subscribe_receives_tick() {
        let proc = Processor::new(100, 100);
        let mut sub = proc.subscribe("tick.binance.BTCUSDT");

        proc.handle_tick(make_tick("binance", "BTCUSDT", "67000.50"));

        let event = tokio::time::timeout(Duration::from_secs(1), sub.rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");

        match event {
            BroadcastEvent::Tick(t, _) => {
                assert_eq!(t.exchange, "binance");
                assert_eq!(t.symbol, "BTCUSDT");
                assert_eq!(t.price, "67000.50");
            }
            _ => panic!("expected Tick"),
        }
    }

    #[tokio::test]
    async fn subscribe_receives_kline() {
        let proc = Processor::new(100, 100);
        let mut sub = proc.subscribe("kline.1m.binance.BTCUSDT");

        proc.handle_kline(make_kline("binance", "BTCUSDT", "1m", "67050.00"));

        let event = tokio::time::timeout(Duration::from_secs(1), sub.rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");

        match event {
            BroadcastEvent::Kline(k, _) => {
                assert_eq!(k.exchange, "binance");
                assert_eq!(k.symbol, "BTCUSDT");
                assert_eq!(k.interval, "1m");
                assert_eq!(k.close, "67050.00");
            }
            _ => panic!("expected Kline"),
        }
    }

    #[tokio::test]
    async fn subscribe_does_not_receive_wrong_topic() {
        let proc = Processor::new(100, 100);
        let mut sub = proc.subscribe("tick.binance.ETHUSDT");

        // 发送到 BTCUSDT topic
        proc.handle_tick(make_tick("binance", "BTCUSDT", "67000.00"));

        // ETHUSDT 订阅者不应收到
        let result = tokio::time::timeout(Duration::from_millis(100), sub.rx.recv()).await;
        assert!(result.is_err()); // timeout = 没收到
    }

    #[tokio::test]
    async fn metrics_tick_count() {
        let proc = Processor::new(100, 100);

        proc.handle_tick(make_tick("binance", "BTCUSDT", "67000.00"));
        proc.handle_tick(make_tick("binance", "ETHUSDT", "3500.00"));

        let snap = proc.metrics.snapshot();
        assert_eq!(snap.ticks_processed, 2);
        assert_eq!(snap.klines_processed, 0);
    }

    #[tokio::test]
    async fn metrics_kline_count() {
        let proc = Processor::new(100, 100);

        proc.handle_kline(make_kline("binance", "BTCUSDT", "1m", "67000.00"));
        proc.handle_kline(make_kline("binance", "ETHUSDT", "5m", "3500.00"));

        let snap = proc.metrics.snapshot();
        assert_eq!(snap.ticks_processed, 0);
        assert_eq!(snap.klines_processed, 2);
    }

    #[tokio::test]
    async fn multiple_subscribers_same_topic() {
        let proc = Processor::new(100, 100);
        let mut sub1 = proc.subscribe("tick.binance.BTCUSDT");
        let mut sub2 = proc.subscribe("tick.binance.BTCUSDT");

        proc.handle_tick(make_tick("binance", "BTCUSDT", "67000.00"));

        let evt1 = tokio::time::timeout(Duration::from_secs(1), sub1.rx.recv())
            .await
            .unwrap()
            .unwrap();
        let evt2 = tokio::time::timeout(Duration::from_secs(1), sub2.rx.recv())
            .await
            .unwrap()
            .unwrap();

        match (evt1, evt2) {
            (BroadcastEvent::Tick(t1, _), BroadcastEvent::Tick(t2, _)) => {
                assert_eq!(t1.price, "67000.00");
                assert_eq!(t2.price, "67000.00");
            }
            _ => panic!("expected Tick for both"),
        }
    }

    #[tokio::test]
    async fn tick_kline_channel_senders_work() {
        let proc = Processor::new(100, 100);
        let tick_tx = proc.tick_tx();
        let kline_tx = proc.kline_tx();

        // channel 可以正常发送
        tick_tx
            .send(make_tick("binance", "BTCUSDT", "67000.00"))
            .await
            .unwrap();
        kline_tx
            .send(make_kline("binance", "BTCUSDT", "1m", "67050.00"))
            .await
            .unwrap();
    }

    #[test]
    fn metrics_ingestion_latency_records() {
        let hist = Histogram::new(BUCKETS_MS);

        // 记录几个延迟值（取值与具体桶边界无关，测试对桶下标变化健壮）
        let values = [3u64, 50, 200, 8000];
        for v in values {
            hist.observe(v);
        }

        let snap = hist.snapshot();
        assert_eq!(snap.count, values.len() as u64);
        assert_eq!(snap.sum, values.iter().sum::<u64>());

        // 每个桶只计"恰好落入该桶"的值（非累积）：根据边界推导期望下标
        let bucket_index = |v: u64| -> usize {
            BUCKETS_MS
                .iter()
                .position(|&b| v <= b)
                .unwrap_or(BUCKETS_MS.len()) // 超出所有边界 -> +Inf 桶
        };
        let mut expected = vec![0u64; BUCKETS_MS.len() + 1];
        for v in values {
            expected[bucket_index(v)] += 1;
        }
        for (i, exp) in expected.iter().enumerate() {
            assert_eq!(snap.buckets[i], *exp, "bucket[{}] 计数不符", i);
        }
        // 8000 超出所有边界，必在 +Inf 桶
        assert_eq!(snap.buckets[BUCKETS_MS.len()], 1);
    }

    #[test]
    fn metrics_record_ingestion_latency_clamps_negative() {
        let metrics = ProcessorMetrics::default();
        // 模拟 NTP 回拨导致负延迟（到达延迟与端到端延迟均为负）
        metrics.record_tick_ingestion("binance", "BTCUSDT", -100, -100);
        let snap = metrics.snapshot();
        let series = snap
            .ingestion_series
            .iter()
            .find(|s| s.exchange == "binance" && s.kind == "tick" && s.symbol == "BTCUSDT")
            .expect("series should exist");
        // 负值被 clamp 到 0，落入 le=1 桶
        assert_eq!(series.latency.count, 1);
        assert_eq!(series.latency.sum, 0);
        // 端到端延迟序列同样记录并 clamp 到 0
        let e2e = snap
            .ingestion_e2e_series
            .iter()
            .find(|s| s.exchange == "binance" && s.kind == "tick" && s.symbol == "BTCUSDT")
            .expect("e2e series should exist");
        assert_eq!(e2e.latency.count, 1);
        assert_eq!(e2e.latency.sum, 0);
    }

    #[test]
    fn metrics_per_symbol_dimension_separated() {
        // 验证按标的分维度：BTCUSDT 与 ETHUSDT 各自独立统计
        let metrics = ProcessorMetrics::default();
        // 模拟币安：到达延迟(recv-E) 与端到端(recv-T) 不同
        metrics.record_tick_ingestion("binance", "BTCUSDT", 10, 40);
        metrics.record_tick_ingestion("binance", "BTCUSDT", 20, 50);
        metrics.record_tick_ingestion("binance", "ETHUSDT", 30, 60);
        metrics.record_kline_ingestion("okx", "BTC-USDT", "1m", 100);

        let snap = metrics.snapshot();
        let btc = snap
            .ingestion_series
            .iter()
            .find(|s| s.symbol == "BTCUSDT" && s.kind == "tick")
            .unwrap();
        let eth = snap
            .ingestion_series
            .iter()
            .find(|s| s.symbol == "ETHUSDT" && s.kind == "tick")
            .unwrap();
        let kline = snap
            .ingestion_series
            .iter()
            .find(|s| s.symbol == "BTC-USDT" && s.kind == "kline")
            .unwrap();

        assert_eq!(btc.latency.count, 2);
        assert_eq!(btc.latency.sum, 30); // 到达延迟 10+20
        assert_eq!(eth.latency.count, 1);
        assert_eq!(eth.latency.sum, 30);
        assert_eq!(kline.interval, "1m");
        assert_eq!(kline.latency.count, 1);

        // 端到端延迟（recv-T）独立成列，且数值不同（含交易所内部延迟）
        let btc_e2e = snap
            .ingestion_e2e_series
            .iter()
            .find(|s| s.symbol == "BTCUSDT" && s.kind == "tick")
            .unwrap();
        assert_eq!(btc_e2e.latency.count, 2);
        assert_eq!(btc_e2e.latency.sum, 90); // 端到端 40+50，明显高于到达延迟 30
    }

    #[test]
    fn metrics_gateway_latency_by_topic() {
        let metrics = ProcessorMetrics::default();
        metrics.record_gateway_forward_latency_ms("tick.binance.BTCUSDT", 2);
        metrics.record_gateway_forward_latency_ms("tick.binance.BTCUSDT", 4);
        metrics.record_gateway_forward_latency_ms("kline.1m.okx.BTC-USDT", 1);

        let snap = metrics.snapshot();
        let t = snap
            .gateway_series
            .iter()
            .find(|s| s.topic == "tick.binance.BTCUSDT")
            .unwrap();
        assert_eq!(t.latency.count, 2);
        assert_eq!(t.latency.sum, 6);
        assert_eq!(snap.gateway_series.len(), 2);
    }

    // ---- quote（1.1.0 新增）----

    fn make_quote(exchange: &str, symbol: &str, bid: &str) -> Quote {
        Quote {
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            bid_price: bid.to_string(),
            bid_qty: "1".to_string(),
            ask_price: "999".to_string(),
            ask_qty: "2".to_string(),
            last_price: String::new(),
            timestamp: 1711929600000,
            connector_receive_ts: 1711929600030,
        }
    }

    /// 收取订阅者当前已就绪的全部 quote 事件（非阻塞）
    fn drain_quotes(sub: &mut Subscriber) -> Vec<Arc<Quote>> {
        let mut out = Vec::new();
        while let Ok(ev) = sub.rx.try_recv() {
            match ev {
                BroadcastEvent::Quote(q, _) => out.push(q),
                other => panic!("unexpected event on quote topic: {:?}", other),
            }
        }
        out
    }

    #[tokio::test]
    async fn cache_latest_quote_and_case_insensitive() {
        let proc = Processor::new(100, 100);
        proc.handle_quote(make_quote("binance", "bnbusdt", "743.90"));

        let cached = proc.get_latest_quote("binance", "BNBUSDT").unwrap();
        assert_eq!(cached.bid_price, "743.90");
        assert!(proc.get_latest_quote("binance", "bnbusdt").is_some());
        assert!(proc.get_latest_quote("binance", "NONEXIST").is_none());
    }

    #[tokio::test]
    async fn quote_does_not_touch_tick_or_kline_state() {
        let proc = Processor::new(100, 100);
        proc.handle_quote(make_quote("binance", "BNBUSDT", "1"));

        assert!(proc.get_latest_tick("binance", "BNBUSDT").is_none());
        let snap = proc.metrics.snapshot();
        assert_eq!(snap.ticks_processed, 0);
        assert_eq!(snap.klines_processed, 0);
        assert_eq!(snap.quotes_processed, 1);
        // 入库延迟只多出 type=quote 序列
        assert!(snap.ingestion_series.iter().all(|s| s.kind == "quote"));
        assert!(snap.ingestion_e2e_series.is_empty());
    }

    #[tokio::test]
    async fn subscribe_receives_quote_on_quote_topic_only() {
        let proc = Processor::new(100, 100);
        let mut quote_sub = proc.subscribe("quote.binance.BNBUSDT");
        let mut tick_sub = proc.subscribe("tick.binance.BNBUSDT");

        proc.handle_quote(make_quote("binance", "BNBUSDT", "743.90"));

        let ev = tokio::time::timeout(Duration::from_secs(1), quote_sub.rx.recv())
            .await
            .expect("timeout")
            .expect("closed");
        match &ev {
            BroadcastEvent::Quote(q, _) => assert_eq!(q.bid_price, "743.90"),
            other => panic!("expected Quote, got {:?}", other),
        }
        assert_eq!(ev.kind(), "quote");
        // tick 主题订阅者收不到 quote
        assert!(tick_sub.rx.try_recv().is_err());
    }

    /// 节流：同一主题 10ms 一条、共 30 条（覆盖 300ms）→ 发布数 ≤ 4，
    /// 任意相邻两次发布间隔 ≥ 100ms，且最后一条（trailing）必达。
    #[tokio::test]
    async fn quote_throttle_caps_rate_and_delivers_last() {
        let proc = Processor::new(100, 100);
        let mut sub = proc.subscribe("quote.binance.BNBUSDT");
        let base = Instant::now();

        let mut published_at: Vec<u64> = Vec::new();
        for i in 0..30u64 {
            let t = base + Duration::from_millis(i * 10);
            proc.handle_quote_at(make_quote("binance", "BNBUSDT", &format!("p{}", i)), t);
            proc.flush_pending_quotes_at(t);
            for _ in drain_quotes(&mut sub) {
                published_at.push(i * 10);
            }
        }
        // 窗口再走过 100ms 以冲刷最后一条
        let t_end = base + Duration::from_millis(29 * 10 + 100);
        proc.flush_pending_quotes_at(t_end);
        let tail = drain_quotes(&mut sub);
        assert_eq!(tail.len(), 1, "trailing 报价必须送达");
        assert_eq!(tail[0].bid_price, "p29", "trailing 必须是最后一条");
        published_at.push(29 * 10 + 100);

        assert!(
            published_at.len() <= 5,
            "300ms 内发布 {} 次，超过 10/s",
            published_at.len()
        );
        for w in published_at.windows(2) {
            assert!(w[1] - w[0] >= 100, "相邻发布间隔 {}ms < 100ms", w[1] - w[0]);
        }
        // 缓存始终是最新值
        assert_eq!(
            proc.get_latest_quote("binance", "BNBUSDT")
                .unwrap()
                .bid_price,
            "p29"
        );
    }

    /// 1 秒内每 10ms 一条（100 条/秒）→ 发布数必须 ≤ 10 次/秒（+1 为首条 leading 的边界），
    /// 且不应过度丢弃（≥ 9）。把 `QUOTE_MIN_PUBLISH_INTERVAL` 改小/置 0 时本测试必须失败
    /// （变异验证见交付报告）。
    #[tokio::test]
    async fn quote_throttle_one_second_burst_capped() {
        let cap = 1000 / QUOTE_MIN_PUBLISH_INTERVAL.as_millis() as usize;
        let proc = Processor::new(100, 100);
        let mut sub = proc.subscribe("quote.okx.BNB-USDT-SWAP");
        let base = Instant::now();
        let mut n = 0usize;
        for i in 0..100u64 {
            let t = base + Duration::from_millis(i * 10);
            proc.handle_quote_at(make_quote("okx", "BNB-USDT-SWAP", &format!("p{}", i)), t);
            proc.flush_pending_quotes_at(t);
            n += drain_quotes(&mut sub).len();
        }
        assert!(n <= cap + 1, "1 秒内发布 {} 次，超过 {} 次/秒上限", n, cap);
        assert!(n >= cap - 1, "节流不应过度丢弃: {}", n);
    }

    /// 不同主题互不节流
    #[tokio::test]
    async fn quote_throttle_is_per_topic() {
        let proc = Processor::new(100, 100);
        let mut a = proc.subscribe("quote.binance.BNBUSDT");
        let mut b = proc.subscribe("quote.okx.BNB-USDT-SWAP");
        let t = Instant::now();
        proc.handle_quote_at(make_quote("binance", "BNBUSDT", "a1"), t);
        proc.handle_quote_at(make_quote("okx", "BNB-USDT-SWAP", "b1"), t);
        assert_eq!(drain_quotes(&mut a).len(), 1);
        assert_eq!(drain_quotes(&mut b).len(), 1);
    }

    /// 经由 mpsc + run() 的端到端：第一条立即到，第二条（节流窗口内）由定时器冲刷在 ~100ms 内到达
    #[tokio::test]
    async fn quote_flows_through_run_loop_with_trailing_flush() {
        let proc = Arc::new(Processor::new_full(100, 100, 256, 100));
        let mut sub = proc.subscribe("quote.binance.BNBUSDT");
        let p = proc.clone();
        let handle = tokio::spawn(async move { p.run().await });

        let tx = proc.quote_tx();
        tx.send(make_quote("binance", "BNBUSDT", "first"))
            .await
            .unwrap();
        tx.send(make_quote("binance", "BNBUSDT", "second"))
            .await
            .unwrap();

        let first = tokio::time::timeout(Duration::from_millis(500), sub.rx.recv())
            .await
            .expect("first timeout")
            .unwrap();
        let t_first = Instant::now();
        match first {
            BroadcastEvent::Quote(q, _) => assert_eq!(q.bid_price, "first"),
            other => panic!("unexpected {:?}", other),
        }
        let second = tokio::time::timeout(Duration::from_millis(800), sub.rx.recv())
            .await
            .expect("second (trailing) timeout")
            .unwrap();
        match second {
            BroadcastEvent::Quote(q, _) => assert_eq!(q.bid_price, "second"),
            other => panic!("unexpected {:?}", other),
        }
        assert!(
            t_first.elapsed() >= Duration::from_millis(60),
            "trailing 不应早于节流窗口"
        );

        proc.shutdown();
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[test]
    fn metrics_quote_dimensions() {
        let metrics = ProcessorMetrics::default();
        metrics.record_quote_ingestion("okx", "BNB-USDT-SWAP", 25);
        metrics.ws_message_sent("quote");
        metrics.record_broadcast_lagged("quote");
        let snap = metrics.snapshot();
        let s = snap
            .ingestion_series
            .iter()
            .find(|s| s.kind == "quote" && s.symbol == "BNB-USDT-SWAP")
            .expect("quote series");
        assert_eq!(s.interval, "");
        assert_eq!(s.latency.count, 1);
        assert_eq!(s.latency.sum, 25);
        assert_eq!(snap.ws_messages_sent_quote, 1);
        assert_eq!(snap.broadcast_lagged_quote, 1);
        // 不污染 tick/kline 计数
        assert_eq!(snap.ws_messages_sent_tick, 0);
        assert_eq!(snap.broadcast_lagged_tick, 0);
    }
}

#[cfg(test)]
mod ws_subscription_guard_tests {
    use super::*;

    fn gauge(m: &ProcessorMetrics) -> u64 {
        m.ws_subscriptions_active.load(Ordering::Relaxed)
    }

    #[test]
    fn guard_tracks_delta_and_releases_on_drop() {
        let m = Arc::new(ProcessorMetrics::default());
        let mut a = WsSubscriptionGuard::new(m.clone());
        let mut b = WsSubscriptionGuard::new(m.clone());
        a.sync(3);
        b.sync(2);
        assert_eq!(gauge(&m), 5);
        a.sync(1); // 退订 2 个
        assert_eq!(gauge(&m), 3);
        a.sync(1); // 重复 sync 幂等
        assert_eq!(gauge(&m), 3);
        drop(b); // 连接断开：份额扣回
        assert_eq!(gauge(&m), 1);
        drop(a);
        assert_eq!(gauge(&m), 0);
    }

    #[test]
    fn guard_releases_even_if_task_panics() {
        let m = Arc::new(ProcessorMetrics::default());
        let m2 = m.clone();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let mut g = WsSubscriptionGuard::new(m2);
            g.sync(4);
            panic!("boom");
        }));
        assert!(r.is_err());
        assert_eq!(gauge(&m), 0, "panic 展开时 Drop 仍应归还份额");
    }
}
