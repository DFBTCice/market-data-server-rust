pub mod base;
pub mod binance;
pub mod okx;

use async_trait::async_trait;
use md_domain::types::{Tick, Kline};
use std::fmt;
use std::time::Duration;

/// 数据事件 -- 从交易所解析出的 Tick/Kline
#[derive(Debug, Clone)]
pub enum DataEvent {
    Tick(Tick),
    Kline(Kline),
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
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Tick => write!(f, "TICK"),
            DataType::Kline => write!(f, "KLINE"),
        }
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
    async fn add_subscriptions(&self, targets: Vec<SubscriptionTarget>) -> Result<(), ConnectorError>;
    async fn remove_subscriptions(&self, targets: Vec<SubscriptionTarget>) -> Result<(), ConnectorError>;
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
