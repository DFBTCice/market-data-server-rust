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

/// 单个交易所 + 数据类型的入库指标
///
/// 对标 Go 版按 (exchange, kind) 维度分组的吞吐和延迟监控：
/// - 数据采集吞吐量（按交易所 + 类型）
/// - Tick / Kline 采集延迟 P50/P99（按交易所）
#[derive(Debug)]
pub struct ExchangeMetrics {
    /// Tick 入库总数
    pub ticks_processed: AtomicU64,
    /// Kline 入库总数
    pub klines_processed: AtomicU64,
    /// Tick 入库延迟（exchange_event_ts → connector_receive_ts，毫秒）
    pub ingestion_latency_tick: Histogram,
    /// Kline 入库延迟（毫秒）
    pub ingestion_latency_kline: Histogram,
}

impl ExchangeMetrics {
    pub fn new() -> Self {
        Self {
            ticks_processed: AtomicU64::new(0),
            klines_processed: AtomicU64::new(0),
            ingestion_latency_tick: Histogram::new(BUCKETS_MS),
            ingestion_latency_kline: Histogram::new(BUCKETS_MS),
        }
    }
}

impl Default for ExchangeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局 Processor 指标（无锁原子计数器 + per-exchange map）
#[derive(Debug)]
pub struct ProcessorMetrics {
    // ---- 全局聚合（兼容老 dashboard） ----
    pub ticks_processed: AtomicU64,
    pub klines_processed: AtomicU64,
    pub ticks_dropped: Arc<AtomicU64>,
    pub klines_dropped: Arc<AtomicU64>,
    /// 全局入库延迟 histogram（与 per-exchange 同步记录，方便单查询总览）
    pub ingestion_latency: Histogram,
    /// broadcast lagged 计数 (tick)
    pub broadcast_lagged_tick: AtomicU64,
    /// broadcast lagged 计数 (kline)
    pub broadcast_lagged_kline: AtomicU64,

    // ---- 按交易所分维度（对应 Go dashboard 的 binance/okx 分图） ----
    /// key: "binance" / "okx" / "okx-kline" 等
    pub per_exchange: DashMap<String, Arc<ExchangeMetrics>>,

    // ---- 网关 / WebSocket 指标 ----
    /// 当前活跃 WebSocket 连接数（gateway + legacy 合计）
    pub ws_active_clients: AtomicU64,
    /// 因 broadcast lagged 累计达到阈值被踢出的客户端总数
    pub ws_kicked_lagged_total: AtomicU64,
    /// 网关推送总条数（每条 WS 消息 +1，按 topic_kind 区分）
    pub ws_messages_sent_tick: AtomicU64,
    pub ws_messages_sent_kline: AtomicU64,
    /// 网关内部转发延迟（处理器 publish → 客户端 send 完成，毫秒）
    pub gateway_forward_latency: Histogram,
}

impl Default for ProcessorMetrics {
    fn default() -> Self {
        Self {
            ticks_processed: AtomicU64::new(0),
            klines_processed: AtomicU64::new(0),
            ticks_dropped: Arc::new(AtomicU64::new(0)),
            klines_dropped: Arc::new(AtomicU64::new(0)),
            ingestion_latency: Histogram::new(BUCKETS_MS),
            broadcast_lagged_tick: AtomicU64::new(0),
            broadcast_lagged_kline: AtomicU64::new(0),
            per_exchange: DashMap::new(),
            ws_active_clients: AtomicU64::new(0),
            ws_kicked_lagged_total: AtomicU64::new(0),
            ws_messages_sent_tick: AtomicU64::new(0),
            ws_messages_sent_kline: AtomicU64::new(0),
            gateway_forward_latency: Histogram::new(GATEWAY_LATENCY_BUCKETS_MS),
        }
    }
}

impl ProcessorMetrics {
    /// 获取（或惰性创建）某交易所的指标
    pub fn exchange(&self, name: &str) -> Arc<ExchangeMetrics> {
        if let Some(m) = self.per_exchange.get(name) {
            return m.clone();
        }
        self.per_exchange
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(ExchangeMetrics::new()))
            .clone()
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let per_exchange: Vec<ExchangeMetricsSnapshot> = self
            .per_exchange
            .iter()
            .map(|entry| ExchangeMetricsSnapshot {
                exchange: entry.key().clone(),
                ticks_processed: entry.value().ticks_processed.load(Ordering::Relaxed),
                klines_processed: entry.value().klines_processed.load(Ordering::Relaxed),
                ingestion_latency_tick: entry.value().ingestion_latency_tick.snapshot(),
                ingestion_latency_kline: entry.value().ingestion_latency_kline.snapshot(),
            })
            .collect();

        MetricsSnapshot {
            ticks_processed: self.ticks_processed.load(Ordering::Relaxed),
            klines_processed: self.klines_processed.load(Ordering::Relaxed),
            ticks_dropped: (*self.ticks_dropped).load(Ordering::Relaxed),
            klines_dropped: (*self.klines_dropped).load(Ordering::Relaxed),
            ingestion_latency: self.ingestion_latency.snapshot(),
            broadcast_lagged_tick: self.broadcast_lagged_tick.load(Ordering::Relaxed),
            broadcast_lagged_kline: self.broadcast_lagged_kline.load(Ordering::Relaxed),
            per_exchange,
            ws_active_clients: self.ws_active_clients.load(Ordering::Relaxed),
            ws_kicked_lagged_total: self.ws_kicked_lagged_total.load(Ordering::Relaxed),
            ws_messages_sent_tick: self.ws_messages_sent_tick.load(Ordering::Relaxed),
            ws_messages_sent_kline: self.ws_messages_sent_kline.load(Ordering::Relaxed),
            gateway_forward_latency: self.gateway_forward_latency.snapshot(),
        }
    }

    /// 记录某交易所的 Tick 入库延迟（毫秒），并同步全局
    pub fn record_tick_ingestion(&self, exchange: &str, latency_ms: i64) {
        let clamped = latency_ms.max(0) as u64;
        self.ingestion_latency.observe(clamped);
        let m = self.exchange(exchange);
        m.ticks_processed.fetch_add(1, Ordering::Relaxed);
        m.ingestion_latency_tick.observe(clamped);
    }

    /// 记录某交易所的 Kline 入库延迟（毫秒），并同步全局
    pub fn record_kline_ingestion(&self, exchange: &str, latency_ms: i64) {
        let clamped = latency_ms.max(0) as u64;
        self.ingestion_latency.observe(clamped);
        let m = self.exchange(exchange);
        m.klines_processed.fetch_add(1, Ordering::Relaxed);
        m.ingestion_latency_kline.observe(clamped);
    }

    /// 记录入库延迟（毫秒），防负值 -- 仅全局直方图
    /// 兼容旧调用点；新代码请使用 record_tick_ingestion / record_kline_ingestion
    pub fn record_ingestion_latency(&self, latency_ms: i64) {
        let clamped = latency_ms.max(0) as u64;
        self.ingestion_latency.observe(clamped);
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

    /// 记录网关内部转发延迟（毫秒）
    pub fn record_gateway_forward_latency_ms(&self, latency_ms: u64) {
        self.gateway_forward_latency.observe(latency_ms);
    }
}

#[derive(Debug, Clone)]
pub struct ExchangeMetricsSnapshot {
    pub exchange: String,
    pub ticks_processed: u64,
    pub klines_processed: u64,
    pub ingestion_latency_tick: HistogramSnapshot,
    pub ingestion_latency_kline: HistogramSnapshot,
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub ticks_processed: u64,
    pub klines_processed: u64,
    pub ticks_dropped: u64,
    pub klines_dropped: u64,
    pub ingestion_latency: HistogramSnapshot,
    pub broadcast_lagged_tick: u64,
    pub broadcast_lagged_kline: u64,
    pub per_exchange: Vec<ExchangeMetricsSnapshot>,
    pub ws_active_clients: u64,
    pub ws_kicked_lagged_total: u64,
    pub ws_messages_sent_tick: u64,
    pub ws_messages_sent_kline: u64,
    pub gateway_forward_latency: HistogramSnapshot,
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

        // 记录入库延迟（saturating_sub 防 NTP 回拨导致负值），按交易所分维度
        if tick.connector_receive_ts > 0 && tick.exchange_event_ts > 0 {
            let latency_ms = tick
                .connector_receive_ts
                .saturating_sub(tick.exchange_event_ts);
            self.metrics
                .record_tick_ingestion(&tick.exchange, latency_ms);
        } else {
            // 即使时间戳缺失也要计入交易所吞吐
            let m = self.metrics.exchange(&tick.exchange);
            m.ticks_processed.fetch_add(1, Ordering::Relaxed);
        }

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

        // 记录入库延迟（按交易所）
        if kline.connector_receive_ts > 0 && kline.exchange_event_ts > 0 {
            let latency_ms = kline
                .connector_receive_ts
                .saturating_sub(kline.exchange_event_ts);
            self.metrics
                .record_kline_ingestion(&kline.exchange, latency_ms);
        } else {
            let m = self.metrics.exchange(&kline.exchange);
            m.klines_processed.fetch_add(1, Ordering::Relaxed);
        }

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
        metrics.record_ingestion_latency(-100);
        let snap = metrics.ingestion_latency.snapshot();
        // 负值被 clamp 到 0，落入 le=1 桶
        assert_eq!(snap.count, 1);
        assert_eq!(snap.sum, 0);
    }
}
