//! 日志过滤器：启动时初始化；运行中可由 SIGHUP 热更 YAML 的 `log_level`。
//!
//! # 优先级（高 → 低）
//!
//! 1. 环境变量 `RUST_LOG`（设置、非空且合法时）：整个过滤器由它决定，可以写 `warn,md_connector=debug`
//!    这类逐模块指令；此时 YAML 的 `log_level` **不起作用**，SIGHUP 改它也不会生效（会明确提示）。
//! 2. YAML `log_level`（含环境变量 `LOG_LEVEL` 的覆盖），缺省 `info`；SIGHUP 可热更，不用重启、不会重连。
//!
//! 取"环境变量优先于配置文件"这一通行约定：现网（198、compose、systemd 样例）都设了 `RUST_LOG=warn`，
//! 沿用它的优先级，升级后日志量不会被悄悄改变；想让 YAML / SIGHUP 接管日志级别，去掉 `RUST_LOG` 即可。
//! 两者同时存在且写得不一致时，启动日志会 WARN 点名，不会让人以为 YAML 在起作用。
//!
//! # 历史
//!
//! 1.1.0 / 1.1.1 里 `log_level` 只被读出来打印一行，从未接到日志过滤器上（`main.rs` 注释写的是
//! "默认 info，可通过 RUST_LOG 或配置覆盖"，"配置覆盖"从来没实现过）。

use md_config::normalize_log_level;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{reload, EnvFilter, Registry};

pub(crate) const RUST_LOG_ENV: &str = "RUST_LOG";

// ---------------------------------------------------------------------------
// 启动方案（纯函数，便于测试）
// ---------------------------------------------------------------------------

/// 过滤器的来源
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FilterSource {
    /// 来自 YAML `log_level`（含 `LOG_LEVEL` 环境变量覆盖）：可由 SIGHUP 热更
    Config,
    /// 来自环境变量 `RUST_LOG`（原文）：优先于 YAML，SIGHUP 不能改变它
    RustLog(String),
}

/// 启动时的日志方案：用哪个过滤器、为什么、需要提醒什么
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Plan {
    /// 交给 `EnvFilter` 的指令串（标准级别名，或 `RUST_LOG` 原文）
    pub directives: String,
    pub source: FilterSource,
    /// 启动时输出的一行 INFO：过滤器来自哪里
    pub summary: String,
    /// 需要引起注意的提示（`RUST_LOG` 与配置冲突 / `RUST_LOG` 不合法），以 WARN 输出
    pub warnings: Vec<String>,
}

/// 决定启动时的日志过滤器。
///
/// - `config_level`：生效配置里的 `log_level`（必须已经是标准小写写法，见 `reload::load_effective_config`）
/// - `rust_log`：环境变量 `RUST_LOG` 的值（未设置传 `None`；设置为空串视同未设置）
pub(crate) fn plan(config_level: &str, rust_log: Option<&str>) -> Plan {
    let from_config = |note: &str, warnings: Vec<String>| Plan {
        directives: config_level.to_string(),
        source: FilterSource::Config,
        summary: format!("log filter: {config_level} (from config log_level; {note})"),
        warnings,
    };

    let Some(raw) = rust_log.map(str::trim).filter(|s| !s.is_empty()) else {
        return from_config("RUST_LOG is not set", Vec::new());
    };

    // 不合法的 RUST_LOG：旧版本悄悄退回 info；现在退回配置里的级别，并明确告警。
    if let Err(e) = EnvFilter::try_new(raw) {
        return from_config(
            "RUST_LOG is invalid and ignored",
            vec![format!(
                "{RUST_LOG_ENV}={raw:?} is invalid ({e}); ignoring it and using config log_level \"{config_level}\""
            )],
        );
    }

    let mut warnings = Vec::new();
    // `warning` 是 log_level 接受的别名，但 EnvFilter 不认识它（会当成 target 名），所以不算"普通级别"
    let plain = normalize_log_level(raw).filter(|_| !raw.eq_ignore_ascii_case("warning"));
    if let Some(plain) = plain {
        if plain != config_level {
            warnings.push(format!(
                "config log_level \"{config_level}\" is overridden by {RUST_LOG_ENV}={raw}: \
                 {RUST_LOG_ENV} takes precedence; unset {RUST_LOG_ENV} (and restart) to let \
                 log_level and SIGHUP control the log level"
            ));
        }
    }
    Plan {
        directives: raw.to_string(),
        source: FilterSource::RustLog(raw.to_string()),
        summary: format!(
            "log filter: {raw} (from {RUST_LOG_ENV}; config log_level \"{config_level}\" is \
             ignored while {RUST_LOG_ENV} is set)"
        ),
        warnings,
    }
}

// ---------------------------------------------------------------------------
// 运行中的控制句柄
// ---------------------------------------------------------------------------

/// 热更日志级别的结果
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LevelOutcome {
    Applied,
    Rejected(String),
}

/// SIGHUP 用来切换日志级别的接口（生产实现是 [`LogControl`]；测试里用 mock）
pub(crate) trait LevelSwitch: Send + Sync {
    /// 过滤器由 `RUST_LOG` 决定时返回它的原文：此时 YAML 的 `log_level` 不起作用，调用 `apply` 没有意义
    fn overridden_by(&self) -> Option<String>;
    /// 把过滤器切到 `level`（标准小写级别名）
    fn apply(&self, level: &str) -> LevelOutcome;
}

/// 全局日志过滤器的热更句柄
pub(crate) struct LogControl {
    handle: reload::Handle<EnvFilter, Registry>,
    source: FilterSource,
}

impl LevelSwitch for LogControl {
    fn overridden_by(&self) -> Option<String> {
        match &self.source {
            FilterSource::RustLog(raw) => Some(raw.clone()),
            FilterSource::Config => None,
        }
    }

    fn apply(&self, level: &str) -> LevelOutcome {
        if let Some(raw) = self.overridden_by() {
            return LevelOutcome::Rejected(format!(
                "{RUST_LOG_ENV}={raw} takes precedence over log_level"
            ));
        }
        let Some(level) = normalize_log_level(level) else {
            return LevelOutcome::Rejected(md_config::invalid_log_level_message(level));
        };
        let filter = match EnvFilter::try_new(level) {
            Ok(f) => f,
            Err(e) => return LevelOutcome::Rejected(e.to_string()),
        };
        match self.handle.reload(filter) {
            Ok(()) => LevelOutcome::Applied,
            Err(e) => LevelOutcome::Rejected(e.to_string()),
        }
    }
}

/// 初始化全局日志（只能在进程启动时调用一次）。
///
/// 输出格式与 1.1.x 完全相同（`tracing_subscriber::fmt` 默认格式、stdout）；
/// 区别只是过滤器套了一层 `reload`，以便 SIGHUP 热更 `log_level`。
pub(crate) fn init(config_level: &str) -> LogControl {
    let rust_log = std::env::var(RUST_LOG_ENV).ok();
    let plan = plan(config_level, rust_log.as_deref());
    let filter = EnvFilter::try_new(&plan.directives).unwrap_or_else(|_| EnvFilter::new("info"));
    let (filter_layer, handle) = reload::Layer::new(filter);
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!(target: "md_server", "{}", plan.summary);
    for w in &plan.warnings {
        tracing::warn!(target: "md_server", "{}", w);
    }
    LogControl {
        handle,
        source: plan.source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::Layer;

    // ---- plan()：启动时的优先级 ----

    #[test]
    fn config_level_is_used_when_rust_log_is_unset_or_blank() {
        for rust_log in [None, Some(""), Some("   ")] {
            let p = plan("warn", rust_log);
            assert_eq!(p.directives, "warn", "rust_log={rust_log:?}");
            assert_eq!(p.source, FilterSource::Config);
            assert!(p.warnings.is_empty());
            assert!(p.summary.contains("from config log_level"), "{}", p.summary);
        }
    }

    #[test]
    fn rust_log_takes_precedence_over_config() {
        let p = plan("info", Some("warn,md_connector=debug"));
        assert_eq!(p.directives, "warn,md_connector=debug");
        assert_eq!(
            p.source,
            FilterSource::RustLog("warn,md_connector=debug".into())
        );
        assert!(p.summary.contains("from RUST_LOG"), "{}", p.summary);
        // 逐模块指令无法和单一级别比较：只说明来源，不当作"冲突"告警
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    }

    #[test]
    fn plain_rust_log_that_disagrees_with_config_warns_loudly() {
        // 线上 198 的真实组合：YAML log_level=info，容器环境变量 RUST_LOG=warn
        let p = plan("info", Some("warn"));
        assert_eq!(p.directives, "warn");
        assert_eq!(p.source, FilterSource::RustLog("warn".into()));
        assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings);
        let w = &p.warnings[0];
        assert!(
            w.contains("log_level \"info\"")
                && w.contains("RUST_LOG=warn")
                && w.contains("overridden"),
            "{w}"
        );
    }

    #[test]
    fn plain_rust_log_that_agrees_with_config_is_quiet() {
        for (cfg, env) in [("warn", "warn"), ("info", "INFO"), ("error", " error ")] {
            let p = plan(cfg, Some(env));
            assert!(p.warnings.is_empty(), "{cfg} vs {env}: {:?}", p.warnings);
            assert_eq!(p.source, FilterSource::RustLog(env.trim().into()));
        }
    }

    #[test]
    fn invalid_rust_log_falls_back_to_config_and_says_so() {
        let p = plan("warn", Some("info,md_connector=verbose"));
        assert_eq!(p.directives, "warn");
        assert_eq!(p.source, FilterSource::Config);
        assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings);
        assert!(
            p.warnings[0].contains("RUST_LOG") && p.warnings[0].contains("invalid"),
            "{}",
            p.warnings[0]
        );
    }

    #[test]
    fn every_plan_produces_a_valid_env_filter() {
        for (cfg, env) in [
            ("info", None),
            ("debug", Some("warn")),
            ("trace", Some("warn,md_connector=debug")),
            ("error", Some("bogus=")),
        ] {
            let p = plan(cfg, env);
            assert!(
                EnvFilter::try_new(&p.directives).is_ok(),
                "{cfg}/{env:?}: {}",
                p.directives
            );
        }
    }

    // ---- LogControl：真实的 reload 句柄 ----

    /// 捕获被放行的事件消息
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<String>>>);

    struct MessageVisitor(String);
    impl tracing::field::Visit for MessageVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = format!("{value:?}");
            }
        }
    }

    impl<S: tracing::Subscriber> Layer<S> for Capture {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut v = MessageVisitor(String::new());
            event.record(&mut v);
            self.0.lock().unwrap().push(v.0);
        }
    }

    fn control(
        source: FilterSource,
        initial: &str,
    ) -> (LogControl, impl tracing::Subscriber + Send + Sync, Capture) {
        let (layer, handle) = reload::Layer::new(EnvFilter::new(initial));
        let capture = Capture::default();
        let subscriber = Registry::default().with(layer).with(capture.clone());
        (LogControl { handle, source }, subscriber, capture)
    }

    #[test]
    fn hot_apply_changes_which_events_pass_without_reinstalling_the_subscriber() {
        let (ctl, subscriber, capture) = control(FilterSource::Config, "info");
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("lc1-info-before");
            tracing::debug!("lc1-debug-before");

            assert_eq!(ctl.apply("debug"), LevelOutcome::Applied);
            tracing::debug!("lc1-debug-after-raise");

            assert_eq!(ctl.apply("WARN"), LevelOutcome::Applied); // 大小写不敏感
            tracing::info!("lc1-info-after-lower");
            tracing::warn!("lc1-warn-after-lower");

            assert_eq!(ctl.apply("error"), LevelOutcome::Applied);
            tracing::warn!("lc1-warn-after-error-level");
            tracing::error!("lc1-error-after-error-level");
        });
        assert_eq!(
            *capture.0.lock().unwrap(),
            vec![
                "lc1-info-before",
                "lc1-debug-after-raise",
                "lc1-warn-after-lower",
                "lc1-error-after-error-level",
            ]
        );
    }

    #[test]
    fn current_filter_reflects_last_applied_level() {
        let (ctl, _subscriber, _capture) = control(FilterSource::Config, "info");
        assert_eq!(ctl.apply("warning"), LevelOutcome::Applied); // warning = warn
        let now = ctl.handle.with_current(|f| f.to_string()).unwrap();
        assert_eq!(now, "warn");
    }

    #[test]
    fn rust_log_overrides_and_is_never_touched_by_apply() {
        let (ctl, _subscriber, _capture) = control(FilterSource::RustLog("warn".into()), "warn");
        assert_eq!(ctl.overridden_by().as_deref(), Some("warn"));
        match ctl.apply("debug") {
            LevelOutcome::Rejected(e) => assert!(e.contains("RUST_LOG=warn"), "{e}"),
            other => panic!("RUST_LOG 优先时 apply 必须拒绝，实际 {other:?}"),
        }
        // 过滤器保持不变
        assert_eq!(ctl.handle.with_current(|f| f.to_string()).unwrap(), "warn");
    }

    #[test]
    fn invalid_level_is_rejected_and_keeps_the_running_filter() {
        let (ctl, _subscriber, _capture) = control(FilterSource::Config, "info");
        for bad in ["verbose", "", "info,debug", "md_server=trace"] {
            assert!(
                matches!(ctl.apply(bad), LevelOutcome::Rejected(_)),
                "{bad:?} 必须被拒绝"
            );
        }
        assert_eq!(ctl.handle.with_current(|f| f.to_string()).unwrap(), "info");
    }

    #[test]
    fn apply_after_subscriber_is_gone_is_reported_not_panicked() {
        let (ctl, subscriber, _capture) = control(FilterSource::Config, "info");
        drop(subscriber);
        assert!(matches!(ctl.apply("warn"), LevelOutcome::Rejected(_)));
    }
}
