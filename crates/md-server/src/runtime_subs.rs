//! 运行期订阅管理的接线（1.2.0）：YAML `runtime_subscriptions` → 管理器设置；配置名单 → 受保护目标；SIGHUP 热更。

use crate::reload::RuntimeSubsSwitch;
use md_config::{Config, RuntimeSubscriptionsConfig};
use md_connector::DataType;
use md_gateway::runtime_subs::{ConfigView, RuntimeSubscriptions, Settings, SubKey};
use std::collections::HashSet;
use std::time::Duration;

pub(crate) fn settings_of(rs: &RuntimeSubscriptionsConfig) -> Settings {
    Settings {
        enabled: rs.idle_unsubscribe,
        idle: Duration::from_secs(rs.idle_minutes.saturating_mul(60)),
        check_interval: Duration::from_secs(rs.check_interval_seconds),
        startup_grace: Duration::from_secs(rs.startup_grace_minutes.saturating_mul(60)),
        max_runtime_subscriptions: rs.max_runtime_subscriptions,
        ..Settings::default()
    }
}

/// 配置名单里的全部目标。不看交易所 / quote 开关是否启用：多保护几个不存在的目标没有坏处。
pub(crate) fn config_keys(cfg: &Config) -> HashSet<SubKey> {
    let b = &cfg.connectors.binance;
    let o = &cfg.connectors.okx;
    let mut keys = HashSet::new();
    for (exchange, ticks, klines, quotes) in [
        (
            "binance",
            &b.subscribe_ticks,
            &b.subscribe_klines,
            &b.subscribe_quotes,
        ),
        (
            "okx",
            &o.subscribe_ticks,
            &o.subscribe_klines,
            &o.subscribe_quotes,
        ),
    ] {
        for s in ticks {
            keys.insert(SubKey::new(exchange, DataType::Tick, s, None));
        }
        for (interval, symbols) in klines {
            for s in symbols {
                keys.insert(SubKey::new(exchange, DataType::Kline, s, Some(interval)));
            }
        }
        for s in quotes {
            keys.insert(SubKey::new(exchange, DataType::Quote, s, None));
        }
    }
    keys
}

/// 设置取已应用配置；受保护名单取已应用配置与文件配置的并集
pub(crate) fn view_of(applied: &Config, file: &Config) -> ConfigView {
    let mut protected = config_keys(applied);
    protected.extend(config_keys(file));
    ConfigView {
        settings: settings_of(&applied.runtime_subscriptions),
        protected,
    }
}

impl RuntimeSubsSwitch for RuntimeSubscriptions {
    fn apply(&self, applied: &Config, file: &Config) {
        self.update_config(view_of(applied, file));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = r#"
connectors:
  binance:
    enabled: true
    stream_base_url: "wss://binance.test/market/stream"
    rest_base_url: "https://binance.test"
    subscribe_ticks: ["btcusdt", "ETHUSDT"]
    subscribe_klines: { "5m": ["BTCUSDT"], "1M": ["ETHUSDT"] }
    quotes_enabled: false
    subscribe_quotes: ["BNBUSDT"]
  okx:
    enabled: false
    stream_base_url_public: ""
    stream_base_url_business: ""
    rest_base_url: ""
    subscribe_ticks: ["BTC-USDT-SWAP"]
    subscribe_klines: { "1H": ["ETH-USDT-SWAP"] }
    subscribe_quotes: ["BNB-USDT-SWAP"]
runtime_subscriptions:
  idle_unsubscribe: true
  idle_minutes: 2
  check_interval_seconds: 10
  startup_grace_minutes: 0
  max_runtime_subscriptions: 5
"#;

    fn cfg() -> Config {
        md_config::load_config_from_str(YAML).unwrap()
    }

    #[test]
    fn every_config_list_is_protected_with_canonical_keys() {
        let keys = config_keys(&cfg());
        let k = |ex: &str, dt, s: &str, iv: Option<&str>| SubKey::new(ex, dt, s, iv);
        let expected: HashSet<SubKey> = [
            k("binance", DataType::Tick, "BTCUSDT", None),
            k("binance", DataType::Tick, "ETHUSDT", None),
            k("binance", DataType::Kline, "BTCUSDT", Some("5m")),
            k("binance", DataType::Kline, "ETHUSDT", Some("1M")),
            k("binance", DataType::Quote, "BNBUSDT", None),
            k("okx", DataType::Tick, "BTC-USDT-SWAP", None),
            k("okx", DataType::Kline, "ETH-USDT-SWAP", Some("1H")),
            k("okx", DataType::Quote, "BNB-USDT-SWAP", None),
        ]
        .into_iter()
        .collect();
        assert_eq!(keys, expected);
    }

    #[test]
    fn settings_follow_yaml() {
        let s = settings_of(&cfg().runtime_subscriptions);
        assert!(s.enabled);
        assert_eq!(s.idle, Duration::from_secs(120));
        assert_eq!(s.check_interval, Duration::from_secs(10));
        assert_eq!(s.startup_grace, Duration::ZERO);
        assert_eq!(s.max_runtime_subscriptions, 5);
        // 旧配置：关闭，默认 10 分钟 / 60 秒 / 10 分钟 / 200
        let legacy = settings_of(&RuntimeSubscriptionsConfig::default());
        assert!(!legacy.enabled);
        assert_eq!(legacy.idle, Duration::from_secs(600));
        assert_eq!(legacy.check_interval, Duration::from_secs(60));
        assert_eq!(legacy.startup_grace, Duration::from_secs(600));
        assert_eq!(legacy.max_runtime_subscriptions, 200);
    }

    #[test]
    fn view_protects_union_of_applied_and_file() {
        let applied = cfg();
        let mut file = cfg();
        file.connectors.binance.subscribe_ticks = vec!["SOLUSDT".into()];
        let view = view_of(&applied, &file);
        assert!(view
            .protected
            .contains(&SubKey::new("binance", DataType::Tick, "BTCUSDT", None)));
        assert!(view
            .protected
            .contains(&SubKey::new("binance", DataType::Tick, "SOLUSDT", None)));
        assert_eq!(view.settings, settings_of(&applied.runtime_subscriptions));
    }
}
