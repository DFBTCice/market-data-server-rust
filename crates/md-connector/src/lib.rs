pub mod base;
pub mod binance;
pub mod okx;

use async_trait::async_trait;
use md_domain::types::{Kline, Quote, Tick};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// 数据事件 -- 从交易所解析出的 Tick/Kline/Quote
#[derive(Debug, Clone)]
pub enum DataEvent {
    Tick(Tick),
    Kline(Kline),
    /// 买一卖一报价（1.1.0 新增；仅 quote 专用连接器会产生）
    Quote(Quote),
    /// 订阅错误事件 -- 交易所返回的订阅失败响应
    SubscribeError(SubscribeErrorInfo),
}

/// 订阅错误信息
#[derive(Debug, Clone)]
pub struct SubscribeErrorInfo {
    /// 错误码
    pub code: String,
    /// 错误消息
    pub message: String,
    /// 失败的 stream（channel:instId 格式）
    pub stream: String,
}

/// 订阅目标 -- 对标 Go 版 common.SubscriptionTarget
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubscriptionTarget {
    pub exchange: String,
    pub data_type: DataType,
    pub symbol: String,
    pub kline_interval: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    Tick,
    Kline,
    /// 买一卖一报价（1.1.0 新增）
    Quote,
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Tick => write!(f, "TICK"),
            DataType::Kline => write!(f, "KLINE"),
            DataType::Quote => write!(f, "QUOTE"),
        }
    }
}

/// 订阅管理（REST / gRPC Admin）的连接器路由。`exchange_key` 须已小写。
///
/// - TICK  → `"{ex}"`
/// - KLINE → `"{ex}-kline"`，缺失时回退 `"{ex}"`（OKX 双连接；Binance 单连接）—— 与 1.0.0 一致
/// - QUOTE → `"{ex}-quote"`，**严格匹配、不回退**：quote 必须走专用 Public 连接
///   （币安 bookTicker 在 market 连接上订阅成功但不推送），回退到 tick 连接会静默拿不到数据。
///   开关关闭时该连接器不存在，返回 None。
pub fn find_connector<'a>(
    connectors: &'a HashMap<String, Arc<dyn Connector>>,
    exchange_key: &str,
    data_type: DataType,
) -> Option<&'a Arc<dyn Connector>> {
    match data_type {
        DataType::Tick => connectors.get(exchange_key),
        DataType::Kline => connectors
            .get(&format!("{}-kline", exchange_key))
            .or_else(|| connectors.get(exchange_key)),
        DataType::Quote => connectors.get(&format!("{}-quote", exchange_key)),
    }
}

/// WebSocket 写命令 -- 用于写多路复用器
#[derive(Debug)]
pub enum WsCommand {
    Text(String),
    Ping(Vec<u8>),
    Close,
}

/// 连接器公共 trait -- 对标 Go 版 common.Connector
#[async_trait]
pub trait Connector: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self) -> Result<(), ConnectorError>;
    async fn stop(&self) -> Result<(), ConnectorError>;
    async fn add_subscriptions(
        &self,
        targets: Vec<SubscriptionTarget>,
    ) -> Result<(), ConnectorError>;
    async fn remove_subscriptions(
        &self,
        targets: Vec<SubscriptionTarget>,
    ) -> Result<(), ConnectorError>;
    fn current_subscriptions(&self) -> Vec<SubscriptionTarget>;
}

/// 交易所适配器 trait -- 定义交易所特定逻辑
#[async_trait]
pub trait ExchangeAdapter: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn ws_url(&self) -> &str;
    fn build_subscribe_msg(&self, streams: &[String]) -> String;
    fn build_unsubscribe_msg(&self, streams: &[String]) -> String;
    fn parse_message(&self, raw: &[u8]) -> Result<Vec<DataEvent>, ParseError>;
    fn target_to_streams(&self, target: &SubscriptionTarget) -> Vec<String>;
    fn heartbeat_message(&self) -> Option<String>;
    fn ping_interval(&self) -> Duration;

    /// Pong 超时 -- 超过此时间未收到 pong 则认为连接已死
    /// Binance: 6 分钟, OKX: 60 秒
    fn pong_timeout(&self) -> Duration {
        self.ping_interval() * 2
    }

    /// 优雅关停超时
    /// Binance: 15 秒, OKX: 20 秒
    fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(15)
    }
}

#[derive(Debug)]
pub enum ConnectorError {
    ConnectionFailed(String),
    SubscribeFailed(String),
    Stopped,
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectorError::ConnectionFailed(s) => write!(f, "connection failed: {}", s),
            ConnectorError::SubscribeFailed(s) => write!(f, "subscribe failed: {}", s),
            ConnectorError::Stopped => write!(f, "connector stopped"),
        }
    }
}

impl std::error::Error for ConnectorError {}

#[derive(Debug)]
pub enum ParseError {
    InvalidJson(String),
    UnknownMessageType(String),
    MissingField(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidJson(s) => write!(f, "invalid JSON: {}", s),
            ParseError::UnknownMessageType(s) => write!(f, "unknown message type: {}", s),
            ParseError::MissingField(s) => write!(f, "missing field: {}", s),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod routing_tests {
    use super::*;

    struct Dummy(&'static str);

    #[async_trait]
    impl Connector for Dummy {
        fn name(&self) -> &str {
            self.0
        }
        async fn start(&self) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn stop(&self) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn add_subscriptions(
            &self,
            _: Vec<SubscriptionTarget>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn remove_subscriptions(
            &self,
            _: Vec<SubscriptionTarget>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        fn current_subscriptions(&self) -> Vec<SubscriptionTarget> {
            Vec::new()
        }
    }

    fn map(keys: &[&'static str]) -> HashMap<String, Arc<dyn Connector>> {
        keys.iter()
            .map(|k| (k.to_string(), Arc::new(Dummy(k)) as Arc<dyn Connector>))
            .collect()
    }

    /// 1.0.0 路由：TICK→ex；KLINE→ex-kline，缺失回退 ex
    #[test]
    fn tick_and_kline_routing_unchanged() {
        let m = map(&["binance", "okx", "okx-kline"]);
        assert_eq!(
            find_connector(&m, "binance", DataType::Tick)
                .unwrap()
                .name(),
            "binance"
        );
        assert_eq!(
            find_connector(&m, "okx", DataType::Tick).unwrap().name(),
            "okx"
        );
        assert_eq!(
            find_connector(&m, "okx", DataType::Kline).unwrap().name(),
            "okx-kline"
        );
        // binance 无 -kline 连接器 → 回退到 binance
        assert_eq!(
            find_connector(&m, "binance", DataType::Kline)
                .unwrap()
                .name(),
            "binance"
        );
        assert!(find_connector(&m, "bybit", DataType::Tick).is_none());
    }

    #[test]
    fn quote_routing_is_strict_no_fallback() {
        // 开关关闭：没有 -quote 连接器，绝不能回退到 tick 连接器
        let m = map(&["binance", "okx", "okx-kline"]);
        assert!(find_connector(&m, "binance", DataType::Quote).is_none());
        assert!(find_connector(&m, "okx", DataType::Quote).is_none());
        // 开关开启
        let m = map(&["binance", "binance-quote", "okx", "okx-quote"]);
        assert_eq!(
            find_connector(&m, "binance", DataType::Quote)
                .unwrap()
                .name(),
            "binance-quote"
        );
        assert_eq!(
            find_connector(&m, "okx", DataType::Quote).unwrap().name(),
            "okx-quote"
        );
        // 开启 quote 后 tick 路由不受影响
        assert_eq!(
            find_connector(&m, "binance", DataType::Tick)
                .unwrap()
                .name(),
            "binance"
        );
    }
}
