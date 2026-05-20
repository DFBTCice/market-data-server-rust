use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

// ---- 配置结构体定义 ----

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Config {
    pub log_level: String,
    pub grpc_server: GRPCConfig,
    pub ws_server: WebSocketConfig,
    pub connectors: ConnectorConfigs,
    pub processor: ProcessorConfig,
    pub api_gateway: ApiGatewayConfig,
    pub admin_server: AdminServerConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GRPCConfig {
    pub listen_address: String,
    pub enabled: bool,
    #[serde(with = "humantime_serde")]
    pub read_timeout: Duration,
    #[serde(with = "humantime_serde")]
    pub write_timeout: Duration,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WebSocketConfig {
    pub listen_address: String,
    pub enabled: bool,
    #[serde(with = "humantime_serde")]
    pub read_timeout: Duration,
    #[serde(with = "humantime_serde")]
    pub write_timeout: Duration,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ConnectorConfigs {
    pub binance: BinanceConfig,
    pub okx: OkxConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BinanceConfig {
    pub enabled: bool,
    pub stream_base_url: String,
    pub rest_base_url: String,
    pub subscribe_ticks: Vec<String>,
    pub subscribe_klines: HashMap<String, Vec<String>>,
    #[serde(with = "humantime_serde")]
    pub reconnect_delay: Duration,
    #[serde(with = "humantime_serde")]
    pub ping_interval: Duration,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct OkxConfig {
    pub enabled: bool,
    pub stream_base_url_public: String,
    pub stream_base_url_business: String,
    pub rest_base_url: String,
    pub subscribe_ticks: Vec<String>,
    pub subscribe_klines: HashMap<String, Vec<String>>,
    #[serde(with = "humantime_serde")]
    pub reconnect_delay: Duration,
    #[serde(with = "humantime_serde")]
    pub ping_interval: Duration,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ProcessorConfig {
    pub tick_channel_buffer: usize,
    pub kline_channel_buffer: usize,
    /// broadcast channel 容量（默认 4096，高吞吐场景可调大）
    pub broadcast_capacity: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AdminServerConfig {
    pub listen_address: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiGatewayConfig {
    pub enabled: bool,
    pub listen_address: String,
    pub market_data_grpc_target: String,
    pub admin_grpc_target: String,
    #[serde(with = "humantime_serde")]
    pub ws_ping_period: Duration,
    #[serde(with = "humantime_serde")]
    pub ws_write_wait: Duration,
    pub ws_max_message_size: i64,
}

// ---- 加载函数 ----

/// 从文件路径加载配置
pub fn load_config(path: &str) -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::IoError(format!("{}: {}", path, e)))?;
    load_config_from_str(&content)
}

/// 从 YAML 字符串加载配置（带默认值填充）
pub fn load_config_from_str(yaml: &str) -> Result<Config, ConfigError> {
    let raw: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

    let with_defaults = merge_defaults(raw);

    let cfg: Config = serde_yaml::from_value(with_defaults)
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

    validate(&cfg)?;

    Ok(cfg)
}

/// 用环境变量覆盖配置
pub fn apply_env_overrides(cfg: &mut Config) {
    if let Ok(val) = std::env::var("GRPC_SERVER_LISTEN_ADDRESS") {
        cfg.grpc_server.listen_address = val;
    }
    if let Ok(val) = std::env::var("GRPC_SERVER_ENABLED") {
        cfg.grpc_server.enabled = val.parse().unwrap_or(cfg.grpc_server.enabled);
    }
    if let Ok(val) = std::env::var("WS_SERVER_LISTEN_ADDRESS") {
        cfg.ws_server.listen_address = val;
    }
    if let Ok(val) = std::env::var("LOG_LEVEL") {
        cfg.log_level = val;
    }
    if let Ok(val) = std::env::var("API_GATEWAY_LISTEN_ADDRESS") {
        cfg.api_gateway.listen_address = val;
    }
    if let Ok(val) = std::env::var("API_GATEWAY_ENABLED") {
        cfg.api_gateway.enabled = val.parse().unwrap_or(cfg.api_gateway.enabled);
    }
    if let Ok(val) = std::env::var("ADMIN_SERVER_LISTEN_ADDRESS") {
        cfg.admin_server.listen_address = val;
    }
}

#[derive(Debug)]
pub enum ConfigError {
    IoError(String),
    ParseError(String),
    ValidationError(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::IoError(s) => write!(f, "IO error: {}", s),
            ConfigError::ParseError(s) => write!(f, "Parse error: {}", s),
            ConfigError::ValidationError(s) => write!(f, "Validation error: {}", s),
        }
    }
}

impl std::error::Error for ConfigError {}

// ---- 内部函数 ----

fn merge_defaults(raw: serde_yaml::Value) -> serde_yaml::Value {
    use serde_yaml::Value;

    let mut map = match raw {
        Value::Mapping(m) => m,
        _ => return raw,
    };

    set_default(&mut map, "log_level", Value::String("info".into()));

    let grpc = ensure_mapping(&mut map, "grpc_server");
    set_default(grpc, "enabled", Value::Bool(true));
    set_default(grpc, "listen_address", Value::String(":50051".into()));
    set_default(grpc, "read_timeout", Value::String("10s".into()));
    set_default(grpc, "write_timeout", Value::String("10s".into()));

    let ws = ensure_mapping(&mut map, "ws_server");
    set_default(ws, "enabled", Value::Bool(true));
    set_default(ws, "listen_address", Value::String(":8080".into()));
    set_default(ws, "read_timeout", Value::String("60s".into()));
    set_default(ws, "write_timeout", Value::String("10s".into()));
    set_default(ws, "path", Value::String("/ws".into()));

    let connectors = ensure_mapping(&mut map, "connectors");
    let binance = ensure_mapping(connectors, "binance");
    set_default(binance, "enabled", Value::Bool(false));
    set_default(binance, "reconnect_delay", Value::String("5s".into()));
    set_default(binance, "ping_interval", Value::String("3m".into()));

    let okx = ensure_mapping(connectors, "okx");
    set_default(okx, "enabled", Value::Bool(false));
    set_default(okx, "reconnect_delay", Value::String("10s".into()));
    set_default(okx, "ping_interval", Value::String("25s".into()));

    let proc = ensure_mapping(&mut map, "processor");
    set_default(proc, "tick_channel_buffer", Value::Number(1000.into()));
    set_default(proc, "kline_channel_buffer", Value::Number(1000.into()));
    set_default(proc, "broadcast_capacity", Value::Number(4096.into()));

    let admin = ensure_mapping(&mut map, "admin_server");
    set_default(admin, "listen_address", Value::String(":50052".into()));

    let gw = ensure_mapping(&mut map, "api_gateway");
    set_default(gw, "enabled", Value::Bool(true));
    set_default(gw, "listen_address", Value::String(":8081".into()));
    set_default(gw, "market_data_grpc_target", Value::String("localhost:50051".into()));
    set_default(gw, "admin_grpc_target", Value::String("localhost:50052".into()));
    set_default(gw, "ws_ping_period", Value::String("30s".into()));
    set_default(gw, "ws_write_wait", Value::String("10s".into()));
    set_default(gw, "ws_max_message_size", Value::Number(1024.into()));

    Value::Mapping(map)
}

fn set_default(map: &mut serde_yaml::Mapping, key: &str, default: serde_yaml::Value) {
    let k = serde_yaml::Value::String(key.to_string());
    if !map.contains_key(&k) {
        map.insert(k, default);
    }
}

fn ensure_mapping<'a>(
    parent: &'a mut serde_yaml::Mapping,
    key: &str,
) -> &'a mut serde_yaml::Mapping {
    let k = serde_yaml::Value::String(key.to_string());
    let entry = parent.entry(k).or_insert_with(|| {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    });
    if !matches!(entry, serde_yaml::Value::Mapping(_)) {
        *entry = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    }
    match entry {
        serde_yaml::Value::Mapping(m) => m,
        _ => unreachable!(),
    }
}

fn validate(cfg: &Config) -> Result<(), ConfigError> {
    if cfg.connectors.binance.enabled {
        if cfg.connectors.binance.stream_base_url.trim().is_empty() {
            return Err(ConfigError::ValidationError(
                "binance 连接器已启用（enabled=true）但未配置 stream_base_url".into(),
            ));
        }
        if cfg.connectors.binance.rest_base_url.trim().is_empty() {
            return Err(ConfigError::ValidationError(
                "binance 连接器已启用但未配置 rest_base_url".into(),
            ));
        }
    }
    if cfg.connectors.okx.enabled {
        if cfg.connectors.okx.stream_base_url_public.trim().is_empty() {
            return Err(ConfigError::ValidationError(
                "okx 连接器已启用但未配置 stream_base_url_public".into(),
            ));
        }
        if cfg.connectors.okx.stream_base_url_business.trim().is_empty() {
            return Err(ConfigError::ValidationError(
                "okx 连接器已启用但未配置 stream_base_url_business".into(),
            ));
        }
        if cfg.connectors.okx.rest_base_url.trim().is_empty() {
            return Err(ConfigError::ValidationError(
                "okx 连接器已启用但未配置 rest_base_url".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_default_config() {
        let cfg = load_config("../../config.yaml").unwrap();
        assert_eq!(cfg.log_level, "info");
        assert_eq!(cfg.grpc_server.listen_address, ":50051");
        assert!(cfg.grpc_server.enabled);
        assert!(cfg.connectors.binance.enabled);
        assert_eq!(cfg.connectors.binance.stream_base_url, "wss://fstream.binance.com/market/stream");
        assert_eq!(cfg.api_gateway.listen_address, ":8081");
        assert_eq!(cfg.admin_server.listen_address, ":50052");
    }

    #[test]
    fn load_config_from_yaml_string() {
        let yaml = r#"
log_level: "warn"
grpc_server:
  listen_address: ":9999"
  enabled: false
  read_timeout: "5s"
  write_timeout: "5s"
ws_server:
  listen_address: ":9090"
  enabled: true
  read_timeout: "30s"
  write_timeout: "5s"
  path: "/ws"
connectors:
  binance:
    enabled: false
    stream_base_url: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "3m"
  okx:
    enabled: false
    stream_base_url_public: ""
    stream_base_url_business: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "25s"
processor:
  tick_channel_buffer: 500
  kline_channel_buffer: 500
admin_server:
  listen_address: ":50052"
api_gateway:
  enabled: true
  listen_address: ":8081"
  market_data_grpc_target: "localhost:50051"
  admin_grpc_target: "localhost:50052"
  ws_ping_period: "30s"
  ws_write_wait: "10s"
  ws_max_message_size: 1024
"#;
        let cfg = load_config_from_str(yaml).unwrap();
        assert_eq!(cfg.log_level, "warn");
        assert_eq!(cfg.grpc_server.listen_address, ":9999");
        assert!(!cfg.grpc_server.enabled);
        assert_eq!(cfg.processor.tick_channel_buffer, 500);
    }

    #[test]
    fn env_var_override() {
        let yaml = r#"
log_level: "info"
grpc_server:
  listen_address: ":50051"
  enabled: true
  read_timeout: "10s"
  write_timeout: "10s"
ws_server:
  listen_address: ":8080"
  enabled: true
  read_timeout: "60s"
  write_timeout: "10s"
  path: "/ws"
connectors:
  binance:
    enabled: false
    stream_base_url: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "3m"
  okx:
    enabled: false
    stream_base_url_public: ""
    stream_base_url_business: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "25s"
processor:
  tick_channel_buffer: 1000
  kline_channel_buffer: 1000
admin_server:
  listen_address: ":50052"
api_gateway:
  enabled: true
  listen_address: ":8081"
  market_data_grpc_target: "localhost:50051"
  admin_grpc_target: "localhost:50052"
  ws_ping_period: "30s"
  ws_write_wait: "10s"
  ws_max_message_size: 1024
"#;
        let mut cfg = load_config_from_str(yaml).unwrap();
        assert_eq!(cfg.grpc_server.listen_address, ":50051");

        std::env::set_var("GRPC_SERVER_LISTEN_ADDRESS", ":7777");
        apply_env_overrides(&mut cfg);
        assert_eq!(cfg.grpc_server.listen_address, ":7777");

        std::env::remove_var("GRPC_SERVER_LISTEN_ADDRESS");
    }

    #[test]
    fn missing_fields_use_defaults() {
        let yaml = r#"
log_level: "info"
connectors:
  binance:
    enabled: false
    stream_base_url: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "3m"
  okx:
    enabled: false
    stream_base_url_public: ""
    stream_base_url_business: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "25s"
"#;
        let cfg = load_config_from_str(yaml).unwrap();
        assert_eq!(cfg.grpc_server.listen_address, ":50051");
        assert!(cfg.grpc_server.enabled);
        assert_eq!(cfg.ws_server.path, "/ws");
        assert_eq!(cfg.processor.tick_channel_buffer, 1000);
        assert_eq!(cfg.admin_server.listen_address, ":50052");
    }

    #[test]
    fn binance_enabled_without_url_fails() {
        let yaml = r#"
log_level: "info"
grpc_server:
  listen_address: ":50051"
  enabled: true
  read_timeout: "10s"
  write_timeout: "10s"
ws_server:
  listen_address: ":8080"
  enabled: true
  read_timeout: "60s"
  write_timeout: "10s"
  path: "/ws"
connectors:
  binance:
    enabled: true
    stream_base_url: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "3m"
  okx:
    enabled: false
    stream_base_url_public: ""
    stream_base_url_business: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "25s"
processor:
  tick_channel_buffer: 1000
  kline_channel_buffer: 1000
admin_server:
  listen_address: ":50052"
api_gateway:
  enabled: true
  listen_address: ":8081"
  market_data_grpc_target: "localhost:50051"
  admin_grpc_target: "localhost:50052"
  ws_ping_period: "30s"
  ws_write_wait: "10s"
  ws_max_message_size: 1024
"#;
        let result = load_config_from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("stream_base_url"), "error: {}", err);
    }

    #[test]
    fn okx_enabled_without_url_fails() {
        let yaml = r#"
log_level: "info"
grpc_server:
  listen_address: ":50051"
  enabled: true
  read_timeout: "10s"
  write_timeout: "10s"
ws_server:
  listen_address: ":8080"
  enabled: true
  read_timeout: "60s"
  write_timeout: "10s"
  path: "/ws"
connectors:
  binance:
    enabled: false
    stream_base_url: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "3m"
  okx:
    enabled: true
    stream_base_url_public: ""
    stream_base_url_business: ""
    rest_base_url: ""
    subscribe_ticks: []
    subscribe_klines: {}
    reconnect_delay: "5s"
    ping_interval: "25s"
processor:
  tick_channel_buffer: 1000
  kline_channel_buffer: 1000
admin_server:
  listen_address: ":50052"
api_gateway:
  enabled: true
  listen_address: ":8081"
  market_data_grpc_target: "localhost:50051"
  admin_grpc_target: "localhost:50052"
  ws_ping_period: "30s"
  ws_write_wait: "10s"
  ws_max_message_size: 1024
"#;
        let result = load_config_from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("stream_base_url_public"), "error: {}", err);
    }

    #[test]
    fn field_order_matches_go() {
        let cfg = load_config("../../config.yaml").unwrap();
        assert!(!cfg.log_level.is_empty());
    }
}
