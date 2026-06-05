use dashmap::DashMap;
use md_domain::topic;
use md_domain::types::{Kline, Tick};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};
use uuid::Uuid;

// ---- Metrics ----

/// 入库延迟直方图桶边界（毫秒）-- 覆盖 Tick 链路常见区间（1ms~5s）
pub const BUCKETS_MS: &[u64] = &[1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000];

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
            buckets: self.buckets.iter().map(|b| b.load(Ordering::Relaxed)).collect(),
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
    /// "tick" 或 "kline"
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

    // ---- 入库延迟/吞吐：按 (exchange, type, symbol, interval) 维度 ----
    /// 对标 Go 版 marketdata_processor_ingestion_latency_ms，dashboard 由此聚合出交易所/标的/周期视图
    pub ingestion_latency: DashMap<SeriesKey, Arc<Histogram>>,

    // ---- 网关 / WebSocket 指标 ----
    /// 当前活跃 WebSocket 连接数（gateway + legacy 合计）
    pub ws_active_clients: AtomicU64,
    /// 因 broadcast lagged 累计达到阈值被踢出的客户端总数
    pub ws_kicked_lagged_total: AtomicU64,
    /// 网关推送总条数（每条 WS 消息 +1，按 topic_kind 区分）
    pub ws_messages_sent_tick: AtomicU64,
    pub ws_messages_sent_kline: AtomicU64,
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
            ingestion_latency: DashMap::new(),
            ws_active_clients: AtomicU64::new(0),
            ws_kicked_lagged_total: AtomicU64::new(0),
            ws_messages_sent_tick: AtomicU64::new(0),
            ws_messages_sent_kline: AtomicU64::new(0),
            gateway_forward_latency: DashMap::new(),
        }
    }
}

impl ProcessorMetrics {
    /// 获取（或惰性创建）某 series 的入库延迟 histogram
    fn ingestion_hist(&self, key: SeriesKey) -> Arc<Histogram> {
        if let Some(h) = self.ingestion_latency.get(&key) {
            return h.clone();
        }
        self.ingestion_latency
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
            ingestion_series,
            ws_active_clients: self.ws_active_clients.load(Ordering::Relaxed),
            ws_kicked_lagged_total: self.ws_kicked_lagged_total.load(Ordering::Relaxed),
            ws_messages_sent_tick: self.ws_messages_sent_tick.load(Ordering::Relaxed),
            ws_messages_sent_kline: self.ws_messages_sent_kline.load(Ordering::Relaxed),
            gateway_series,
        }
    }

    /// 记录 Tick 入库延迟（毫秒）-- 维度 (exchange, tick, symbol)
    pub fn record_tick_ingestion(&self, exchange: &str, symbol: &str, latency_ms: i64) {
        let clamped = latency_ms.max(0) as u64;
        let key = SeriesKey {
            exchange: exchange.to_string(),
            kind: "tick",
            symbol: symbol.to_string(),
            interval: String::new(),
        };
        self.ingestion_hist(key).observe(clamped);
    }

    /// 记录 Kline 入库延迟（毫秒）-- 维度 (exchange, kline, symbol, interval)
    pub fn record_kline_ingestion(&self, exchange: &str, symbol: &str, interval: &str, latency_ms: i64) {
        let clamped = latency_ms.max(0) as u64;
        let key = SeriesKey {
            exchange: exchange.to_string(),
            kind: "kline",
            symbol: symbol.to_string(),
            interval: interval.to_string(),
        };
        self.ingestion_hist(key).observe(clamped);
    }

    /// 记录 broadcast lagged
    pub fn record_broadcast_lagged(&self, kind: &str) {
        match kind {
            "tick" => { self.broadcast_lagged_tick.fetch_add(1, Ordering::Relaxed); }
            "kline" => { self.broadcast_lagged_kline.fetch_add(1, Ordering::Relaxed); }
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
        let _ = self.ws_active_clients.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
            if v == 0 { None } else { Some(v - 1) }
        });
    }

    /// 因 broadcast 持续 lagged 主动踢出客户端
    pub fn ws_client_kicked_lagged(&self) {
        self.ws_kicked_lagged_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 网关成功推送一条消息
    pub fn ws_message_sent(&self, kind: &str) {
        match kind {
            "tick" => { self.ws_messages_sent_tick.fetch_add(1, Ordering::Relaxed); }
            "kline" => { self.ws_messages_sent_kline.fetch_add(1, Ordering::Relaxed); }
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
    pub ingestion_series: Vec<SeriesSnapshot>,
    pub ws_active_clients: u64,
    pub ws_kicked_lagged_total: u64,
    pub ws_messages_sent_tick: u64,
    pub ws_messages_sent_kline: u64,
    pub gateway_series: Vec<GatewaySeriesSnapshot>,
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
}

impl BroadcastEvent {
    /// emit 时刻（用于计算转发延迟）
    pub fn emit_instant(&self) -> Instant {
        match self {
            BroadcastEvent::Tick(_, ts) => *ts,
            BroadcastEvent::Kline(_, ts) => *ts,
        }
    }

    /// topic kind 字符串："tick" 或 "kline"
    pub fn kind(&self) -> &'static str {
        match self {
            BroadcastEvent::Tick(..) => "tick",
            BroadcastEvent::Kline(..) => "kline",
        }
    }
}

// ---- Processor ----

/// 数据处理器 -- 对标 Go 版 processor.Processor
///
/// 职责：
/// 1. 从 connector 接收 Tick/Kline（通过 mpsc channel）
/// 2. 缓存最新数据（DashMap）
/// 3. 发布到 PubSub（broadcast channel）
/// 4. 记录 metrics
pub struct Processor {
    tick_tx: mpsc::Sender<Tick>,
    kline_tx: mpsc::Sender<Kline>,
    tick_rx: Mutex<mpsc::Receiver<Tick>>,
    kline_rx: Mutex<mpsc::Receiver<Kline>>,

    /// 最新 Tick 缓存 -- key: "exchange:SYMBOL"
    latest_ticks: DashMap<String, Arc<Tick>>,
    /// 最新 Kline 缓存 -- key: "exchange:SYMBOL:interval"
    latest_klines: DashMap<String, Arc<Kline>>,

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

    pub fn new_with_broadcast_capacity(tick_buffer: usize, kline_buffer: usize, broadcast_capacity: usize) -> Self {
        let (tick_tx, tick_rx) = mpsc::channel(tick_buffer);
        let (kline_tx, kline_rx) = mpsc::channel(kline_buffer);
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            tick_tx,
            kline_tx,
            tick_rx: Mutex::new(tick_rx),
            kline_rx: Mutex::new(kline_rx),
            latest_ticks: DashMap::new(),
            latest_klines: DashMap::new(),
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

    /// 订阅指定 topic 的数据
    /// topic 格式: "tick.{exchange}.{SYMBOL}" 或 "kline.{interval}.{exchange}.{SYMBOL}"
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
        let mut shutdown_rx = self.shutdown_tx.subscribe();

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
        let latency_ms = if tick.connector_receive_ts > 0 && tick.exchange_event_ts > 0 {
            tick.connector_receive_ts.saturating_sub(tick.exchange_event_ts)
        } else {
            0
        };
        self.metrics
            .record_tick_ingestion(&tick.exchange, &tick.symbol, latency_ms);

        // 缓存（使用统一的 cache key 归一化）
        let key = topic::tick_cache_key(&tick.exchange, &tick.symbol);
        let tick = Arc::new(tick);
        self.latest_ticks.insert(key, tick.clone());

        // 发布
        let topic = topic::tick_topic(&tick.exchange, &tick.symbol);
        self.publish(&topic, BroadcastEvent::Tick(tick, Instant::now()));
    }

    pub fn handle_kline(&self, kline: Kline) {
        self.metrics.klines_processed.fetch_add(1, Ordering::Relaxed);

        // 记录入库延迟，维度 (exchange, kline, symbol, interval)
        let latency_ms = if kline.connector_receive_ts > 0 && kline.exchange_event_ts > 0 {
            kline.connector_receive_ts.saturating_sub(kline.exchange_event_ts)
        } else {
            0
        };
        self.metrics
            .record_kline_ingestion(&kline.exchange, &kline.symbol, &kline.interval, latency_ms);

        // 缓存（使用统一的 cache key 归一化）
        let key = topic::kline_cache_key(&kline.exchange, &kline.symbol, &kline.interval);
        let kline = Arc::new(kline);
        self.latest_klines.insert(key, kline.clone());

        // 发布
        let topic = topic::kline_topic(&kline.exchange, &kline.symbol, &kline.interval);
        self.publish(&topic, BroadcastEvent::Kline(kline, Instant::now()));
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
        assert!(proc
            .get_latest_kline("binance", "NONEXIST", "1m")
            .is_none());
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

        // 记录几个延迟值
        hist.observe(3);    // 落入 le=5 桶
        hist.observe(50);   // 落入 le=50 桶
        hist.observe(200);  // 落入 le=250 桶
        hist.observe(8000); // 超出所有桶，落入 +Inf

        let snap = hist.snapshot();
        assert_eq!(snap.count, 4);
        assert_eq!(snap.sum, 3 + 50 + 200 + 8000);

        // 每个桶只计落入该桶的值（非累积）
        assert_eq!(snap.buckets[0], 0); // le=1: 无
        assert_eq!(snap.buckets[1], 1); // le=5: 3
        assert_eq!(snap.buckets[2], 0); // le=10: 无
        assert_eq!(snap.buckets[3], 0); // le=25: 无
        assert_eq!(snap.buckets[4], 1); // le=50: 50
        assert_eq!(snap.buckets[5], 0); // le=100: 无
        assert_eq!(snap.buckets[6], 1); // le=250: 200
        // +Inf 桶
        assert_eq!(snap.buckets[BUCKETS_MS.len()], 1); // 8000
    }

    #[test]
    fn metrics_record_ingestion_latency_clamps_negative() {
        let metrics = ProcessorMetrics::default();
        // 模拟 NTP 回拨导致负延迟
        metrics.record_tick_ingestion("binance", "BTCUSDT", -100);
        let snap = metrics.snapshot();
        let series = snap
            .ingestion_series
            .iter()
            .find(|s| s.exchange == "binance" && s.kind == "tick" && s.symbol == "BTCUSDT")
            .expect("series should exist");
        // 负值被 clamp 到 0，落入 le=1 桶
        assert_eq!(series.latency.count, 1);
        assert_eq!(series.latency.sum, 0);
    }

    #[test]
    fn metrics_per_symbol_dimension_separated() {
        // 验证按标的分维度：BTCUSDT 与 ETHUSDT 各自独立统计
        let metrics = ProcessorMetrics::default();
        metrics.record_tick_ingestion("binance", "BTCUSDT", 10);
        metrics.record_tick_ingestion("binance", "BTCUSDT", 20);
        metrics.record_tick_ingestion("binance", "ETHUSDT", 30);
        metrics.record_kline_ingestion("okx", "BTC-USDT", "1m", 100);

        let snap = metrics.snapshot();
        let btc = snap.ingestion_series.iter()
            .find(|s| s.symbol == "BTCUSDT" && s.kind == "tick").unwrap();
        let eth = snap.ingestion_series.iter()
            .find(|s| s.symbol == "ETHUSDT" && s.kind == "tick").unwrap();
        let kline = snap.ingestion_series.iter()
            .find(|s| s.symbol == "BTC-USDT" && s.kind == "kline").unwrap();

        assert_eq!(btc.latency.count, 2);
        assert_eq!(btc.latency.sum, 30);
        assert_eq!(eth.latency.count, 1);
        assert_eq!(eth.latency.sum, 30);
        assert_eq!(kline.interval, "1m");
        assert_eq!(kline.latency.count, 1);
    }

    #[test]
    fn metrics_gateway_latency_by_topic() {
        let metrics = ProcessorMetrics::default();
        metrics.record_gateway_forward_latency_ms("tick.binance.BTCUSDT", 2);
        metrics.record_gateway_forward_latency_ms("tick.binance.BTCUSDT", 4);
        metrics.record_gateway_forward_latency_ms("kline.1m.okx.BTC-USDT", 1);

        let snap = metrics.snapshot();
        let t = snap.gateway_series.iter()
            .find(|s| s.topic == "tick.binance.BTCUSDT").unwrap();
        assert_eq!(t.latency.count, 2);
        assert_eq!(t.latency.sum, 6);
        assert_eq!(snap.gateway_series.len(), 2);
    }
}
