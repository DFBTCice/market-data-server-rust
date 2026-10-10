use serde::{Deserialize, Serialize};

use crate::serde_helpers;

/// Tick 数据 -- 对标 Go 版 `marketdata.Tick` protobuf struct
///
/// Go struct tags（来自 marketdata.pb.go）:
/// ```text
/// Exchange           string  `json:"exchange,omitempty"`
/// Symbol             string  `json:"symbol,omitempty"`
/// Timestamp          int64   `json:"timestamp,omitempty"`
/// Price              string  `json:"price,omitempty"`
/// Quantity           string  `json:"quantity,omitempty"`
/// TradeId            int64   `json:"trade_id,omitempty"`
/// IsBuyerMaker       bool    `json:"is_buyer_maker,omitempty"`
/// BestBidPrice       string  `json:"best_bid_price,omitempty"`
/// BestBidQuantity    string  `json:"best_bid_quantity,omitempty"`
/// BestAskPrice       string  `json:"best_ask_price,omitempty"`
/// BestAskQuantity    string  `json:"best_ask_quantity,omitempty"`
/// ExchangeEventTs    int64   `json:"exchange_event_ts,omitempty"`
/// ConnectorReceiveTs int64   `json:"connector_receive_ts,omitempty"`
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Tick {
    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub exchange: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub symbol: String,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub timestamp: i64,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub price: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub quantity: String,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub trade_id: i64,

    #[serde(skip_serializing_if = "serde_helpers::is_false")]
    pub is_buyer_maker: bool,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub best_bid_price: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub best_bid_quantity: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub best_ask_price: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub best_ask_quantity: String,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub exchange_event_ts: i64,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub connector_receive_ts: i64,

    /// 交易所成交撮合时间 T（毫秒）。仅内部用于"端到端延迟(recv-T)"指标：
    /// 币安 aggTrade 的 T 早于事件推送时间 E（E-T 是币安内部聚合/分发延迟）；
    /// OKX 无独立推送时间，置为与 ts 相同。
    /// `#[serde(skip)]`：不参与 JSON/proto 序列化，保持与 Go 线格式字节级一致。
    #[serde(skip)]
    pub exchange_trade_ts: i64,
}

/// Kline 数据 -- 对标 Go 版 `marketdata.Kline` protobuf struct
///
/// Go struct tags:
/// ```text
/// Exchange           string  `json:"exchange,omitempty"`
/// Symbol             string  `json:"symbol,omitempty"`
/// Interval           string  `json:"interval,omitempty"`
/// OpenTime           int64   `json:"open_time,omitempty"`
/// Open               string  `json:"open,omitempty"`
/// High               string  `json:"high,omitempty"`
/// Low                string  `json:"low,omitempty"`
/// Close              string  `json:"close,omitempty"`
/// Volume             string  `json:"volume,omitempty"`
/// CloseTime          int64   `json:"close_time,omitempty"`
/// QuoteAssetVolume   string  `json:"quote_asset_volume,omitempty"`
/// NumberOfTrades     int64   `json:"number_of_trades,omitempty"`
/// IsClosed           bool    `json:"is_closed,omitempty"`
/// ExchangeEventTs    int64   `json:"exchange_event_ts,omitempty"`
/// ConnectorReceiveTs int64   `json:"connector_receive_ts,omitempty"`
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[derive(Default)]
pub struct Kline {
    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub exchange: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub symbol: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub interval: String,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub open_time: i64,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub open: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub high: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub low: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub close: String,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub volume: String,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub close_time: i64,

    #[serde(skip_serializing_if = "serde_helpers::is_empty_string")]
    pub quote_asset_volume: String,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub number_of_trades: i64,

    #[serde(skip_serializing_if = "serde_helpers::is_false")]
    pub is_closed: bool,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub exchange_event_ts: i64,

    #[serde(skip_serializing_if = "serde_helpers::is_zero_i64")]
    pub connector_receive_ts: i64,
}

/// Quote 报价数据 -- 1.1.0 新增主题 `quote.<exchange>.<SYMBOL>`
///
/// 买一卖一 + 最新价，用于让消费端区分"价格没变"与"数据断了"（tick 只由成交驱动，
/// 成交稀疏的合约两笔成交之间可能十几秒无推送）。
///
/// JSON 约定（与 tick 风格一致，但**不使用 omitempty**）：
/// - 数值一律字符串，时间戳为毫秒整数；
/// - 字段顺序固定：exchange, symbol, bid_price, bid_qty, ask_price, ask_qty,
///   last_price, timestamp, connector_receive_ts；
/// - 币安 bookTicker 不含最新成交价，`last_price` 恒为空串 `""` 且仍然输出
///   （消费端据此区分"无 last"与"字段缺失"）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Quote {
    pub exchange: String,
    pub symbol: String,
    pub bid_price: String,
    pub bid_qty: String,
    pub ask_price: String,
    pub ask_qty: String,
    /// 最新成交价；币安 bookTicker 无此字段时为空串，OKX 取 tickers.last
    pub last_price: String,
    /// 交易所事件时间（毫秒）：币安 bookTicker 的 E、OKX tickers 的 ts
    pub timestamp: i64,
    /// 连接器接收时间（毫秒）
    pub connector_receive_ts: i64,
}

/// WebSocket 推送消息 -- API Gateway 格式
/// {"type": "tick", "topic": "tick.binance.BTCUSDT", "data": {...}}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsGatewayMessage<T> {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub topic: String,
    pub data: T,
}

/// WebSocket 推送消息 -- Legacy 格式
/// {"topic": "tick.binance.BTCUSDT", "data": {...}}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsLegacyMessage<T> {
    pub topic: String,
    pub data: T,
}

/// REST 错误响应
/// {"error": "message"}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// 默认 Tick（所有字段为零值）
impl Default for Tick {
    fn default() -> Self {
        Self {
            exchange: String::new(),
            symbol: String::new(),
            timestamp: 0,
            price: String::new(),
            quantity: String::new(),
            trade_id: 0,
            is_buyer_maker: false,
            best_bid_price: String::new(),
            best_bid_quantity: String::new(),
            best_ask_price: String::new(),
            best_ask_quantity: String::new(),
            exchange_event_ts: 0,
            connector_receive_ts: 0,
            exchange_trade_ts: 0,
        }
    }
}
