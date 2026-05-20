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
        }
    }
}

impl Default for Kline {
    fn default() -> Self {
        Self {
            exchange: String::new(),
            symbol: String::new(),
            interval: String::new(),
            open_time: 0,
            open: String::new(),
            high: String::new(),
            low: String::new(),
            close: String::new(),
            volume: String::new(),
            close_time: 0,
            quote_asset_volume: String::new(),
            number_of_trades: 0,
            is_closed: false,
            exchange_event_ts: 0,
            connector_receive_ts: 0,
        }
    }
}
