//! SIGHUP 配置重载。
//!
//! 设计要点（修复 1.1.0 / 1.1.1 的三个问题，见 `evidence/sighup-reload-fix/REPORT.md`）：
//!
//! 1. **区分"文件中的新配置"和"实际已应用配置"。** [`ReloadState`] 只保存进程**真正在用**的配置（`applied`）。
//!    每次 SIGHUP 都拿文件里的新配置和 `applied` 比较；只有真正热更成功的订阅列表才会写进 `applied`，
//!    需要重启才能生效的字段永远不会写进去。因此：重复发 SIGHUP 会持续提示、改回原值后提示自动消失，
//!    不会再出现"第二次 SIGHUP 误报已应用"。
//! 2. **启动和重载使用同一套"生效配置"推导**（YAML + 环境变量覆盖 + `--port-offset`，见 [`load_effective_config`]），
//!    否则带端口偏移 / 环境变量覆盖的部署会被误报"监听地址变更"。
//! 3. **字段分类用结构体穷尽解构**（[`restart_only_fields`]，不写 `..`）：给 `md_config::Config` 新增任何字段，
//!    这里会编译失败，逼着作者决定它是"可热更"还是"需重启"；不会再因为漏写一条 `if` 而漏检
//!    （OKX `stream_base_url_business` 就是这样漏掉的）。默认归类为"需重启"，宁可多提示也不误报成功。
//! 4. **订阅列表热更保持原有行为**（对运行中的连接器做增量 add/remove，不重连）。区别只有：连接器没在运行（启动时没创建）
//!    或下发失败的订阅组，不再被静默吞掉，而是作为"待处理"持续提示，且不推进 `applied`。
//! 5. **`log_level` 可热更**（见 [`crate::logging`]）：SIGHUP 直接切换日志过滤器，不重连、不重启。
//!    环境变量 `RUST_LOG` 优先于 YAML：设置了它时，`log_level` 的改动不会生效，作为待处理持续提示（并写明原因）。
//!
//! 约定：URL 等"需重启"的配置不做连接器热重建；自动重连只会重连启动时的地址（`ExchangeAdapter::ws_url()` 取自启动配置）。

use crate::logging::{LevelOutcome, LevelSwitch};
use md_config::{
    AdminServerConfig, ApiGatewayConfig, BinanceConfig, Config, ConfigError, ConnectorConfigs,
    GRPCConfig, OkxConfig, ProcessorConfig, WebSocketConfig,
};
use md_connector::{Connector, DataType, SubscriptionTarget};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::RwLock;

// 日志 target 固定为 `md_server`（与 1.1.0 中这些日志的前缀一致）：依赖日志前缀的 grep / 告警规则不受模块拆分影响。
macro_rules! info {
    ($($arg:tt)+) => { tracing::info!(target: "md_server", $($arg)+) };
}
macro_rules! warn {
    ($($arg:tt)+) => { tracing::warn!(target: "md_server", $($arg)+) };
}
macro_rules! error {
    ($($arg:tt)+) => { tracing::error!(target: "md_server", $($arg)+) };
}

/// 连接器映射（键：`binance` / `okx` / `okx-kline` / `binance-quote` / `okx-quote`）
pub(crate) type ConnectorMap = HashMap<String, Arc<dyn Connector>>;
pub(crate) type SharedConnectors = Arc<RwLock<ConnectorMap>>;

// ---------------------------------------------------------------------------
// 生效配置（启动与重载共用）
// ---------------------------------------------------------------------------

/// 将 ":PORT" / "IP:PORT" 格式的地址加上偏移量
pub(crate) fn offset_port(addr: &str, offset: u16) -> String {
    if let Some(port_str) = addr.strip_prefix(':') {
        if let Ok(port) = port_str.parse::<u16>() {
            return format!(":{}", port + offset);
        }
    }
    if let Ok(mut sa) = addr.parse::<std::net::SocketAddr>() {
        sa.set_port(sa.port() + offset);
        return sa.to_string();
    }
    addr.to_string()
}

/// 端口偏移：gRPC / API Gateway / Admin 三个监听地址（`offset == 0` 不做任何事）
fn apply_port_offset(cfg: &mut Config, offset: u16) {
    if offset == 0 {
        return;
    }
    cfg.grpc_server.listen_address = offset_port(&cfg.grpc_server.listen_address, offset);
    cfg.api_gateway.listen_address = offset_port(&cfg.api_gateway.listen_address, offset);
    cfg.admin_server.listen_address = offset_port(&cfg.admin_server.listen_address, offset);
}

/// 加载**生效配置** = YAML + 环境变量覆盖 + 端口偏移。
///
/// 启动（`main`）和 SIGHUP 重载必须走同一个函数：只要两边的推导不一致，
/// 带 `--port-offset` 或环境变量覆盖的部署，每次 SIGHUP 都会把"没改过"的监听地址误报为变更。
pub(crate) fn load_effective_config(path: &str, port_offset: u16) -> Result<Config, ConfigError> {
    let mut cfg = md_config::load_config(path)?;
    md_config::apply_env_overrides(&mut cfg);
    apply_port_offset(&mut cfg, port_offset);
    canonicalize_log_level(&mut cfg)?;
    Ok(cfg)
}

/// 把 `log_level` 统一成标准小写写法（`INFO`、`warning` 等都规范化），保证"比较是否变更"和"交给日志过滤器"用的是同一个值。
///
/// YAML 里的非法值在 `md_config::load_config` 的校验里已被拒绝；这里兜住的是环境变量 `LOG_LEVEL` 覆盖进来的非法值
/// （环境变量覆盖发生在校验之后）。
fn canonicalize_log_level(cfg: &mut Config) -> Result<(), ConfigError> {
    match md_config::normalize_log_level(&cfg.log_level) {
        Some(level) => {
            cfg.log_level = level.to_string();
            Ok(())
        }
        None => Err(ConfigError::ValidationError(format!(
            "{}（若 YAML 中的值合法，则来自环境变量 LOG_LEVEL 的覆盖）",
            md_config::invalid_log_level_message(&cfg.log_level)
        ))),
    }
}

// ---------------------------------------------------------------------------
// 待处理变更（文件里改了，但运行中的进程还没生效）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PendingKind {
    /// 该配置只在进程启动时读取：文件里的新值没有生效，需重启。
    RestartRequired {
        running: String,
        file: String,
        note: Option<String>,
    },
    /// 订阅列表变了，但对应连接器启动时没有创建（交易所未启用 / 列表为空 / quote 开关关闭），无处下发；需重启。
    ConnectorNotRunning {
        connector: &'static str,
        hint: &'static str,
    },
    /// 订阅变更下发失败：运行中的订阅集合保持旧列表，下一次 SIGHUP 会重试。
    ApplyFailed {
        connector: &'static str,
        error: String,
    },
    /// 日志级别切换失败（值在加载时已校验过，正常不会发生）：保持旧级别，下一次 SIGHUP 会重试。
    LogLevelFailed { error: String },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingChange {
    /// YAML 路径，如 `connectors.okx.stream_base_url_business`
    pub path: &'static str,
    pub kind: PendingKind,
}

impl PendingChange {
    /// 是否需要重启才能生效（下发 / 切换失败的项不是重启问题：修好原因再发一次 SIGHUP 即可）
    fn needs_restart(&self) -> bool {
        !matches!(
            self.kind,
            PendingKind::ApplyFailed { .. } | PendingKind::LogLevelFailed { .. }
        )
    }

    fn log(&self) {
        match &self.kind {
            PendingKind::RestartRequired {
                running,
                file,
                note,
            } => match note {
                Some(note) => warn!(
                    "  {} changed: {} -> {} (NOT applied, requires restart; {})",
                    self.path, running, file, note
                ),
                None => warn!(
                    "  {} changed: {} -> {} (NOT applied, requires restart)",
                    self.path, running, file
                ),
            },
            PendingKind::ConnectorNotRunning { connector, hint } => warn!(
                "  {}: subscription list changed but the {} connector is not running ({}) \
                 -- NOT applied, requires restart",
                self.path, connector, hint
            ),
            PendingKind::ApplyFailed { connector, error } => warn!(
                "  {}: failed to apply subscription changes to the {} connector: {} \
                 (running subscriptions keep the old list; retried on the next SIGHUP)",
                self.path, connector, error
            ),
            PendingKind::LogLevelFailed { error } => warn!(
                "  {}: failed to switch the log filter: {} \
                 (the running log level is unchanged; retried on the next SIGHUP)",
                self.path, error
            ),
        }
    }
}

/// 一次重载的结果
#[derive(Debug, Default)]
pub(crate) struct ReloadReport {
    /// 文件与运行配置之间仍未生效的差异（空 = 文件内容已全部生效）
    pub pending: Vec<PendingChange>,
}

impl ReloadReport {
    pub(crate) fn fully_applied(&self) -> bool {
        self.pending.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn has_pending(&self, path: &str) -> bool {
        self.pending.iter().any(|p| p.path == path)
    }

    fn log_verdict(&self) {
        if self.fully_applied() {
            info!("config reloaded and applied successfully");
            return;
        }
        let paths = self
            .pending
            .iter()
            .map(|p| p.path)
            .collect::<Vec<_>>()
            .join(", ");
        if self.pending.iter().any(PendingChange::needs_restart) {
            warn!(
                "some config changes require connector restart -- please restart the service \
                 ({} pending: {})",
                self.pending.len(),
                paths
            );
        } else {
            warn!(
                "some config changes failed to apply -- the running process keeps its previous \
                 settings for them; fix the cause and send SIGHUP again ({} pending: {})",
                self.pending.len(),
                paths
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 只在启动时读取的字段（需重启）
// ---------------------------------------------------------------------------

/// WebSocket 地址类配置的补充说明：运维常以为"等它断线重连就会换地址"，其实不会。
const URL_NOTE: &str =
    "connectors keep using the startup address; auto-reconnect does not re-read the config file";

fn show<T: Debug + ?Sized>(v: &T) -> String {
    format!("{:?}", v)
}

/// 全部"只在进程启动时读取"的配置项：`(YAML 路径, 值的展示串)`。
///
/// **穷尽解构、不写 `..`**：`Config` 及其子结构新增字段时这里编译失败，必须在此显式归类。
/// 三类 `subscribe_*` 订阅列表用 `_` 标出，由 [`SubGroup`] 热更；`log_level` 同样用 `_` 标出，
/// 由 [`ReloadState::apply`] 通过 [`LevelSwitch`] 热更。它们都不在此处比较。
fn restart_only_fields(cfg: &Config) -> Vec<(&'static str, String)> {
    let Config {
        log_level: _,
        grpc_server:
            GRPCConfig {
                listen_address: grpc_listen,
                enabled: grpc_enabled,
                read_timeout: grpc_read_timeout,
                write_timeout: grpc_write_timeout,
            },
        ws_server:
            WebSocketConfig {
                listen_address: ws_listen,
                enabled: ws_enabled,
                read_timeout: ws_read_timeout,
                write_timeout: ws_write_timeout,
                path: ws_path,
            },
        connectors:
            ConnectorConfigs {
                binance:
                    BinanceConfig {
                        enabled: binance_enabled,
                        stream_base_url: binance_stream_url,
                        rest_base_url: binance_rest_url,
                        subscribe_ticks: _,
                        subscribe_klines: _,
                        reconnect_delay: binance_reconnect_delay,
                        ping_interval: binance_ping_interval,
                        quotes_enabled: binance_quotes_enabled,
                        subscribe_quotes: _,
                        stream_base_url_public: binance_stream_url_public,
                    },
                okx:
                    OkxConfig {
                        enabled: okx_enabled,
                        stream_base_url_public: okx_stream_url_public,
                        stream_base_url_business: okx_stream_url_business,
                        rest_base_url: okx_rest_url,
                        subscribe_ticks: _,
                        subscribe_klines: _,
                        reconnect_delay: okx_reconnect_delay,
                        ping_interval: okx_ping_interval,
                        quotes_enabled: okx_quotes_enabled,
                        subscribe_quotes: _,
                    },
            },
        processor:
            ProcessorConfig {
                tick_channel_buffer,
                kline_channel_buffer,
                broadcast_capacity,
                quote_channel_buffer,
            },
        api_gateway:
            ApiGatewayConfig {
                enabled: gateway_enabled,
                listen_address: gateway_listen,
                market_data_grpc_target,
                admin_grpc_target,
                ws_ping_period,
                ws_write_wait,
                ws_max_message_size,
            },
        admin_server: AdminServerConfig {
            listen_address: admin_listen,
        },
    } = cfg;

    vec![
        ("grpc_server.listen_address", show(grpc_listen)),
        ("grpc_server.enabled", show(grpc_enabled)),
        ("grpc_server.read_timeout", show(grpc_read_timeout)),
        ("grpc_server.write_timeout", show(grpc_write_timeout)),
        ("ws_server.listen_address", show(ws_listen)),
        ("ws_server.enabled", show(ws_enabled)),
        ("ws_server.read_timeout", show(ws_read_timeout)),
        ("ws_server.write_timeout", show(ws_write_timeout)),
        ("ws_server.path", show(ws_path)),
        ("connectors.binance.enabled", show(binance_enabled)),
        (
            "connectors.binance.stream_base_url",
            show(binance_stream_url),
        ),
        (
            "connectors.binance.stream_base_url_public",
            show(binance_stream_url_public),
        ),
        ("connectors.binance.rest_base_url", show(binance_rest_url)),
        (
            "connectors.binance.reconnect_delay",
            show(binance_reconnect_delay),
        ),
        (
            "connectors.binance.ping_interval",
            show(binance_ping_interval),
        ),
        (
            "connectors.binance.quotes_enabled",
            show(binance_quotes_enabled),
        ),
        ("connectors.okx.enabled", show(okx_enabled)),
        (
            "connectors.okx.stream_base_url_public",
            show(okx_stream_url_public),
        ),
        (
            "connectors.okx.stream_base_url_business",
            show(okx_stream_url_business),
        ),
        ("connectors.okx.rest_base_url", show(okx_rest_url)),
        ("connectors.okx.reconnect_delay", show(okx_reconnect_delay)),
        ("connectors.okx.ping_interval", show(okx_ping_interval)),
        ("connectors.okx.quotes_enabled", show(okx_quotes_enabled)),
        ("processor.tick_channel_buffer", show(tick_channel_buffer)),
        ("processor.kline_channel_buffer", show(kline_channel_buffer)),
        ("processor.broadcast_capacity", show(broadcast_capacity)),
        ("processor.quote_channel_buffer", show(quote_channel_buffer)),
        ("api_gateway.enabled", show(gateway_enabled)),
        ("api_gateway.listen_address", show(gateway_listen)),
        (
            "api_gateway.market_data_grpc_target",
            show(market_data_grpc_target),
        ),
        ("api_gateway.admin_grpc_target", show(admin_grpc_target)),
        ("api_gateway.ws_ping_period", show(ws_ping_period)),
        ("api_gateway.ws_write_wait", show(ws_write_wait)),
        ("api_gateway.ws_max_message_size", show(ws_max_message_size)),
        ("admin_server.listen_address", show(admin_listen)),
    ]
}

/// 文件配置与**运行中配置**在"只在启动时读取"的字段上的全部差异。
pub(crate) fn restart_required_changes(applied: &Config, file: &Config) -> Vec<PendingChange> {
    restart_only_fields(applied)
        .into_iter()
        .zip(restart_only_fields(file))
        .filter(|((_, running), (_, new))| running != new)
        .map(|((path, running), (_, file))| PendingChange {
            path,
            kind: PendingKind::RestartRequired {
                running,
                file,
                note: match path {
                    p if p.contains("stream_base_url") => Some(URL_NOTE.to_string()),
                    _ => None,
                },
            },
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 订阅列表（可热更）
// ---------------------------------------------------------------------------

/// 订阅热更的分组：一个分组 = 一个连接器 + 它负责的订阅列表字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubGroup {
    /// `binance` 连接器：`subscribe_ticks` + `subscribe_klines`
    BinanceTickKline,
    /// `okx` 连接器（Public / trades）：`subscribe_ticks`
    OkxTick,
    /// `okx-kline` 连接器（Business / candles）：`subscribe_klines`
    OkxKline,
    /// `binance-quote` 连接器：`subscribe_quotes`
    BinanceQuote,
    /// `okx-quote` 连接器：`subscribe_quotes`
    OkxQuote,
}

impl SubGroup {
    pub(crate) const ALL: [SubGroup; 5] = [
        SubGroup::BinanceTickKline,
        SubGroup::OkxTick,
        SubGroup::OkxKline,
        SubGroup::BinanceQuote,
        SubGroup::OkxQuote,
    ];

    fn path(self) -> &'static str {
        match self {
            SubGroup::BinanceTickKline => "connectors.binance.subscribe_ticks/subscribe_klines",
            SubGroup::OkxTick => "connectors.okx.subscribe_ticks",
            SubGroup::OkxKline => "connectors.okx.subscribe_klines",
            SubGroup::BinanceQuote => "connectors.binance.subscribe_quotes",
            SubGroup::OkxQuote => "connectors.okx.subscribe_quotes",
        }
    }

    /// 连接器在 connectors 映射里的键
    pub(crate) fn connector(self) -> &'static str {
        match self {
            SubGroup::BinanceTickKline => "binance",
            SubGroup::OkxTick => "okx",
            SubGroup::OkxKline => "okx-kline",
            SubGroup::BinanceQuote => "binance-quote",
            SubGroup::OkxQuote => "okx-quote",
        }
    }

    /// 订阅目标里的交易所名
    fn exchange(self) -> &'static str {
        match self {
            SubGroup::BinanceTickKline | SubGroup::BinanceQuote => "binance",
            SubGroup::OkxTick | SubGroup::OkxKline | SubGroup::OkxQuote => "okx",
        }
    }

    /// 日志措辞里的数据类型前缀（保持与 1.1.0 日志一致："adding 1 tick subscriptions"）
    fn label(self) -> &'static str {
        match self {
            SubGroup::BinanceTickKline => "",
            SubGroup::OkxTick => "tick ",
            SubGroup::OkxKline => "kline ",
            SubGroup::BinanceQuote | SubGroup::OkxQuote => "quote ",
        }
    }

    /// 连接器不存在时的原因提示（对应 `start_connectors` 的创建条件）
    fn not_running_hint(self) -> &'static str {
        match self {
            SubGroup::BinanceTickKline => "binance.enabled=false at startup",
            SubGroup::OkxTick => "okx.enabled=false or subscribe_ticks was empty at startup",
            SubGroup::OkxKline => "okx.enabled=false or subscribe_klines was empty at startup",
            SubGroup::BinanceQuote | SubGroup::OkxQuote => {
                "quotes_enabled=false or list was empty at startup"
            }
        }
    }

    fn changed(self, applied: &Config, file: &Config) -> bool {
        let (a, f) = (&applied.connectors, &file.connectors);
        match self {
            SubGroup::BinanceTickKline => {
                a.binance.subscribe_ticks != f.binance.subscribe_ticks
                    || a.binance.subscribe_klines != f.binance.subscribe_klines
            }
            SubGroup::OkxTick => a.okx.subscribe_ticks != f.okx.subscribe_ticks,
            SubGroup::OkxKline => a.okx.subscribe_klines != f.okx.subscribe_klines,
            SubGroup::BinanceQuote => a.binance.subscribe_quotes != f.binance.subscribe_quotes,
            SubGroup::OkxQuote => a.okx.subscribe_quotes != f.okx.subscribe_quotes,
        }
    }

    /// 热更成功后，把该分组的列表写进"已应用配置"
    fn adopt(self, applied: &mut Config, file: &Config) {
        let (a, f) = (&mut applied.connectors, &file.connectors);
        match self {
            SubGroup::BinanceTickKline => {
                a.binance.subscribe_ticks = f.binance.subscribe_ticks.clone();
                a.binance.subscribe_klines = f.binance.subscribe_klines.clone();
            }
            SubGroup::OkxTick => a.okx.subscribe_ticks = f.okx.subscribe_ticks.clone(),
            SubGroup::OkxKline => a.okx.subscribe_klines = f.okx.subscribe_klines.clone(),
            SubGroup::BinanceQuote => {
                a.binance.subscribe_quotes = f.binance.subscribe_quotes.clone()
            }
            SubGroup::OkxQuote => a.okx.subscribe_quotes = f.okx.subscribe_quotes.clone(),
        }
    }

    /// 需要新增 / 移除的订阅目标（均已排序，保证下发顺序稳定）
    fn targets(
        self,
        applied: &Config,
        file: &Config,
    ) -> (Vec<SubscriptionTarget>, Vec<SubscriptionTarget>) {
        let (a, f) = (&applied.connectors, &file.connectors);
        let none: HashMap<String, Vec<String>> = HashMap::new();
        let ex = self.exchange();
        match self {
            SubGroup::BinanceTickKline => diff_targets(
                &build_targets(ex, &a.binance.subscribe_ticks, &a.binance.subscribe_klines),
                &build_targets(ex, &f.binance.subscribe_ticks, &f.binance.subscribe_klines),
            ),
            SubGroup::OkxTick => diff_targets(
                &build_targets(ex, &a.okx.subscribe_ticks, &none),
                &build_targets(ex, &f.okx.subscribe_ticks, &none),
            ),
            SubGroup::OkxKline => diff_targets(
                &build_targets(ex, &[], &a.okx.subscribe_klines),
                &build_targets(ex, &[], &f.okx.subscribe_klines),
            ),
            SubGroup::BinanceQuote => {
                diff_quote_targets(ex, &a.binance.subscribe_quotes, &f.binance.subscribe_quotes)
            }
            SubGroup::OkxQuote => {
                diff_quote_targets(ex, &a.okx.subscribe_quotes, &f.okx.subscribe_quotes)
            }
        }
    }
}

/// tick 列表 + kline（interval -> symbols）展开成订阅目标集合
fn build_targets(
    exchange: &str,
    ticks: &[String],
    klines: &HashMap<String, Vec<String>>,
) -> HashSet<SubscriptionTarget> {
    let mut set = HashSet::new();
    for sym in ticks {
        set.insert(SubscriptionTarget {
            exchange: exchange.into(),
            data_type: DataType::Tick,
            symbol: sym.clone(),
            kline_interval: None,
        });
    }
    for (interval, syms) in klines {
        for sym in syms {
            set.insert(SubscriptionTarget {
                exchange: exchange.into(),
                data_type: DataType::Kline,
                symbol: sym.clone(),
                kline_interval: Some(interval.clone()),
            });
        }
    }
    set
}

fn sort_targets(v: &mut [SubscriptionTarget]) {
    v.sort_by_key(|t| {
        (
            t.symbol.clone(),
            t.kline_interval.clone(),
            t.data_type.to_string(),
        )
    });
}

/// 订阅集合差集：返回 (新增, 移除)
fn diff_targets(
    old: &HashSet<SubscriptionTarget>,
    new: &HashSet<SubscriptionTarget>,
) -> (Vec<SubscriptionTarget>, Vec<SubscriptionTarget>) {
    let mut to_add: Vec<SubscriptionTarget> = new.difference(old).cloned().collect();
    let mut to_remove: Vec<SubscriptionTarget> = old.difference(new).cloned().collect();
    sort_targets(&mut to_add);
    sort_targets(&mut to_remove);
    (to_add, to_remove)
}

/// 计算 quote 订阅差集：返回 (新增, 移除)，均为带交易所名的订阅目标
pub(crate) fn diff_quote_targets(
    exchange: &str,
    old_list: &[String],
    new_list: &[String],
) -> (Vec<SubscriptionTarget>, Vec<SubscriptionTarget>) {
    let old_set: HashSet<&String> = old_list.iter().collect();
    let new_set: HashSet<&String> = new_list.iter().collect();
    let mk = |sym: &String| SubscriptionTarget {
        exchange: exchange.into(),
        data_type: DataType::Quote,
        symbol: sym.clone(),
        kline_interval: None,
    };
    let mut to_add: Vec<SubscriptionTarget> = new_set.difference(&old_set).map(|s| mk(s)).collect();
    let mut to_remove: Vec<SubscriptionTarget> =
        old_set.difference(&new_set).map(|s| mk(s)).collect();
    sort_targets(&mut to_add);
    sort_targets(&mut to_remove);
    (to_add, to_remove)
}

/// 单个订阅分组的热更结果
#[derive(Debug, PartialEq)]
pub(crate) enum GroupOutcome {
    /// 列表相对已应用配置没有变化
    Unchanged,
    /// 已下发给运行中的连接器
    Applied,
    /// 连接器没在运行（启动时没创建）：无处下发
    NotRunning,
    /// 下发失败（连接器返回错误）
    Failed(String),
}

/// 对一个订阅分组做增量热更。返回结果由调用方决定是否推进"已应用配置"。
pub(crate) async fn apply_group(
    conns: &ConnectorMap,
    group: SubGroup,
    applied: &Config,
    file: &Config,
) -> GroupOutcome {
    if !group.changed(applied, file) {
        return GroupOutcome::Unchanged;
    }
    let name = group.connector();
    let Some(connector) = conns.get(name) else {
        return GroupOutcome::NotRunning;
    };
    let (to_add, to_remove) = group.targets(applied, file);
    let label = group.label();
    let mut failure: Option<String> = None;

    if !to_add.is_empty() {
        info!("  {}: adding {} {}subscriptions", name, to_add.len(), label);
        if let Err(e) = connector.add_subscriptions(to_add).await {
            warn!("  {}: failed to add {}subscriptions: {}", name, label, e);
            failure.get_or_insert_with(|| format!("add failed: {}", e));
        }
    }
    if !to_remove.is_empty() {
        info!(
            "  {}: removing {} {}subscriptions",
            name,
            to_remove.len(),
            label
        );
        if let Err(e) = connector.remove_subscriptions(to_remove).await {
            warn!("  {}: failed to remove {}subscriptions: {}", name, label, e);
            failure.get_or_insert_with(|| format!("remove failed: {}", e));
        }
    }
    match failure {
        Some(e) => GroupOutcome::Failed(e),
        None => GroupOutcome::Applied,
    }
}

// ---------------------------------------------------------------------------
// 重载状态机
// ---------------------------------------------------------------------------

/// 运行中进程的"已应用配置"。**只有真正生效的内容才会写进来。**
pub(crate) struct ReloadState {
    applied: Config,
    /// 日志过滤器的热更接口
    log: Arc<dyn LevelSwitch>,
}

impl ReloadState {
    /// `startup_effective`：启动时实际使用的配置（已含环境变量覆盖和端口偏移）；
    /// `log`：日志级别的热更接口（生产里是 `logging::LogControl`）
    pub(crate) fn new(startup_effective: Config, log: Arc<dyn LevelSwitch>) -> Self {
        Self {
            applied: startup_effective,
            log,
        }
    }

    #[cfg(test)]
    pub(crate) fn applied(&self) -> &Config {
        &self.applied
    }

    /// 把文件里的新配置与"已应用配置"对比：订阅列表热更，其余需重启的差异作为待处理持续提示。
    pub(crate) async fn apply(
        &mut self,
        file: Config,
        connectors: &SharedConnectors,
    ) -> ReloadReport {
        let mut report = ReloadReport::default();
        let mut next = self.applied.clone();

        // 0. 日志级别：热更（不重连）。RUST_LOG 优先时不生效，作为待处理持续提示。
        //    放在最前面：调高详细度时，后面的订阅热更日志就能看到；调低时，"applying..."这行仍在旧级别下输出。
        let mut log_pending = None;
        if file.log_level != self.applied.log_level {
            match self.log.overridden_by() {
                Some(rust_log) => {
                    log_pending = Some(PendingChange {
                        path: "log_level",
                        kind: PendingKind::RestartRequired {
                            running: self.applied.log_level.clone(),
                            file: file.log_level.clone(),
                            note: Some(format!(
                                "RUST_LOG={rust_log} takes precedence over log_level; \
                                 unset RUST_LOG and restart to let log_level (and SIGHUP) control the log level"
                            )),
                        },
                    });
                }
                None => {
                    info!(
                        "  log_level changed: {} -> {}, applying...",
                        self.applied.log_level, file.log_level
                    );
                    match self.log.apply(&file.log_level) {
                        LevelOutcome::Applied => {
                            info!(
                                "  log_level changed: {} -> {} (applied)",
                                self.applied.log_level, file.log_level
                            );
                            next.log_level = file.log_level.clone();
                        }
                        LevelOutcome::Rejected(error) => {
                            log_pending = Some(PendingChange {
                                path: "log_level",
                                kind: PendingKind::LogLevelFailed { error },
                            });
                        }
                    }
                }
            }
        }

        // 1. 订阅列表：对运行中的连接器做增量 add/remove（不重连）
        let changed: Vec<SubGroup> = SubGroup::ALL
            .into_iter()
            .filter(|g| g.changed(&self.applied, &file))
            .collect();
        let mut sub_pending = Vec::new();
        if !changed.is_empty() {
            info!("  subscription lists changed, applying...");
            let conns = connectors.read().await;
            for group in changed {
                match apply_group(&conns, group, &self.applied, &file).await {
                    GroupOutcome::Unchanged => {}
                    GroupOutcome::Applied => group.adopt(&mut next, &file),
                    GroupOutcome::NotRunning => sub_pending.push(PendingChange {
                        path: group.path(),
                        kind: PendingKind::ConnectorNotRunning {
                            connector: group.connector(),
                            hint: group.not_running_hint(),
                        },
                    }),
                    GroupOutcome::Failed(error) => sub_pending.push(PendingChange {
                        path: group.path(),
                        kind: PendingKind::ApplyFailed {
                            connector: group.connector(),
                            error,
                        },
                    }),
                }
            }
        }

        // 2. 只在启动时读取的字段：文件值 != 运行值 → 需重启。
        //    注意：这些值**不会**写进 next，所以下次 SIGHUP 仍会对比出来、继续提示。
        report.pending = restart_required_changes(&self.applied, &file);
        report.pending.extend(log_pending);
        report.pending.extend(sub_pending);

        for p in &report.pending {
            p.log();
        }
        report.log_verdict();

        self.applied = next;
        report
    }
}

/// 处理一次 SIGHUP：读取文件 → 对比 → 热更 / 提示。读取或解析失败时保持运行配置不变并报错。
///
/// 返回 `None` 表示文件没能加载（运行配置、订阅均未改动）。
pub(crate) async fn handle_sighup(
    state: &mut ReloadState,
    config_path: &str,
    port_offset: u16,
    connectors: &SharedConnectors,
) -> Option<ReloadReport> {
    info!("received SIGHUP, reloading config from {}", config_path);
    match load_effective_config(config_path, port_offset) {
        Ok(file) => Some(state.apply(file, connectors).await),
        Err(e) => {
            error!(
                "failed to reload config: {}, keeping current running config (nothing applied)",
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use md_connector::ConnectorError;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    // ---- 测试夹具 ----

    const BASE_YAML: &str = r#"
log_level: "info"
grpc_server: { enabled: true, listen_address: "127.0.0.1:50051" }
admin_server: { listen_address: "127.0.0.1:50052" }
api_gateway: { enabled: true, listen_address: "127.0.0.1:8081" }
connectors:
  binance:
    enabled: true
    stream_base_url: "wss://binance.test/market/stream"
    stream_base_url_public: "wss://binance.test/public/stream"
    rest_base_url: "https://binance.test"
    subscribe_ticks: ["BTCUSDT"]
    subscribe_klines: { "1m": ["BTCUSDT"] }
    quotes_enabled: true
    subscribe_quotes: ["BNBUSDT"]
  okx:
    enabled: true
    stream_base_url_public: "wss://okx.test/public"
    stream_base_url_business: "wss://okx.test/business"
    rest_base_url: "https://okx.test"
    subscribe_ticks: ["BTC-USDT-SWAP"]
    subscribe_klines: { "1m": ["BTC-USDT-SWAP"] }
    quotes_enabled: true
    subscribe_quotes: ["BNB-USDT-SWAP"]
"#;

    fn base() -> Config {
        md_config::load_config_from_str(BASE_YAML).expect("base yaml must be valid")
    }

    struct MockConnector {
        added: Mutex<Vec<SubscriptionTarget>>,
        removed: Mutex<Vec<SubscriptionTarget>>,
        fail: AtomicBool,
    }

    impl MockConnector {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                added: Mutex::new(Vec::new()),
                removed: Mutex::new(Vec::new()),
                fail: AtomicBool::new(false),
            })
        }
        fn added(&self) -> Vec<(String, String)> {
            self.added
                .lock()
                .unwrap()
                .iter()
                .map(|t| (t.data_type.to_string(), t.symbol.clone()))
                .collect()
        }
        fn removed(&self) -> Vec<(String, String)> {
            self.removed
                .lock()
                .unwrap()
                .iter()
                .map(|t| (t.data_type.to_string(), t.symbol.clone()))
                .collect()
        }
        fn calls(&self) -> usize {
            self.added.lock().unwrap().len() + self.removed.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl Connector for MockConnector {
        fn name(&self) -> &str {
            "mock"
        }
        async fn start(&self) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn stop(&self) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn add_subscriptions(
            &self,
            targets: Vec<SubscriptionTarget>,
        ) -> Result<(), ConnectorError> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(ConnectorError::SubscribeFailed("mock failure".into()));
            }
            self.added.lock().unwrap().extend(targets);
            Ok(())
        }
        async fn remove_subscriptions(
            &self,
            targets: Vec<SubscriptionTarget>,
        ) -> Result<(), ConnectorError> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(ConnectorError::SubscribeFailed("mock failure".into()));
            }
            self.removed.lock().unwrap().extend(targets);
            Ok(())
        }
        fn current_subscriptions(&self) -> Vec<SubscriptionTarget> {
            Vec::new()
        }
    }

    /// 五个连接器全部在运行的映射 + 各自的 mock 句柄
    struct Rig {
        conns: SharedConnectors,
        binance: Arc<MockConnector>,
        okx: Arc<MockConnector>,
        okx_kline: Arc<MockConnector>,
        binance_quote: Arc<MockConnector>,
        okx_quote: Arc<MockConnector>,
    }

    fn rig(skip: &[&str]) -> Rig {
        let r = Rig {
            conns: Arc::new(RwLock::new(HashMap::new())),
            binance: MockConnector::new(),
            okx: MockConnector::new(),
            okx_kline: MockConnector::new(),
            binance_quote: MockConnector::new(),
            okx_quote: MockConnector::new(),
        };
        let all: [(&str, &Arc<MockConnector>); 5] = [
            ("binance", &r.binance),
            ("okx", &r.okx),
            ("okx-kline", &r.okx_kline),
            ("binance-quote", &r.binance_quote),
            ("okx-quote", &r.okx_quote),
        ];
        let mut map: ConnectorMap = HashMap::new();
        for (name, m) in all {
            if !skip.contains(&name) {
                map.insert(name.to_string(), m.clone() as Arc<dyn Connector>);
            }
        }
        *r.conns.try_write().unwrap() = map;
        r
    }

    /// 日志级别切换的 mock：记录被应用的级别；可模拟"RUST_LOG 优先"和"切换失败"
    struct MockLevel {
        overridden: Option<String>,
        fail: AtomicBool,
        applied: Mutex<Vec<String>>,
    }

    impl MockLevel {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                overridden: None,
                fail: AtomicBool::new(false),
                applied: Mutex::new(Vec::new()),
            })
        }
        fn overridden_by_rust_log(raw: &str) -> Arc<Self> {
            Arc::new(Self {
                overridden: Some(raw.to_string()),
                fail: AtomicBool::new(false),
                applied: Mutex::new(Vec::new()),
            })
        }
        fn applied(&self) -> Vec<String> {
            self.applied.lock().unwrap().clone()
        }
    }

    impl LevelSwitch for MockLevel {
        fn overridden_by(&self) -> Option<String> {
            self.overridden.clone()
        }
        fn apply(&self, level: &str) -> LevelOutcome {
            if self.fail.load(Ordering::SeqCst) {
                return LevelOutcome::Rejected("simulated failure".into());
            }
            self.applied.lock().unwrap().push(level.to_string());
            LevelOutcome::Applied
        }
    }

    /// 日志级别没有特别要求的测试用这个（RUST_LOG 未设置、切换总是成功）
    fn state_of(cfg: Config) -> ReloadState {
        ReloadState::new(cfg, MockLevel::new())
    }

    fn state_with(cfg: Config, level: &Arc<MockLevel>) -> ReloadState {
        ReloadState::new(cfg, level.clone())
    }

    const URL_PATHS: [&str; 4] = [
        "connectors.okx.stream_base_url_public",
        "connectors.okx.stream_base_url_business",
        "connectors.binance.stream_base_url",
        "connectors.binance.stream_base_url_public",
    ];

    fn set_url(cfg: &mut Config, path: &str, v: &str) {
        match path {
            "connectors.okx.stream_base_url_public" => {
                cfg.connectors.okx.stream_base_url_public = v.into()
            }
            "connectors.okx.stream_base_url_business" => {
                cfg.connectors.okx.stream_base_url_business = v.into()
            }
            "connectors.binance.stream_base_url" => {
                cfg.connectors.binance.stream_base_url = v.into()
            }
            "connectors.binance.stream_base_url_public" => {
                cfg.connectors.binance.stream_base_url_public = v.into()
            }
            other => panic!("unknown url path {other}"),
        }
    }

    fn get_url(cfg: &Config, path: &str) -> String {
        match path {
            "connectors.okx.stream_base_url_public" => {
                cfg.connectors.okx.stream_base_url_public.clone()
            }
            "connectors.okx.stream_base_url_business" => {
                cfg.connectors.okx.stream_base_url_business.clone()
            }
            "connectors.binance.stream_base_url" => cfg.connectors.binance.stream_base_url.clone(),
            "connectors.binance.stream_base_url_public" => {
                cfg.connectors.binance.stream_base_url_public.clone()
            }
            other => panic!("unknown url path {other}"),
        }
    }

    fn temp_file(tag: &str, content: &str) -> std::path::PathBuf {
        use std::sync::atomic::AtomicUsize;
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "md-server-reload-test-{}-{}-{}.yaml",
            std::process::id(),
            tag,
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::write(&p, content).unwrap();
        p
    }

    // ---- 1. "需重启"字段检测完整性 ----

    /// 每个"只在启动时读取"的字段，单独改动都必须被检测到，且路径正确。
    /// 表项数必须等于 `restart_only_fields` 的字段数：新增字段（穷尽解构会逼着加进列表）时，这里也必须补一行。
    #[rustfmt::skip]
    #[test]
    fn every_restart_only_field_is_detected_individually() {
        use std::time::Duration;
        type Mutator = Box<dyn Fn(&mut Config)>;
        let secs = |n| Duration::from_secs(n);
        let table: Vec<(&str, Mutator)> = vec![
            ("grpc_server.listen_address", Box::new(|c| c.grpc_server.listen_address = ":1".into())),
            ("grpc_server.enabled", Box::new(|c| c.grpc_server.enabled = !c.grpc_server.enabled)),
            ("grpc_server.read_timeout", Box::new(move |c| c.grpc_server.read_timeout = secs(99))),
            ("grpc_server.write_timeout", Box::new(move |c| c.grpc_server.write_timeout = secs(99))),
            ("ws_server.listen_address", Box::new(|c| c.ws_server.listen_address = ":1".into())),
            ("ws_server.enabled", Box::new(|c| c.ws_server.enabled = !c.ws_server.enabled)),
            ("ws_server.read_timeout", Box::new(move |c| c.ws_server.read_timeout = secs(99))),
            ("ws_server.write_timeout", Box::new(move |c| c.ws_server.write_timeout = secs(99))),
            ("ws_server.path", Box::new(|c| c.ws_server.path = "/x".into())),
            ("connectors.binance.enabled", Box::new(|c| c.connectors.binance.enabled = !c.connectors.binance.enabled)),
            ("connectors.binance.stream_base_url", Box::new(|c| c.connectors.binance.stream_base_url = "wss://x".into())),
            ("connectors.binance.stream_base_url_public", Box::new(|c| c.connectors.binance.stream_base_url_public = "wss://x".into())),
            ("connectors.binance.rest_base_url", Box::new(|c| c.connectors.binance.rest_base_url = "https://x".into())),
            ("connectors.binance.reconnect_delay", Box::new(move |c| c.connectors.binance.reconnect_delay = secs(99))),
            ("connectors.binance.ping_interval", Box::new(move |c| c.connectors.binance.ping_interval = secs(99))),
            ("connectors.binance.quotes_enabled", Box::new(|c| c.connectors.binance.quotes_enabled = !c.connectors.binance.quotes_enabled)),
            ("connectors.okx.enabled", Box::new(|c| c.connectors.okx.enabled = !c.connectors.okx.enabled)),
            ("connectors.okx.stream_base_url_public", Box::new(|c| c.connectors.okx.stream_base_url_public = "wss://x".into())),
            ("connectors.okx.stream_base_url_business", Box::new(|c| c.connectors.okx.stream_base_url_business = "wss://x".into())),
            ("connectors.okx.rest_base_url", Box::new(|c| c.connectors.okx.rest_base_url = "https://x".into())),
            ("connectors.okx.reconnect_delay", Box::new(move |c| c.connectors.okx.reconnect_delay = secs(99))),
            ("connectors.okx.ping_interval", Box::new(move |c| c.connectors.okx.ping_interval = secs(99))),
            ("connectors.okx.quotes_enabled", Box::new(|c| c.connectors.okx.quotes_enabled = !c.connectors.okx.quotes_enabled)),
            ("processor.tick_channel_buffer", Box::new(|c| c.processor.tick_channel_buffer += 1)),
            ("processor.kline_channel_buffer", Box::new(|c| c.processor.kline_channel_buffer += 1)),
            ("processor.broadcast_capacity", Box::new(|c| c.processor.broadcast_capacity += 1)),
            ("processor.quote_channel_buffer", Box::new(|c| c.processor.quote_channel_buffer += 1)),
            ("api_gateway.enabled", Box::new(|c| c.api_gateway.enabled = !c.api_gateway.enabled)),
            ("api_gateway.listen_address", Box::new(|c| c.api_gateway.listen_address = ":1".into())),
            ("api_gateway.market_data_grpc_target", Box::new(|c| c.api_gateway.market_data_grpc_target = "x:1".into())),
            ("api_gateway.admin_grpc_target", Box::new(|c| c.api_gateway.admin_grpc_target = "x:1".into())),
            ("api_gateway.ws_ping_period", Box::new(move |c| c.api_gateway.ws_ping_period = secs(99))),
            ("api_gateway.ws_write_wait", Box::new(move |c| c.api_gateway.ws_write_wait = secs(99))),
            ("api_gateway.ws_max_message_size", Box::new(|c| c.api_gateway.ws_max_message_size += 1)),
            ("admin_server.listen_address", Box::new(|c| c.admin_server.listen_address = ":1".into())),
        ];

        let applied = base();
        let listed: Vec<&str> = restart_only_fields(&applied).iter().map(|(p, _)| *p).collect();
        let unique: HashSet<&&str> = listed.iter().collect();
        assert_eq!(unique.len(), listed.len(), "restart_only_fields 路径必须唯一");
        assert_eq!(table.len(), listed.len(), "测试表必须覆盖 restart_only_fields 的每个字段");

        for (path, mutate) in &table {
            assert!(listed.contains(path), "{path} 不在 restart_only_fields 中");
            let mut file = applied.clone();
            mutate(&mut file);
            let changes = restart_required_changes(&applied, &file);
            assert_eq!(changes.len(), 1, "改动 {path} 应恰好检测出 1 项，实际: {changes:?}");
            assert_eq!(changes[0].path, *path);
        }
    }

    #[test]
    fn identical_configs_have_no_restart_changes() {
        assert!(restart_required_changes(&base(), &base()).is_empty());
    }

    #[test]
    fn subscription_lists_are_not_restart_changes() {
        let applied = base();
        let mut file = base();
        file.connectors
            .binance
            .subscribe_ticks
            .push("ETHUSDT".into());
        file.connectors.okx.subscribe_ticks.clear();
        file.connectors.okx.subscribe_klines.clear();
        file.connectors.binance.subscribe_quotes.clear();
        file.connectors
            .okx
            .subscribe_quotes
            .push("ETH-USDT-SWAP".into());
        assert!(restart_required_changes(&applied, &file).is_empty());
    }

    // ---- 2. 验收 (a)：只改 Business 地址 ----

    #[tokio::test]
    async fn okx_business_url_only_change_requires_restart_and_is_not_success() {
        let r = rig(&[]);
        let mut state = state_of(base());
        let mut file = base();
        file.connectors.okx.stream_base_url_business = "wss://okx.test/business-new".into();

        let report = state.apply(file, &r.conns).await;

        assert!(!report.fully_applied(), "不得报告全部应用成功");
        assert!(report.has_pending("connectors.okx.stream_base_url_business"));
        assert_eq!(report.pending.len(), 1);
        assert!(report.pending[0].needs_restart());
        // 没有任何订阅动作
        assert_eq!(r.okx_kline.calls() + r.okx.calls() + r.binance.calls(), 0);
    }

    // ---- 3. 验收 (b)：任一地址，连续多次 SIGHUP 都不得误报已应用 ----

    #[tokio::test]
    async fn every_url_stays_pending_across_consecutive_reloads_and_clears_on_revert() {
        for path in URL_PATHS {
            let r = rig(&[]);
            let original = base();
            let mut state = state_of(original.clone());
            let mut file = original.clone();
            set_url(&mut file, path, "wss://changed.test/ws");

            for n in 1..=3 {
                let report = state.apply(file.clone(), &r.conns).await;
                assert!(
                    !report.fully_applied(),
                    "{path}: 第 {n} 次 SIGHUP 不得报告成功"
                );
                assert!(report.has_pending(path), "{path}: 第 {n} 次必须点名");
                // 关键：已应用配置没有被"尚未生效的 URL"污染
                assert_eq!(
                    get_url(state.applied(), path),
                    get_url(&original, path),
                    "{path}: 已应用配置不得记录未生效的 URL"
                );
            }

            // 文件改回原值：提示自动消失
            let report = state.apply(original.clone(), &r.conns).await;
            assert!(report.fully_applied(), "{path}: 改回原值后应无待处理项");
            assert_eq!(get_url(state.applied(), path), get_url(&original, path));
        }
    }

    /// 问题 3：自动重连不会读新配置。URL 类提示必须直接写明这一点，避免运维以为"等断线重连就好了"。
    #[test]
    fn url_changes_explain_that_reconnect_keeps_the_startup_address() {
        for path in URL_PATHS {
            let applied = base();
            let mut file = base();
            set_url(&mut file, path, "wss://changed.test/ws");
            let changes = restart_required_changes(&applied, &file);
            assert_eq!(changes.len(), 1);
            match &changes[0].kind {
                PendingKind::RestartRequired { note: Some(n), .. } => {
                    assert!(n.contains("auto-reconnect"), "{path}: {n}")
                }
                other => panic!("{path}: 期望带说明的 RestartRequired，实际 {other:?}"),
            }
        }
        // 非 URL 字段不带这条说明
        let mut file = base();
        file.processor.broadcast_capacity += 1;
        match &restart_required_changes(&base(), &file)[0].kind {
            PendingKind::RestartRequired { note, .. } => assert!(note.is_none()),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn pending_restart_items_accumulate_and_stay_visible() {
        let r = rig(&[]);
        let mut state = state_of(base());
        let mut file = base();
        file.connectors.okx.stream_base_url_public = "wss://a".into();
        file.connectors.binance.stream_base_url = "wss://b".into();
        file.connectors.okx.reconnect_delay = std::time::Duration::from_secs(77);
        for _ in 0..2 {
            let report = state.apply(file.clone(), &r.conns).await;
            assert_eq!(report.pending.len(), 3);
            assert!(!report.fully_applied());
        }
    }

    // ---- 4. 验收 (d)：订阅列表热更新不受影响 ----

    #[tokio::test]
    async fn subscription_hot_update_applies_once_and_updates_applied_lists() {
        let r = rig(&[]);
        let mut state = state_of(base());
        let mut file = base();
        file.connectors.binance.subscribe_ticks = vec!["BTCUSDT".into(), "ETHUSDT".into()];
        file.connectors.okx.subscribe_ticks = vec!["ETH-USDT-SWAP".into()]; // 加 ETH、删 BTC
        file.connectors.okx.subscribe_klines.insert(
            "1m".into(),
            vec!["BTC-USDT-SWAP".into(), "ETH-USDT-SWAP".into()],
        );
        file.connectors.binance.subscribe_quotes = vec!["BNBUSDT".into(), "ETHUSDT".into()];
        file.connectors.okx.subscribe_quotes = vec![];

        let report = state.apply(file.clone(), &r.conns).await;
        assert!(
            report.fully_applied(),
            "纯订阅变更应全部热更成功: {:?}",
            report.pending
        );

        assert_eq!(r.binance.added(), vec![("TICK".into(), "ETHUSDT".into())]);
        assert!(r.binance.removed().is_empty());
        assert_eq!(r.okx.added(), vec![("TICK".into(), "ETH-USDT-SWAP".into())]);
        assert_eq!(
            r.okx.removed(),
            vec![("TICK".into(), "BTC-USDT-SWAP".into())]
        );
        assert_eq!(
            r.okx_kline.added(),
            vec![("KLINE".into(), "ETH-USDT-SWAP".into())]
        );
        assert_eq!(
            r.binance_quote.added(),
            vec![("QUOTE".into(), "ETHUSDT".into())]
        );
        assert_eq!(
            r.okx_quote.removed(),
            vec![("QUOTE".into(), "BNB-USDT-SWAP".into())]
        );

        // 已应用配置的订阅列表已推进到文件里的值
        assert_eq!(
            state.applied().connectors.okx.subscribe_ticks,
            file.connectors.okx.subscribe_ticks
        );
        assert_eq!(
            state.applied().connectors.binance.subscribe_quotes,
            file.connectors.binance.subscribe_quotes
        );

        // 第二次 SIGHUP：不得重复下发
        let before = r.binance.calls()
            + r.okx.calls()
            + r.okx_kline.calls()
            + r.binance_quote.calls()
            + r.okx_quote.calls();
        let report = state.apply(file, &r.conns).await;
        assert!(report.fully_applied());
        let after = r.binance.calls()
            + r.okx.calls()
            + r.okx_kline.calls()
            + r.binance_quote.calls()
            + r.okx_quote.calls();
        assert_eq!(before, after, "未变化的订阅列表不得重复下发");
    }

    #[tokio::test]
    async fn subscription_hot_update_coexists_with_pending_url_change() {
        let r = rig(&[]);
        let mut state = state_of(base());
        let mut file = base();
        file.connectors.okx.stream_base_url_business = "wss://okx.test/business-new".into();
        file.connectors
            .okx
            .subscribe_ticks
            .push("ETH-USDT-SWAP".into());

        let report = state.apply(file.clone(), &r.conns).await;
        assert_eq!(
            r.okx.added(),
            vec![("TICK".into(), "ETH-USDT-SWAP".into())],
            "订阅照常热更"
        );
        assert!(!report.fully_applied());
        assert_eq!(
            report.pending.len(),
            1,
            "只有 URL 待重启: {:?}",
            report.pending
        );
        assert!(report.has_pending("connectors.okx.stream_base_url_business"));

        // 第二次：订阅不重复下发；URL 继续提示
        let report = state.apply(file, &r.conns).await;
        assert_eq!(r.okx.calls(), 1, "订阅不得重复下发");
        assert!(report.has_pending("connectors.okx.stream_base_url_business"));
        assert!(!report.fully_applied());
    }

    #[tokio::test]
    async fn unchanged_file_is_fully_applied_and_touches_no_connector() {
        let r = rig(&[]);
        let mut state = state_of(base());
        let report = state.apply(base(), &r.conns).await;
        assert!(report.fully_applied());
        assert_eq!(
            r.binance.calls()
                + r.okx.calls()
                + r.okx_kline.calls()
                + r.binance_quote.calls()
                + r.okx_quote.calls(),
            0
        );
    }

    // ---- 5. 连接器没在运行 / 下发失败：不静默、不推进已应用配置 ----

    #[tokio::test]
    async fn missing_connector_keeps_group_pending_until_reverted() {
        // 启动时没有 okx-kline、okx-quote（klines 为空 / quote 关闭）
        let r = rig(&["okx-kline", "okx-quote"]);
        let mut applied = base();
        applied.connectors.okx.subscribe_klines.clear();
        applied.connectors.okx.subscribe_quotes.clear();
        let mut state = state_of(applied.clone());

        let mut file = applied.clone();
        file.connectors
            .okx
            .subscribe_klines
            .insert("1m".into(), vec!["BTC-USDT-SWAP".into()]);
        file.connectors.okx.subscribe_quotes = vec!["BNB-USDT-SWAP".into()];

        for n in 1..=2 {
            let report = state.apply(file.clone(), &r.conns).await;
            assert!(!report.fully_applied(), "第 {n} 次不得报成功");
            assert!(
                report.has_pending("connectors.okx.subscribe_klines"),
                "第 {n} 次"
            );
            assert!(
                report.has_pending("connectors.okx.subscribe_quotes"),
                "第 {n} 次"
            );
            assert!(report
                .pending
                .iter()
                .all(|p| matches!(p.kind, PendingKind::ConnectorNotRunning { .. })));
            // 已应用配置没有被推进
            assert!(state.applied().connectors.okx.subscribe_klines.is_empty());
            assert!(state.applied().connectors.okx.subscribe_quotes.is_empty());
        }

        // 改回去：提示消失
        let report = state.apply(applied, &r.conns).await;
        assert!(report.fully_applied());
    }

    #[tokio::test]
    async fn failed_apply_keeps_old_list_and_is_retried_on_next_reload() {
        let r = rig(&[]);
        let original = base();
        let mut state = state_of(original.clone());
        let mut file = base();
        file.connectors
            .okx
            .subscribe_ticks
            .push("ETH-USDT-SWAP".into());

        r.okx.fail.store(true, Ordering::SeqCst);
        let report = state.apply(file.clone(), &r.conns).await;
        assert!(!report.fully_applied(), "下发失败不得报告成功");
        assert!(matches!(
            report.pending[0].kind,
            PendingKind::ApplyFailed { .. }
        ));
        assert!(
            !report.pending[0].needs_restart(),
            "下发失败可重试，不是需重启"
        );
        assert_eq!(
            state.applied().connectors.okx.subscribe_ticks,
            original.connectors.okx.subscribe_ticks
        );

        r.okx.fail.store(false, Ordering::SeqCst);
        let report = state.apply(file.clone(), &r.conns).await;
        assert!(
            report.fully_applied(),
            "故障恢复后重试应成功: {:?}",
            report.pending
        );
        assert_eq!(r.okx.added(), vec![("TICK".into(), "ETH-USDT-SWAP".into())]);
        assert_eq!(
            state.applied().connectors.okx.subscribe_ticks,
            file.connectors.okx.subscribe_ticks
        );
    }

    // ---- 6. 验收 (e)：读取 / 解析 / 校验失败 → 保留运行配置并报错 ----

    #[tokio::test]
    async fn load_failure_keeps_running_config_and_touches_nothing() {
        let r = rig(&[]);
        let original = base();
        let mut state = state_of(original.clone());

        // 文件不存在
        assert!(handle_sighup(
            &mut state,
            "/nonexistent/md-server/config.yaml",
            0,
            &r.conns
        )
        .await
        .is_none());
        // YAML 语法错误
        let bad = temp_file("syntax", "connectors: [unclosed\n  binance: {\n");
        assert!(
            handle_sighup(&mut state, bad.to_str().unwrap(), 0, &r.conns)
                .await
                .is_none()
        );
        // 校验失败：okx.enabled 但 business 地址为空
        let invalid = temp_file(
            "validate",
            &BASE_YAML.replace("wss://okx.test/business", ""),
        );
        assert!(
            handle_sighup(&mut state, invalid.to_str().unwrap(), 0, &r.conns)
                .await
                .is_none()
        );

        assert_eq!(state.applied(), &original, "运行配置必须原样保留");
        assert_eq!(
            r.binance.calls()
                + r.okx.calls()
                + r.okx_kline.calls()
                + r.binance_quote.calls()
                + r.okx_quote.calls(),
            0
        );

        // 之后文件修好：正常恢复
        let good = temp_file(
            "good",
            &BASE_YAML.replace(
                "[\"BTC-USDT-SWAP\"]\n    subscribe_klines",
                "[\"BTC-USDT-SWAP\", \"ETH-USDT-SWAP\"]\n    subscribe_klines",
            ),
        );
        let report = handle_sighup(&mut state, good.to_str().unwrap(), 0, &r.conns)
            .await
            .expect("修好后应能加载");
        assert!(report.fully_applied());
        assert_eq!(r.okx.added(), vec![("TICK".into(), "ETH-USDT-SWAP".into())]);

        for p in [bad, invalid, good] {
            let _ = std::fs::remove_file(p);
        }
    }

    // ---- 7. 启动与重载的生效配置推导一致（端口偏移） ----

    #[test]
    fn effective_config_is_derived_identically_for_startup_and_reload() {
        let path = temp_file("offset", BASE_YAML);
        let startup = load_effective_config(path.to_str().unwrap(), 10).unwrap();
        let reload = load_effective_config(path.to_str().unwrap(), 10).unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!(startup.grpc_server.listen_address, "127.0.0.1:50061");
        assert_eq!(startup.api_gateway.listen_address, "127.0.0.1:8091");
        assert_eq!(startup.admin_server.listen_address, "127.0.0.1:50062");
        assert!(
            restart_required_changes(&startup, &reload).is_empty(),
            "没改文件时，带端口偏移的重载不得检测出任何监听地址变更"
        );
    }

    #[test]
    fn offset_port_handles_both_address_forms_and_zero() {
        assert_eq!(offset_port(":50051", 10), ":50061");
        assert_eq!(offset_port("127.0.0.1:8081", 10), "127.0.0.1:8091");
        assert_eq!(offset_port("not-an-address", 10), "not-an-address");

        let mut cfg = base();
        let before = cfg.clone();
        apply_port_offset(&mut cfg, 0);
        assert_eq!(cfg, before, "offset=0 不得改动任何地址");
    }

    // ---- 8. 订阅 diff（从 main.rs 迁入，语义不变） ----

    #[test]
    fn diff_quote_targets_add_and_remove() {
        let old = vec!["BNBUSDT".to_string(), "BTCUSDT".to_string()];
        let new = vec!["BNBUSDT".to_string(), "ETHUSDT".to_string()];
        let (add, remove) = diff_quote_targets("binance", &old, &new);
        assert_eq!(add.len(), 1);
        assert_eq!(add[0].symbol, "ETHUSDT");
        assert_eq!(add[0].exchange, "binance");
        assert_eq!(add[0].data_type, DataType::Quote);
        assert_eq!(remove.len(), 1);
        assert_eq!(remove[0].symbol, "BTCUSDT");

        // 无变化 → 无操作
        let (add, remove) = diff_quote_targets("okx", &old, &old);
        assert!(add.is_empty() && remove.is_empty());
    }

    /// 旧版配置（无任何 quote 键）：SIGHUP 的 quote 逻辑不应产生任何订阅动作；
    /// 连接器不存在时不 panic、不创建连接器，并且明确返回 NotRunning（不再静默）。
    #[tokio::test]
    async fn quote_group_noop_when_lists_equal_or_connector_missing() {
        let conns: ConnectorMap = HashMap::new();
        let applied = base();

        // 列表相同：Unchanged
        assert_eq!(
            apply_group(&conns, SubGroup::BinanceQuote, &applied, &applied).await,
            GroupOutcome::Unchanged
        );
        // 列表不同但连接器未启动：NotRunning（不 panic，不创建连接器）
        let mut file = applied.clone();
        file.connectors.binance.subscribe_quotes = vec!["ETHUSDT".into()];
        assert_eq!(
            apply_group(&conns, SubGroup::BinanceQuote, &applied, &file).await,
            GroupOutcome::NotRunning
        );
        assert!(conns.is_empty());
    }

    // ---- 9. log_level 热更 ----

    #[test]
    fn log_level_is_not_a_restart_only_field() {
        // log_level 不再属于"只在启动时读取"的字段：它由 SIGHUP 热更
        assert!(restart_only_fields(&base())
            .iter()
            .all(|(path, _)| *path != "log_level"));
        let applied = base();
        let mut file = base();
        file.log_level = "debug".into();
        assert!(restart_required_changes(&applied, &file).is_empty());
    }

    #[tokio::test]
    async fn log_level_change_is_hot_applied_once_and_adopted() {
        let r = rig(&[]);
        let lv = MockLevel::new();
        let mut state = state_with(base(), &lv);
        let mut file = base();
        file.log_level = "debug".into();

        let report = state.apply(file.clone(), &r.conns).await;
        assert!(
            report.fully_applied(),
            "热更成功应报告全部生效: {:?}",
            report.pending
        );
        assert_eq!(lv.applied(), vec!["debug"]);
        assert_eq!(
            state.applied().log_level,
            "debug",
            "热更成功后应推进已应用配置"
        );

        // 第二次 SIGHUP：文件没变，不得重复切换
        let report = state.apply(file.clone(), &r.conns).await;
        assert!(report.fully_applied());
        assert_eq!(lv.applied(), vec!["debug"], "不得重复应用同一个级别");

        // 再调低
        file.log_level = "warn".into();
        let report = state.apply(file, &r.conns).await;
        assert!(report.fully_applied());
        assert_eq!(lv.applied(), vec!["debug", "warn"]);
        assert_eq!(state.applied().log_level, "warn");

        // 日志级别与连接器无关：不得有任何订阅动作
        assert_eq!(r.okx.calls() + r.binance.calls() + r.okx_kline.calls(), 0);
    }

    #[tokio::test]
    async fn unchanged_log_level_never_touches_the_filter() {
        let r = rig(&[]);
        let lv = MockLevel::new();
        let mut state = state_with(base(), &lv);
        let report = state.apply(base(), &r.conns).await;
        assert!(report.fully_applied());
        assert!(lv.applied().is_empty());
    }

    #[tokio::test]
    async fn rust_log_override_keeps_log_level_pending_across_reloads_and_clears_on_revert() {
        let r = rig(&[]);
        let lv = MockLevel::overridden_by_rust_log("warn");
        let mut state = state_with(base(), &lv); // 启动时 YAML = info，但实际过滤器来自 RUST_LOG=warn
        let mut file = base();
        file.log_level = "debug".into();

        for n in 1..=3 {
            let report = state.apply(file.clone(), &r.conns).await;
            assert!(
                !report.fully_applied(),
                "第 {n} 次：RUST_LOG 优先时不得报告 log_level 已生效"
            );
            assert!(
                report.has_pending("log_level"),
                "第 {n} 次必须点名 log_level"
            );
            match &report.pending[0].kind {
                PendingKind::RestartRequired {
                    running,
                    file,
                    note: Some(note),
                } => {
                    assert_eq!(running, "info");
                    assert_eq!(file, "debug");
                    assert!(
                        note.contains("RUST_LOG=warn"),
                        "必须说明是被 RUST_LOG 覆盖: {note}"
                    );
                }
                other => panic!("期望带说明的 RestartRequired，实际 {other:?}"),
            }
            assert!(report.pending[0].needs_restart());
            assert!(lv.applied().is_empty(), "RUST_LOG 优先时不得去动过滤器");
            assert_eq!(
                state.applied().log_level,
                "info",
                "未生效的值不得写进已应用配置"
            );
        }

        // 文件改回启动时的值：提示消失
        let report = state.apply(base(), &r.conns).await;
        assert!(report.fully_applied());
    }

    #[tokio::test]
    async fn log_level_failure_keeps_old_level_and_is_retried_on_next_reload() {
        let r = rig(&[]);
        let lv = MockLevel::new();
        let mut state = state_with(base(), &lv);
        let mut file = base();
        file.log_level = "debug".into();

        lv.fail.store(true, Ordering::SeqCst);
        let report = state.apply(file.clone(), &r.conns).await;
        assert!(!report.fully_applied());
        assert!(report.has_pending("log_level"));
        assert!(matches!(
            report.pending[0].kind,
            PendingKind::LogLevelFailed { .. }
        ));
        assert!(!report.pending[0].needs_restart(), "切换失败不是重启问题");
        assert_eq!(state.applied().log_level, "info", "失败时保持旧级别");

        // 原因消除后再发一次 SIGHUP：重试成功
        lv.fail.store(false, Ordering::SeqCst);
        let report = state.apply(file, &r.conns).await;
        assert!(report.fully_applied());
        assert_eq!(lv.applied(), vec!["debug"]);
        assert_eq!(state.applied().log_level, "debug");
    }

    #[tokio::test]
    async fn log_level_hot_update_coexists_with_url_pending_and_subscription_update() {
        let r = rig(&[]);
        let lv = MockLevel::new();
        let mut state = state_with(base(), &lv);
        let mut file = base();
        file.log_level = "debug".into();
        file.connectors.okx.stream_base_url_business = "wss://okx.test/business-new".into();
        file.connectors.okx.subscribe_ticks = vec!["ETH-USDT-SWAP".into()];

        let report = state.apply(file, &r.conns).await;
        assert_eq!(
            report.pending.len(),
            1,
            "只剩地址变更待重启: {:?}",
            report.pending
        );
        assert!(report.has_pending("connectors.okx.stream_base_url_business"));
        assert_eq!(lv.applied(), vec!["debug"], "日志级别照常热更");
        assert_eq!(
            r.okx.added(),
            vec![("TICK".into(), "ETH-USDT-SWAP".into())],
            "订阅热更不受影响"
        );
    }

    #[test]
    fn effective_config_canonicalizes_log_level_spellings() {
        for (written, want) in [("INFO", "info"), ("Warning", "warn"), ("debug", "debug")] {
            let yaml =
                BASE_YAML.replace("log_level: \"info\"", &format!("log_level: \"{written}\""));
            let path = temp_file("canon", &yaml);
            let cfg = load_effective_config(path.to_str().unwrap(), 0).unwrap();
            let _ = std::fs::remove_file(path);
            assert_eq!(cfg.log_level, want, "写法 {written:?}");
        }
    }

    #[test]
    fn canonicalize_log_level_rejects_invalid_values_such_as_a_bad_log_level_env() {
        // 环境变量 LOG_LEVEL 的覆盖发生在 YAML 校验之后，所以这里要兜底
        let mut cfg = base();
        cfg.log_level = "verbose".into();
        let err = canonicalize_log_level(&mut cfg).unwrap_err().to_string();
        assert!(
            err.contains("log_level") && err.contains("verbose") && err.contains("LOG_LEVEL"),
            "{err}"
        );
        assert_eq!(cfg.log_level, "verbose", "失败时不改动原值");

        let mut ok = base();
        ok.log_level = " Error ".into();
        canonicalize_log_level(&mut ok).unwrap();
        assert_eq!(ok.log_level, "error");
    }

    #[tokio::test]
    async fn sighup_with_invalid_log_level_in_file_keeps_running_level() {
        let r = rig(&[]);
        let lv = MockLevel::new();
        let original = base();
        let mut state = state_with(original.clone(), &lv);
        let bad = temp_file(
            "badlevel",
            &BASE_YAML.replace("log_level: \"info\"", "log_level: \"verbose\""),
        );

        let report = handle_sighup(&mut state, bad.to_str().unwrap(), 0, &r.conns).await;
        let _ = std::fs::remove_file(bad);

        assert!(report.is_none(), "非法 log_level 必须按「加载失败」处理");
        assert!(lv.applied().is_empty());
        assert_eq!(state.applied(), &original, "运行配置必须原样保留");
    }

    #[tokio::test]
    async fn sighup_applies_log_level_written_in_any_accepted_spelling() {
        let r = rig(&[]);
        let lv = MockLevel::new();
        let mut state = state_with(base(), &lv);
        let good = temp_file(
            "warnlevel",
            &BASE_YAML.replace("log_level: \"info\"", "log_level: \"WARNING\""),
        );

        let report = handle_sighup(&mut state, good.to_str().unwrap(), 0, &r.conns)
            .await
            .expect("应能加载");
        let _ = std::fs::remove_file(good);

        assert!(report.fully_applied());
        assert_eq!(
            lv.applied(),
            vec!["warn"],
            "WARNING 应规范成 warn 再交给过滤器"
        );
    }
}
