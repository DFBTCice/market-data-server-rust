//! 运行期订阅的闲置退订与按需补订（1.2.0）。
//!
//! - **运行期订阅**：连接器 `desired_subs` 里不在配置名单中的目标（`POST /api/v1/subscriptions`、gRPC Admin 登记的，
//!   以及本模块按需补订的）。配置名单（`subscribe_ticks` / `subscribe_klines` / `subscribe_quotes`）里的目标永不自动退订；
//!   同一目标同时在配置和运行期时按配置处理。比较时交易所小写、币对大写（与发布端 topic 的规范写法一致）。
//! - **有人在接收**：目标对应的规范 topic 上有接收端（Gateway WS、Legacy WS、gRPC 流都经 `Processor::subscribe`），
//!   或最近 `idle` 内有 REST latest 查询过它，或最近 `idle` 内有新接收端登记过它。
//! - **闲置退订**：每 `check_interval` 检查一次；连续 `idle` 没有接收方才退上游并从 `desired_subs` 移除。
//!   闲置起点取"首次观察到没有接收端"的检查时刻，所以实际闲置时长只会 ≥ `idle`、不会提前。
//!   开关打开（进程启动或 SIGHUP 打开）后 `startup_grace` 内不退订。
//! - **按需补订**：某 topic 有接收端、上游却没有对应订阅时，按运行期订阅补订（新接收端登记后约 200ms 内，每次检查再兜底一次）。
//!   只接受已配置的交易所和类型（与 POST 同一路由规则），QUOTE 还须 `on_demand_quote` 打开；上游明确拒绝的目标缓存失败并指数退避；
//!   运行期订阅数达到上限后不再补订；本模块向每个连接器发出的订阅变更消息经令牌桶限速，避免挤占同一连接上的配置订阅。
//!
//! 开关关闭时本模块不读也不改任何订阅，只丢弃收到的通知，行为与 1.1.1 相同。

use md_connector::{find_connector, Connector, DataType, SubscriptionRejected, SubscriptionTarget};
use md_domain::topic::{self, Topic};
use md_processor::Processor;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, watch, Notify};

// 固定日志 target：现网 `RUST_LOG=warn` 时可用 `RUST_LOG=warn,md_runtime_subs=info` 单独打开本模块的 info 日志。
macro_rules! info {
    ($($arg:tt)+) => { tracing::info!(target: "md_runtime_subs", $($arg)+) };
}
macro_rules! warn {
    ($($arg:tt)+) => { tracing::warn!(target: "md_runtime_subs", $($arg)+) };
}
macro_rules! debug {
    ($($arg:tt)+) => { tracing::debug!(target: "md_runtime_subs", $($arg)+) };
}

type ConnectorMap = HashMap<String, Arc<dyn Connector>>;
pub type SharedConnectors = Arc<tokio::sync::RwLock<ConnectorMap>>;
/// 一次检查里按连接器分组的按需补订：连接器 → (连接器, [(目标, 当前接收端数)])
type DemandBatches = HashMap<String, (Arc<dyn Connector>, Vec<(SubKey, usize)>)>;

/// 币安 U 本位合约支持的 K 线级别。币安对不支持的级别、不存在的币对都应答 `{"result":null}`（不报错，
/// 2026-11-06 实测），无法靠上游拒绝识别，所以币安 K 线按需补订只接受这些级别。OKX 会明确拒绝（60018），不需要白名单。
pub const BINANCE_KLINE_INTERVALS: [&str; 15] = [
    "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d", "1w", "1M",
];

/// 被拒绝的记录超过这么久没有再次被拒就忘掉
const REJECT_FORGET_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

pub const SOURCE_CONFIG: &str = "config";
pub const SOURCE_RUNTIME: &str = "runtime";

// ---------------------------------------------------------------------------
// 设置
// ---------------------------------------------------------------------------

/// 运行参数。前五项来自 YAML `runtime_subscriptions`，其余是内部常量（测试可缩短）。
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// 总开关（`idle_unsubscribe`）：闲置退订与按需补订一起开关
    pub enabled: bool,
    pub idle: Duration,
    pub check_interval: Duration,
    pub startup_grace: Duration,
    pub max_runtime_subscriptions: usize,
    /// QUOTE 是否参与按需补订（`on_demand_quote`，默认 false）；不影响 POST 登记的 QUOTE 与闲置退订
    pub on_demand_quote: bool,
    /// 新接收端登记后等这么久再处理，把同一时刻的多个订阅合成一条上游消息
    pub demand_batch_delay: Duration,
    /// 上游拒绝后第一次重试前的等待；之后每次翻倍，封顶 `reject_backoff_max`
    pub reject_backoff_initial: Duration,
    pub reject_backoff_max: Duration,
    /// 每个连接器的订阅变更消息令牌桶：容量与补充间隔（默认 10 条突发、每 15 秒补 1 条，即 ≤ 240 条/小时）
    pub upstream_burst: u32,
    pub upstream_refill: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            idle: Duration::from_secs(10 * 60),
            check_interval: Duration::from_secs(60),
            startup_grace: Duration::from_secs(10 * 60),
            max_runtime_subscriptions: 200,
            on_demand_quote: false,
            demand_batch_delay: Duration::from_millis(200),
            reject_backoff_initial: Duration::from_secs(5 * 60),
            reject_backoff_max: Duration::from_secs(60 * 60),
            upstream_burst: 10,
            upstream_refill: Duration::from_secs(15),
        }
    }
}

/// 设置 + 配置名单（受保护、永不自动退订的目标）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigView {
    pub settings: Settings,
    pub protected: HashSet<SubKey>,
}

// ---------------------------------------------------------------------------
// 订阅键
// ---------------------------------------------------------------------------

/// 订阅目标的规范键：交易所小写、币对大写、K 线级别原样（级别大小写有含义，如币安 `1M` 是月线）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubKey {
    pub exchange: String,
    pub data_type: DataType,
    pub symbol: String,
    pub interval: Option<String>,
}

impl SubKey {
    pub fn new(exchange: &str, data_type: DataType, symbol: &str, interval: Option<&str>) -> Self {
        Self {
            exchange: exchange.to_lowercase(),
            data_type,
            symbol: topic::normalize_symbol(symbol),
            interval: match data_type {
                // 与连接器的 target_to_streams 一致：KLINE 缺级别时按 1m
                DataType::Kline => Some(interval.unwrap_or("1m").to_string()),
                _ => None,
            },
        }
    }

    pub fn of(target: &SubscriptionTarget) -> Self {
        Self::new(
            &target.exchange,
            target.data_type,
            &target.symbol,
            target.kline_interval.as_deref(),
        )
    }

    /// 发布端使用的规范 topic
    pub fn topic(&self) -> String {
        match self.data_type {
            DataType::Tick => topic::tick_topic(&self.exchange, &self.symbol),
            DataType::Kline => topic::kline_topic(
                &self.exchange,
                &self.symbol,
                self.interval.as_deref().unwrap_or("1m"),
            ),
            DataType::Quote => topic::quote_topic(&self.exchange, &self.symbol),
        }
    }

    /// 只认规范写法的 topic（交易所小写、币对大写）。其它写法的 topic 本来就收不到推送（发布端只发规范 topic），
    /// 为它们补订没有意义。
    pub fn from_canonical_topic(raw: &str) -> Option<Self> {
        let parsed = Topic::parse(raw).ok()?;
        if parsed.format() != raw {
            return None;
        }
        let (exchange, data_type, symbol, interval) = match parsed {
            Topic::Tick { exchange, symbol } => (exchange, DataType::Tick, symbol, None),
            Topic::Kline {
                interval,
                exchange,
                symbol,
            } => (exchange, DataType::Kline, symbol, Some(interval)),
            Topic::Quote { exchange, symbol } => (exchange, DataType::Quote, symbol, None),
        };
        if exchange.is_empty()
            || symbol.is_empty()
            || exchange != exchange.to_lowercase()
            || interval.as_deref() == Some("")
        {
            return None;
        }
        Some(Self::new(
            &exchange,
            data_type,
            &symbol,
            interval.as_deref(),
        ))
    }

    pub fn target(&self) -> SubscriptionTarget {
        SubscriptionTarget {
            exchange: self.exchange.clone(),
            data_type: self.data_type,
            symbol: self.symbol.clone(),
            kline_interval: self.interval.clone(),
        }
    }
}

impl fmt::Display for SubKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "exchange={} type={} symbol={} interval={}",
            self.exchange,
            self.data_type,
            self.symbol,
            self.interval.as_deref().unwrap_or("-")
        )
    }
}

fn interval_supported(key: &SubKey) -> bool {
    match (key.data_type, key.exchange.as_str()) {
        (DataType::Kline, "binance") => {
            BINANCE_KLINE_INTERVALS.contains(&key.interval.as_deref().unwrap_or(""))
        }
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// 指标（/metrics 只追加）
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Metrics {
    pub enabled: AtomicU64,
    pub runtime_subscriptions: AtomicU64,
    pub idle_unsubscribed_total: AtomicU64,
    pub demand_subscribed_total: AtomicU64,
    pub demand_failed_total: AtomicU64,
    pub limit_rejected_total: AtomicU64,
}

impl Metrics {
    pub fn render_prometheus(&self, out: &mut String) {
        let rows: [(&str, &str, &str, &AtomicU64); 6] = [
            (
                "md_runtime_subscriptions_enabled",
                "gauge",
                "Runtime subscription idle-unsubscribe and on-demand subscribe switch (runtime_subscriptions.idle_unsubscribe, 0/1)",
                &self.enabled,
            ),
            (
                "md_runtime_subscriptions",
                "gauge",
                "Runtime subscriptions currently held upstream (not in the config lists); refreshed on every check while enabled, 0 while disabled",
                &self.runtime_subscriptions,
            ),
            (
                "md_runtime_subscriptions_idle_unsubscribed_total",
                "counter",
                "Runtime subscriptions unsubscribed upstream after idle_minutes without receivers",
                &self.idle_unsubscribed_total,
            ),
            (
                "md_runtime_subscriptions_demand_subscribed_total",
                "counter",
                "Runtime subscriptions added automatically because a WebSocket/gRPC receiver subscribed a topic missing upstream",
                &self.demand_subscribed_total,
            ),
            (
                "md_runtime_subscriptions_demand_failed_total",
                "counter",
                "On-demand subscriptions rejected by the upstream exchange or failed to send (backed off, not retried immediately)",
                &self.demand_failed_total,
            ),
            (
                "md_runtime_subscriptions_limit_rejected_total",
                "counter",
                "On-demand subscriptions refused because max_runtime_subscriptions was reached",
                &self.limit_rejected_total,
            ),
        ];
        for (name, kind, help, value) in rows {
            out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} {kind}\n"));
            out.push_str(&format!("{name} {}\n", value.load(Ordering::Relaxed)));
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/subscriptions 的追加字段
// ---------------------------------------------------------------------------

/// 开关打开时 GET 订阅列表每条追加的信息
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// `config` / `runtime`
    pub source: &'static str,
    /// 最近一次确认有人在用的时间（Unix 毫秒）；当前有接收端时为当前时间；开关打开以来从未有人用过时为 None
    pub last_active_at: Option<i64>,
}

#[derive(Default)]
struct Shared {
    enabled: AtomicBool,
    protected: RwLock<HashSet<SubKey>>,
    last_active_ms: RwLock<HashMap<SubKey, i64>>,
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn fmt_dur(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{}.{:03}s", s, d.subsec_millis())
    }
}

// ---------------------------------------------------------------------------
// 循环内部状态
// ---------------------------------------------------------------------------

struct Usage {
    /// 本模块第一次看到它在 desired_subs 里的时刻
    registered_at: Instant,
    /// 首次观察到没有接收端的检查时刻；有接收端时为 None
    absent_since: Option<Instant>,
    last_active: Option<Instant>,
}

struct Rejection {
    failures: u32,
    last_failure: Instant,
    retry_at: Instant,
}

struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
}

impl TokenBucket {
    fn refill(&mut self, now: Instant, burst: u32, every: Duration) {
        let every_ms = every.as_millis().max(1);
        let n = now.saturating_duration_since(self.last_refill).as_millis() / every_ms;
        if n == 0 {
            return;
        }
        let n = u32::try_from(n).unwrap_or(u32::MAX);
        self.tokens = self.tokens.saturating_add(n).min(burst);
        self.last_refill = if self.tokens == burst {
            now
        } else {
            self.last_refill + every * n
        };
    }

    fn available(&mut self, now: Instant, s: &Settings) -> bool {
        self.refill(now, s.upstream_burst, s.upstream_refill);
        self.tokens > 0
    }

    fn try_take(&mut self, now: Instant, s: &Settings) -> bool {
        if self.available(now, s) {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }
}

struct LoopState {
    /// 开关打开的时刻；保护期从这里算
    enabled_at: Instant,
    usage: HashMap<SubKey, Usage>,
    last_read: HashMap<SubKey, Instant>,
    last_receiver_event: HashMap<SubKey, Instant>,
    /// 本模块按需补订加进去的目标：被上游拒绝时由本模块撤回（POST 登记的保持原样）
    demand_added: HashSet<SubscriptionTarget>,
    rejected: HashMap<SubKey, Rejection>,
    limit_warned: HashMap<SubKey, Instant>,
    buckets: HashMap<String, TokenBucket>,
}

impl LoopState {
    fn new(now: Instant) -> Self {
        Self {
            enabled_at: now,
            usage: HashMap::new(),
            last_read: HashMap::new(),
            last_receiver_event: HashMap::new(),
            demand_added: HashSet::new(),
            rejected: HashMap::new(),
            limit_warned: HashMap::new(),
            buckets: HashMap::new(),
        }
    }

    fn bucket(&mut self, connector: &str, now: Instant, s: &Settings) -> &mut TokenBucket {
        self.buckets
            .entry(connector.to_string())
            .or_insert(TokenBucket {
                tokens: s.upstream_burst,
                last_refill: now,
            })
    }

    /// 登记一次上游拒绝，返回到下次允许重试的等待时长
    fn record_rejection(&mut self, key: &SubKey, now: Instant, s: &Settings) -> Duration {
        let r = self.rejected.entry(key.clone()).or_insert(Rejection {
            failures: 0,
            last_failure: now,
            retry_at: now,
        });
        r.failures = r.failures.saturating_add(1);
        let shift = (r.failures - 1).min(16);
        let wait = s
            .reject_backoff_initial
            .saturating_mul(1u32 << shift)
            .min(s.reject_backoff_max);
        r.last_failure = now;
        r.retry_at = now + wait;
        wait
    }
}

fn bump(map: &mut HashMap<SubKey, Instant>, key: SubKey, at: Instant) {
    let e = map.entry(key).or_insert(at);
    if at > *e {
        *e = at;
    }
}

/// 与 `find_connector` 同一路由规则，额外返回连接器在映射里的键
fn route(conns: &ConnectorMap, key: &SubKey) -> Option<(String, Arc<dyn Connector>)> {
    let found = find_connector(conns, &key.exchange, key.data_type)?;
    let ptr = Arc::as_ptr(found) as *const ();
    conns
        .iter()
        .find(|(_, c)| std::ptr::eq(Arc::as_ptr(c) as *const (), ptr))
        .map(|(name, c)| (name.clone(), c.clone()))
}

// ---------------------------------------------------------------------------
// 管理器
// ---------------------------------------------------------------------------

pub struct RuntimeSubscriptions {
    processor: Arc<Processor>,
    connectors: SharedConnectors,
    config_tx: watch::Sender<Arc<ConfigView>>,
    reject_rx: Mutex<Option<mpsc::UnboundedReceiver<SubscriptionRejected>>>,
    nudge: Notify,
    metrics: Arc<Metrics>,
    shared: Shared,
}

impl RuntimeSubscriptions {
    /// `reject_rx`：各连接器 `with_reject_sink` 的接收端（上游拒绝订阅的上报）
    pub fn new(
        processor: Arc<Processor>,
        connectors: SharedConnectors,
        initial: ConfigView,
        reject_rx: mpsc::UnboundedReceiver<SubscriptionRejected>,
    ) -> Arc<Self> {
        let (config_tx, _) = watch::channel(Arc::new(initial));
        Arc::new(Self {
            processor,
            connectors,
            config_tx,
            reject_rx: Mutex::new(Some(reject_rx)),
            nudge: Notify::new(),
            metrics: Arc::new(Metrics::default()),
            shared: Shared::default(),
        })
    }

    pub fn metrics(&self) -> Arc<Metrics> {
        self.metrics.clone()
    }

    /// 更新设置与配置名单（启动后由 SIGHUP 重载调用）
    pub fn update_config(&self, view: ConfigView) {
        self.config_tx.send_replace(Arc::new(view));
    }

    /// 请求尽快做一次按需补订检查（如 DELETE 之后，仍有接收方的目标应立即补回）
    pub fn nudge(&self) {
        self.nudge.notify_one();
    }

    /// GET 订阅列表的追加信息；开关关闭时返回 None（响应与 1.1.1 逐字节相同）
    pub fn annotate(&self, target: &SubscriptionTarget) -> Option<Annotation> {
        if !self.shared.enabled.load(Ordering::Relaxed) {
            return None;
        }
        let key = SubKey::of(target);
        let source = if read(&self.shared.protected).contains(&key) {
            SOURCE_CONFIG
        } else {
            SOURCE_RUNTIME
        };
        let last_active_at = if self.processor.receiver_count(&key.topic()) > 0 {
            Some(unix_ms())
        } else {
            read(&self.shared.last_active_ms).get(&key).copied()
        };
        Some(Annotation {
            source,
            last_active_at,
        })
    }

    /// 主循环（进程内只运行一次）。开关关闭时空转：不读不改订阅，只丢弃通知。
    pub async fn run(self: Arc<Self>) {
        let taken = self
            .reject_rx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some(mut reject_rx) = taken else {
            warn!("runtime subscriptions manager is already running; ignoring the second run()");
            return;
        };
        let mut reject_open = true;
        let mut config_rx = self.config_tx.subscribe();
        let mut view = config_rx.borrow_and_update().clone();
        let mut state: Option<LoopState> = None;
        self.transition(&mut state, None, &view);
        let mut ticker = new_ticker(view.settings.check_interval);

        loop {
            tokio::select! {
                changed = config_rx.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let next = config_rx.borrow_and_update().clone();
                    self.transition(&mut state, Some(&view.settings), &next);
                    if next.settings.check_interval != view.settings.check_interval {
                        ticker = new_ticker(next.settings.check_interval);
                    }
                    view = next;
                }
                _ = ticker.tick() => {
                    if let Some(st) = state.as_mut() {
                        self.pass(st, &view, true).await;
                    }
                }
                _ = self.processor.usage_signaled() => {
                    if let Some(st) = state.as_mut() {
                        tokio::time::sleep(view.settings.demand_batch_delay).await;
                        self.pass(st, &view, false).await;
                    }
                }
                _ = self.nudge.notified() => {
                    if let Some(st) = state.as_mut() {
                        tokio::time::sleep(view.settings.demand_batch_delay).await;
                        self.pass(st, &view, false).await;
                    }
                }
                rejected = reject_rx.recv(), if reject_open => match rejected {
                    Some(r) => {
                        if let Some(st) = state.as_mut() {
                            self.on_rejected(st, &view, r).await;
                        }
                    }
                    None => reject_open = false,
                },
            }
        }
    }

    fn transition(&self, state: &mut Option<LoopState>, old: Option<&Settings>, next: &ConfigView) {
        *write(&self.shared.protected) = next.protected.clone();
        let s = &next.settings;
        let was_enabled = old.is_some_and(|o| o.enabled);
        if s.enabled && !was_enabled {
            self.processor.enable_usage_signals();
            *state = Some(LoopState::new(Instant::now()));
            self.shared.enabled.store(true, Ordering::Relaxed);
            self.metrics.enabled.store(1, Ordering::Relaxed);
            info!(
                "runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED ({})",
                describe(s)
            );
            self.nudge.notify_one();
        } else if !s.enabled && was_enabled {
            self.processor.disable_usage_signals();
            *state = None;
            self.shared.enabled.store(false, Ordering::Relaxed);
            write(&self.shared.last_active_ms).clear();
            self.metrics.enabled.store(0, Ordering::Relaxed);
            self.metrics
                .runtime_subscriptions
                .store(0, Ordering::Relaxed);
            info!(
                "runtime subscriptions: idle unsubscribe and on-demand subscribe DISABLED \
                 (current subscriptions are kept as they are)"
            );
        } else if s.enabled && old.is_some_and(|o| o != s) {
            info!("runtime subscriptions: settings changed ({})", describe(s));
            self.nudge.notify_one();
        }
    }

    /// 一次检查：更新使用情况 →（`idle_check` 时）闲置退订 → 按需补订 → 发布状态
    async fn pass(&self, st: &mut LoopState, view: &ConfigView, idle_check: bool) {
        let s = &view.settings;
        let now = Instant::now();

        let signals = self.processor.take_usage_signals();
        for (t, at) in signals.reads {
            if let Some(k) = SubKey::from_canonical_topic(&t) {
                bump(&mut st.last_read, k, at);
            }
        }
        for (t, at) in signals.receivers {
            if let Some(k) = SubKey::from_canonical_topic(&t) {
                bump(&mut st.last_receiver_event, k, at);
            }
        }

        let conns: ConnectorMap = self.connectors.read().await.clone();
        let mut subscribed: HashMap<SubKey, Vec<(String, SubscriptionTarget)>> = HashMap::new();
        for (name, c) in &conns {
            for t in c.subscriptions_snapshot().await {
                subscribed
                    .entry(SubKey::of(&t))
                    .or_default()
                    .push((name.clone(), t));
            }
        }

        // ---- 使用情况 ----
        st.usage.retain(|k, _| subscribed.contains_key(k));
        st.demand_added
            .retain(|t| subscribed.contains_key(&SubKey::of(t)));
        let now_ms = unix_ms();
        let mut last_active_ms = HashMap::new();
        let mut runtime_keys: HashSet<SubKey> = HashSet::new();
        let mut idle_candidates: Vec<(SubKey, Duration)> = Vec::new();
        for k in subscribed.keys() {
            let receivers = self.processor.receiver_count(&k.topic());
            let u = st.usage.entry(k.clone()).or_insert(Usage {
                registered_at: now,
                absent_since: None,
                last_active: None,
            });
            if receivers > 0 {
                u.absent_since = None;
                u.last_active = Some(now);
            } else if u.absent_since.is_none() {
                u.absent_since = Some(now);
            }
            let read_at = st.last_read.get(k).copied();
            let event_at = st.last_receiver_event.get(k).copied();
            for at in [read_at, event_at].into_iter().flatten() {
                if u.last_active.is_none_or(|a| at > a) {
                    u.last_active = Some(at);
                }
            }
            if let Some(a) = u.last_active {
                let ago = now.saturating_duration_since(a).as_millis() as i64;
                last_active_ms.insert(k.clone(), now_ms - ago);
            }
            if view.protected.contains(k) {
                continue;
            }
            runtime_keys.insert(k.clone());
            if let Some(absent) = u.absent_since {
                let idle_start = [Some(absent), Some(u.registered_at), read_at, event_at]
                    .into_iter()
                    .flatten()
                    .max()
                    .unwrap_or(absent);
                let idle_for = now.saturating_duration_since(idle_start);
                if idle_for >= s.idle {
                    idle_candidates.push((k.clone(), idle_for));
                }
            }
        }

        // ---- 闲置退订 ----
        let in_grace = now.saturating_duration_since(st.enabled_at) < s.startup_grace;
        if idle_check && !in_grace && !idle_candidates.is_empty() {
            let mut by_conn: HashMap<String, Vec<(SubKey, Duration, SubscriptionTarget)>> =
                HashMap::new();
            for (k, idle_for) in idle_candidates {
                for (name, t) in &subscribed[&k] {
                    by_conn
                        .entry(name.clone())
                        .or_default()
                        .push((k.clone(), idle_for, t.clone()));
                }
            }
            for (name, items) in by_conn {
                if !st.bucket(&name, now, s).try_take(now, s) {
                    debug!(
                        "idle unsubscribe on connector {} deferred: upstream message rate limit",
                        name
                    );
                    continue;
                }
                let targets: Vec<SubscriptionTarget> = items.iter().map(|i| i.2.clone()).collect();
                match conns[&name].remove_subscriptions(targets).await {
                    Ok(()) => {
                        for (k, idle_for, t) in items {
                            info!(
                                "runtime subscription idle-unsubscribed: {} idle={} connector={}",
                                k,
                                fmt_dur(idle_for),
                                name
                            );
                            self.metrics
                                .idle_unsubscribed_total
                                .fetch_add(1, Ordering::Relaxed);
                            st.usage.remove(&k);
                            st.demand_added.remove(&t);
                            subscribed.remove(&k);
                            runtime_keys.remove(&k);
                        }
                    }
                    Err(e) => warn!(
                        "runtime subscription idle unsubscribe failed on connector {}: {}",
                        name, e
                    ),
                }
            }
        }

        // ---- 按需补订 ----
        let mut runtime_count = runtime_keys.len();
        let mut planned: HashSet<SubKey> = HashSet::new();
        let mut batches: DemandBatches = HashMap::new();
        for raw in self.processor.topics_with_receivers() {
            let Some(k) = SubKey::from_canonical_topic(&raw) else {
                continue;
            };
            if subscribed.contains_key(&k) || !planned.insert(k.clone()) {
                continue;
            }
            if k.data_type == DataType::Quote && !s.on_demand_quote {
                debug!("on-demand subscribe skipped, on_demand_quote=false: {}", k);
                continue;
            }
            let Some((name, conn)) = route(&conns, &k) else {
                debug!(
                    "on-demand subscribe skipped, exchange/type not configured: {}",
                    k
                );
                continue;
            };
            if !interval_supported(&k) {
                debug!(
                    "on-demand subscribe skipped, unsupported kline interval: {}",
                    k
                );
                continue;
            }
            if let Some(r) = st.rejected.get(&k) {
                if now < r.retry_at {
                    debug!(
                        "on-demand subscribe skipped, rejected by upstream before (retry in {}): {}",
                        fmt_dur(r.retry_at - now),
                        k
                    );
                    continue;
                }
            }
            if runtime_count >= s.max_runtime_subscriptions {
                self.metrics
                    .limit_rejected_total
                    .fetch_add(1, Ordering::Relaxed);
                let warned_recently = st
                    .limit_warned
                    .get(&k)
                    .is_some_and(|t| now.saturating_duration_since(*t) < s.check_interval);
                if !warned_recently {
                    st.limit_warned.insert(k.clone(), now);
                    warn!(
                        "runtime subscription on-demand refused: limit reached (runtime={}, max_runtime_subscriptions={}) {}",
                        runtime_count, s.max_runtime_subscriptions, k
                    );
                }
                continue;
            }
            if !st.bucket(&name, now, s).available(now, s) {
                debug!(
                    "on-demand subscribe deferred on connector {}: upstream message rate limit ({})",
                    name, k
                );
                continue;
            }
            runtime_count += 1;
            let receivers = self.processor.receiver_count(&raw);
            batches
                .entry(name)
                .or_insert_with(|| (conn, Vec::new()))
                .1
                .push((k, receivers));
        }
        for (name, (conn, items)) in batches {
            st.bucket(&name, now, s).try_take(now, s);
            let targets: Vec<SubscriptionTarget> = items.iter().map(|(k, _)| k.target()).collect();
            match conn.add_subscriptions(targets).await {
                Ok(()) => {
                    for (k, receivers) in items {
                        info!(
                            "runtime subscription added on demand: {} receivers={} connector={}",
                            k, receivers, name
                        );
                        self.metrics
                            .demand_subscribed_total
                            .fetch_add(1, Ordering::Relaxed);
                        st.demand_added.insert(k.target());
                        st.usage.insert(
                            k.clone(),
                            Usage {
                                registered_at: now,
                                absent_since: None,
                                last_active: Some(now),
                            },
                        );
                        last_active_ms.insert(k, now_ms);
                    }
                }
                Err(e) => {
                    for (k, _) in items {
                        self.metrics
                            .demand_failed_total
                            .fetch_add(1, Ordering::Relaxed);
                        runtime_count -= 1;
                        let wait = st.record_rejection(&k, now, s);
                        warn!(
                            "runtime subscription on-demand subscribe failed on connector {}: {} ({}); no retry for {}",
                            name,
                            e,
                            k,
                            fmt_dur(wait)
                        );
                    }
                }
            }
        }

        // ---- 发布状态、清理过期记录 ----
        self.metrics
            .runtime_subscriptions
            .store(runtime_count as u64, Ordering::Relaxed);
        *write(&self.shared.last_active_ms) = last_active_ms;
        st.last_read
            .retain(|_, t| now.saturating_duration_since(*t) < s.idle);
        st.last_receiver_event
            .retain(|_, t| now.saturating_duration_since(*t) < s.idle);
        st.rejected
            .retain(|_, r| now.saturating_duration_since(r.last_failure) < REJECT_FORGET_AFTER);
        st.limit_warned
            .retain(|_, t| now.saturating_duration_since(*t) < s.check_interval);
    }

    /// 上游明确拒绝某个目标：缓存失败并退避；按需补订加进去的撤回，POST 登记的保持原样
    async fn on_rejected(&self, st: &mut LoopState, view: &ConfigView, r: SubscriptionRejected) {
        let s = &view.settings;
        let now = Instant::now();
        let key = SubKey::of(&r.target);
        let conns: ConnectorMap = self.connectors.read().await.clone();
        let Some((name, conn)) = route(&conns, &key) else {
            return;
        };
        if !conn.subscriptions_snapshot().await.contains(&r.target) {
            debug!(
                "upstream rejection ignored, no longer subscribed: {} code={}",
                key, r.code
            );
            return;
        }
        if view.protected.contains(&key) {
            warn!(
                "config subscription rejected by upstream: {} code={} msg={} \
                 (config subscriptions are never changed automatically)",
                key, r.code, r.message
            );
            return;
        }
        let wait = st.record_rejection(&key, now, s);
        if st.demand_added.remove(&r.target) {
            self.metrics
                .demand_failed_total
                .fetch_add(1, Ordering::Relaxed);
            st.usage.remove(&key);
            if let Err(e) = conn.remove_subscriptions(vec![r.target.clone()]).await {
                warn!(
                    "failed to withdraw rejected on-demand subscription on connector {}: {} ({})",
                    name, e, key
                );
            }
            warn!(
                "runtime subscription rejected by upstream: {} code={} msg={}; \
                 on-demand subscription withdrawn, no retry for {}",
                key,
                r.code,
                r.message,
                fmt_dur(wait)
            );
        } else {
            warn!(
                "runtime subscription rejected by upstream: {} code={} msg={}; \
                 registered via REST/gRPC so it is kept, on-demand will not retry it for {}",
                key,
                r.code,
                r.message,
                fmt_dur(wait)
            );
        }
    }
}

fn describe(s: &Settings) -> String {
    format!(
        "idle_minutes={}, check_interval_seconds={}, startup_grace_minutes={}, max_runtime_subscriptions={}, on_demand_quote={}",
        s.idle.as_secs_f64() / 60.0,
        s.check_interval.as_secs_f64(),
        s.startup_grace.as_secs_f64() / 60.0,
        s.max_runtime_subscriptions,
        s.on_demand_quote
    )
}

fn new_ticker(every: Duration) -> tokio::time::Interval {
    let mut t = tokio::time::interval(every);
    t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(ex: &str, dt: DataType, sym: &str, iv: Option<&str>) -> SubscriptionTarget {
        SubscriptionTarget {
            exchange: ex.into(),
            data_type: dt,
            symbol: sym.into(),
            kline_interval: iv.map(str::to_string),
        }
    }

    #[test]
    fn key_normalizes_case_and_matches_publish_topic() {
        let k = SubKey::of(&target("Binance", DataType::Tick, "btcusdt", None));
        assert_eq!(k.exchange, "binance");
        assert_eq!(k.symbol, "BTCUSDT");
        assert_eq!(k.topic(), "tick.binance.BTCUSDT");
        assert_eq!(
            SubKey::of(&target("okx", DataType::Kline, "ETH-USDT-SWAP", Some("1H"))).topic(),
            "kline.1H.okx.ETH-USDT-SWAP"
        );
        assert_eq!(
            SubKey::of(&target("okx", DataType::Quote, "BNB-USDT-SWAP", None)).topic(),
            "quote.okx.BNB-USDT-SWAP"
        );
        // 与配置里小写写法的同一币对视为同一个键
        assert_eq!(
            SubKey::of(&target("binance", DataType::Tick, "BTCUSDT", None)),
            SubKey::of(&target("binance", DataType::Tick, "btcusdt", None))
        );
        // 级别大小写有含义
        assert_ne!(
            SubKey::of(&target("binance", DataType::Kline, "BTCUSDT", Some("1m"))),
            SubKey::of(&target("binance", DataType::Kline, "BTCUSDT", Some("1M")))
        );
    }

    #[test]
    fn only_canonical_topics_are_accepted() {
        let k = SubKey::from_canonical_topic("tick.okx.BTC-USDT-SWAP").unwrap();
        assert_eq!(
            k.target(),
            target("okx", DataType::Tick, "BTC-USDT-SWAP", None)
        );
        let k = SubKey::from_canonical_topic("kline.5m.binance.ETHUSDT").unwrap();
        assert_eq!(k.interval.as_deref(), Some("5m"));
        assert!(SubKey::from_canonical_topic("quote.binance.BNBUSDT").is_some());
        for bad in [
            "tick.binance.btcusdt",
            "tick.Binance.BTCUSDT",
            "tick.binance",
            "kline..binance.BTCUSDT",
            "tick..BTCUSDT",
            "trade.binance.BTCUSDT",
            "garbage",
        ] {
            assert!(SubKey::from_canonical_topic(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn binance_kline_interval_whitelist() {
        let k = |ex: &str, iv: &str| SubKey::new(ex, DataType::Kline, "BTCUSDT", Some(iv));
        assert!(interval_supported(&k("binance", "5m")));
        assert!(interval_supported(&k("binance", "1M")));
        assert!(!interval_supported(&k("binance", "7m")));
        assert!(!interval_supported(&k("binance", "1H")));
        // OKX 由上游明确拒绝，不做白名单
        assert!(interval_supported(&k("okx", "7m")));
        assert!(interval_supported(&SubKey::new(
            "binance",
            DataType::Tick,
            "BTCUSDT",
            None
        )));
    }

    #[test]
    fn rejection_backoff_doubles_and_caps() {
        let s = Settings {
            reject_backoff_initial: Duration::from_secs(300),
            reject_backoff_max: Duration::from_secs(3600),
            ..Settings::default()
        };
        let mut st = LoopState::new(Instant::now());
        let k = SubKey::new("okx", DataType::Tick, "NOSUCH-USDT-SWAP", None);
        let now = Instant::now();
        let waits: Vec<u64> = (0..6)
            .map(|_| st.record_rejection(&k, now, &s).as_secs())
            .collect();
        assert_eq!(waits, [300, 600, 1200, 2400, 3600, 3600]);
        assert_eq!(st.rejected[&k].retry_at, now + Duration::from_secs(3600));
    }

    #[test]
    fn token_bucket_limits_burst_and_refills() {
        let s = Settings {
            upstream_burst: 3,
            upstream_refill: Duration::from_secs(10),
            ..Settings::default()
        };
        let t0 = Instant::now();
        let mut b = TokenBucket {
            tokens: s.upstream_burst,
            last_refill: t0,
        };
        assert!(b.try_take(t0, &s));
        assert!(b.try_take(t0, &s));
        assert!(b.try_take(t0, &s));
        assert!(!b.try_take(t0, &s), "突发用完后必须等待补充");
        assert!(!b.try_take(t0 + Duration::from_secs(9), &s));
        assert!(b.try_take(t0 + Duration::from_secs(10), &s));
        assert!(!b.try_take(t0 + Duration::from_secs(10), &s));
        // 长时间空闲后最多补满到容量
        let later = t0 + Duration::from_secs(1000);
        assert!(b.available(later, &s));
        assert_eq!(b.tokens, 3);
    }

    #[test]
    fn metrics_render_all_families() {
        let m = Metrics::default();
        m.enabled.store(1, Ordering::Relaxed);
        m.limit_rejected_total.store(4, Ordering::Relaxed);
        let mut out = String::new();
        m.render_prometheus(&mut out);
        for (name, kind) in [
            ("md_runtime_subscriptions_enabled", "gauge"),
            ("md_runtime_subscriptions", "gauge"),
            (
                "md_runtime_subscriptions_idle_unsubscribed_total",
                "counter",
            ),
            (
                "md_runtime_subscriptions_demand_subscribed_total",
                "counter",
            ),
            ("md_runtime_subscriptions_demand_failed_total", "counter"),
            ("md_runtime_subscriptions_limit_rejected_total", "counter"),
        ] {
            assert!(out.contains(&format!("# TYPE {name} {kind}\n")), "{name}");
        }
        assert!(out.contains("md_runtime_subscriptions_enabled 1\n"));
        assert!(out.contains("md_runtime_subscriptions_limit_rejected_total 4\n"));
    }
}
