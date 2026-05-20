use async_trait::async_trait;
use md_domain::topic::normalize_symbol;
use md_domain::types::{Kline, Tick};
use serde::Deserialize;
use std::time::Duration;

use crate::{DataEvent, DataType, ExchangeAdapter, ParseError, SubscriptionTarget};

/// Binance 连接器配置
#[derive(Debug, Clone)]
pub struct BinanceConnectorConfig {
    pub stream_base_url: String,
    pub subscribe_ticks: Vec<String>,
    pub subscribe_klines: std::collections::HashMap<String, Vec<String>>,
    pub reconnect_delay: Duration,
    pub ping_interval: Duration,
}

/// Binance 适配器
pub struct BinanceAdapter {
    config: BinanceConnectorConfig,
}

impl BinanceAdapter {
    pub fn new(config: BinanceConnectorConfig) -> Self {
        Self { config }
    }
}

// ---- Binance JSON 消息结构 ----

#[derive(Debug, Deserialize)]
struct BinanceStreamMessage {
    stream: String,
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AggTradeData {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    trade_time: i64,
    #[serde(rename = "a")]
    trade_id: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[derive(Debug, Deserialize)]
struct KlineData {
    #[serde(rename = "t")]
    open_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "T")]
    close_time: i64,
    #[serde(rename = "q")]
    quote_asset_volume: String,
    #[serde(rename = "n")]
    number_of_trades: i64,
    #[serde(rename = "x")]
    is_closed: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct KlineWrapper {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "E")]
    event_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "k")]
    kline: KlineData,
}

// ---- 公共解析函数（可独立测试）----

/// 解析 Binance aggTrade 消息为 Tick
pub fn parse_aggtrade(json: &serde_json::Value) -> Result<Tick, ParseError> {
    let data: AggTradeData =
        serde_json::from_value(json.clone()).map_err(|e| ParseError::InvalidJson(e.to_string()))?;

    let now_ms = chrono_now_ms();

    Ok(Tick {
        exchange: "binance".to_string(),
        symbol: normalize_symbol(&data.symbol),
        timestamp: data.trade_time,
        price: data.price,
        quantity: data.quantity,
        trade_id: data.trade_id,
        is_buyer_maker: data.is_buyer_maker,
        best_bid_price: String::new(),
        best_bid_quantity: String::new(),
        best_ask_price: String::new(),
        best_ask_quantity: String::new(),
        exchange_event_ts: data.trade_time,
        connector_receive_ts: now_ms,
    })
}

/// 解析 Binance kline 消息为 Kline
pub fn parse_kline(json: &serde_json::Value) -> Result<Kline, ParseError> {
    let wrapper: KlineWrapper =
        serde_json::from_value(json.clone()).map_err(|e| ParseError::InvalidJson(e.to_string()))?;

    let now_ms = chrono_now_ms();
    let k = wrapper.kline;

    Ok(Kline {
        exchange: "binance".to_string(),
        symbol: normalize_symbol(&k.symbol),
        interval: k.interval,
        open_time: k.open_time,
        open: k.open,
        high: k.high,
        low: k.low,
        close: k.close,
        volume: k.volume,
        close_time: k.close_time,
        quote_asset_volume: k.quote_asset_volume,
        number_of_trades: k.number_of_trades,
        is_closed: k.is_closed,
        exchange_event_ts: wrapper.event_time,
        connector_receive_ts: now_ms,
    })
}

/// 将 SubscriptionTarget 映射为 Binance stream key
/// TICK -> ["<symbol>@aggTrade"]
/// KLINE -> ["<symbol>@kline_<interval>"]
pub fn target_to_streams(target: &SubscriptionTarget) -> Vec<String> {
    let sym = target.symbol.to_lowercase();
    match target.data_type {
        DataType::Tick => vec![format!("{}@aggTrade", sym)],
        DataType::Kline => {
            let interval = target.kline_interval.as_deref().unwrap_or("1m");
            vec![format!("{}@kline_{}", sym, interval)]
        }
    }
}

/// 构建 Binance WebSocket 订阅消息
pub fn build_subscribe_msg(streams: &[String]) -> String {
    serde_json::json!({
        "method": "SUBSCRIBE",
        "params": streams,
        "id": 1
    })
    .to_string()
}

/// 构建 Binance WebSocket 取消订阅消息
pub fn build_unsubscribe_msg(streams: &[String]) -> String {
    serde_json::json!({
        "method": "UNSUBSCRIBE",
        "params": streams,
        "id": 1
    })
    .to_string()
}

fn chrono_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[async_trait]
impl ExchangeAdapter for BinanceAdapter {
    fn name(&self) -> &str {
        "binance"
    }

    fn ws_url(&self) -> &str {
        &self.config.stream_base_url
    }

    fn build_subscribe_msg(&self, streams: &[String]) -> String {
        build_subscribe_msg(streams)
    }

    fn build_unsubscribe_msg(&self, streams: &[String]) -> String {
        build_unsubscribe_msg(streams)
    }

    fn parse_message(&self, raw: &[u8]) -> Result<Vec<DataEvent>, ParseError> {
        let msg: BinanceStreamMessage = match serde_json::from_slice(raw) {
            Ok(msg) => msg,
            Err(_) => {
                // Likely a subscription confirmation {"result":null,"id":N} -- silently ignore
                tracing::debug!("ignoring non-stream message ({} bytes)", raw.len());
                return Ok(Vec::new());
            }
        };

        let mut events = Vec::new();

        if msg.stream.contains("@aggTrade") {
            let tick = parse_aggtrade(&msg.data)?;
            events.push(DataEvent::Tick(tick));
        } else if msg.stream.contains("@kline_") {
            let kline = parse_kline(&msg.data)?;
            events.push(DataEvent::Kline(kline));
        } else if msg.stream.ends_with("@bookTicker") || msg.stream.ends_with("@ticker") {
            // bookTicker / ticker -- 暂不处理，跳过
        } else {
            // 订阅确认等消息，忽略
        }

        Ok(events)
    }

    fn target_to_streams(&self, target: &SubscriptionTarget) -> Vec<String> {
        target_to_streams(target)
    }

    fn heartbeat_message(&self) -> Option<String> {
        None // Binance 不需要主动 ping，使用 WebSocket 协议层 ping
    }

    fn ping_interval(&self) -> Duration {
        self.config.ping_interval
    }

    /// Binance pong 超时：90 秒（对标 Go 版，3 倍 ping 间隔）
    fn pong_timeout(&self) -> Duration {
        Duration::from_secs(90)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tick_target(symbol: &str) -> SubscriptionTarget {
        SubscriptionTarget {
            exchange: "binance".into(),
            data_type: DataType::Tick,
            symbol: symbol.into(),
            kline_interval: None,
        }
    }

    fn make_kline_target(symbol: &str, interval: &str) -> SubscriptionTarget {
        SubscriptionTarget {
            exchange: "binance".into(),
            data_type: DataType::Kline,
            symbol: symbol.into(),
            kline_interval: Some(interval.into()),
        }
    }

    // ---- target_to_streams ----

    #[test]
    fn target_to_streams_tick() {
        let t = make_tick_target("BTCUSDT");
        let streams = target_to_streams(&t);
        assert_eq!(streams, vec!["btcusdt@aggTrade"]);
    }

    #[test]
    fn target_to_streams_kline() {
        let t = make_kline_target("BTCUSDT", "1m");
        let streams = target_to_streams(&t);
        assert_eq!(streams, vec!["btcusdt@kline_1m"]);
    }

    #[test]
    fn target_to_streams_kline_5m() {
        let t = make_kline_target("ETHUSDT", "5m");
        let streams = target_to_streams(&t);
        assert_eq!(streams, vec!["ethusdt@kline_5m"]);
    }

    // ---- parse_aggtrade ----

    #[test]
    fn parse_aggtrade_to_tick() {
        let json = serde_json::json!({
            "e": "aggTrade",
            "s": "BTCUSDT",
            "p": "67000.50",
            "q": "0.1",
            "T": 1711929600000i64,
            "a": 12345,
            "m": true
        });

        let tick = parse_aggtrade(&json).unwrap();
        assert_eq!(tick.exchange, "binance");
        assert_eq!(tick.symbol, "BTCUSDT");
        assert_eq!(tick.price, "67000.50");
        assert_eq!(tick.quantity, "0.1");
        assert_eq!(tick.trade_id, 12345);
        assert!(tick.is_buyer_maker);
        assert_eq!(tick.exchange_event_ts, 1711929600000);
        assert!(tick.connector_receive_ts > 0);
    }

    #[test]
    fn parse_aggtrade_symbol_normalized() {
        let json = serde_json::json!({
            "e": "aggTrade",
            "s": "btcusdt",
            "p": "100.0",
            "q": "1.0",
            "T": 1000,
            "a": 1,
            "m": false
        });

        let tick = parse_aggtrade(&json).unwrap();
        assert_eq!(tick.symbol, "BTCUSDT"); // 归一化为大写
    }

    // ---- parse_kline ----

    #[test]
    fn parse_kline_to_kline() {
        let json = serde_json::json!({
            "e": "kline",
            "E": 1711929600000i64,
            "s": "BTCUSDT",
            "k": {
                "t": 1711929600000i64,
                "s": "BTCUSDT",
                "i": "1m",
                "o": "67000.00",
                "h": "67100.00",
                "l": "66900.00",
                "c": "67050.00",
                "v": "100.5",
                "T": 1711929659999i64,
                "q": "6730000.0",
                "n": 500,
                "x": true
            }
        });

        let kline = parse_kline(&json).unwrap();
        assert_eq!(kline.exchange, "binance");
        assert_eq!(kline.symbol, "BTCUSDT");
        assert_eq!(kline.interval, "1m");
        assert_eq!(kline.open, "67000.00");
        assert_eq!(kline.high, "67100.00");
        assert_eq!(kline.close, "67050.00");
        assert_eq!(kline.volume, "100.5");
        assert_eq!(kline.number_of_trades, 500);
        assert!(kline.is_closed);
    }

    // ---- build messages ----

    #[test]
    fn build_subscribe_message() {
        let msg = build_subscribe_msg(&["btcusdt@aggTrade".into()]);
        let val: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(val["method"], "SUBSCRIBE");
        assert_eq!(val["params"][0], "btcusdt@aggTrade");
    }

    #[test]
    fn build_unsubscribe_message() {
        let msg = build_unsubscribe_msg(&["btcusdt@kline_1m".into()]);
        let val: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(val["method"], "UNSUBSCRIBE");
    }

    // ---- adapter parse_message ----

    #[test]
    fn adapter_parse_aggtrade_message() {
        let adapter = BinanceAdapter::new(BinanceConnectorConfig {
            stream_base_url: "wss://test".into(),
            subscribe_ticks: vec![],
            subscribe_klines: Default::default(),
            reconnect_delay: Duration::from_secs(5),
            ping_interval: Duration::from_secs(180),
        });

        let raw = serde_json::json!({
            "stream": "btcusdt@aggTrade",
            "data": {
                "e": "aggTrade",
                "s": "BTCUSDT",
                "p": "67000.50",
                "q": "0.1",
                "T": 1711929600000i64,
                "a": 12345,
                "m": true
            }
        });

        let events = adapter.parse_message(raw.to_string().as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Tick(t) => {
                assert_eq!(t.symbol, "BTCUSDT");
                assert_eq!(t.price, "67000.50");
            }
            _ => panic!("expected Tick"),
        }
    }

    #[test]
    fn adapter_parse_kline_message() {
        let adapter = BinanceAdapter::new(BinanceConnectorConfig {
            stream_base_url: "wss://test".into(),
            subscribe_ticks: vec![],
            subscribe_klines: Default::default(),
            reconnect_delay: Duration::from_secs(5),
            ping_interval: Duration::from_secs(180),
        });

        let raw = serde_json::json!({
            "stream": "btcusdt@kline_1m",
            "data": {
                "e": "kline",
                "E": 1711929600000i64,
                "s": "BTCUSDT",
                "k": {
                    "t": 1711929600000i64,
                    "s": "BTCUSDT",
                    "i": "1m",
                    "o": "67000.00",
                    "h": "67100.00",
                    "l": "66900.00",
                    "c": "67050.00",
                    "v": "100.5",
                    "T": 1711929659999i64,
                    "q": "6730000.0",
                    "n": 500,
                    "x": true
                }
            }
        });

        let events = adapter.parse_message(raw.to_string().as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Kline(k) => {
                assert_eq!(k.symbol, "BTCUSDT");
                assert_eq!(k.interval, "1m");
            }
            _ => panic!("expected Kline"),
        }
    }

    #[test]
    fn adapter_ignores_book_ticker() {
        let adapter = BinanceAdapter::new(BinanceConnectorConfig {
            stream_base_url: "wss://test".into(),
            subscribe_ticks: vec![],
            subscribe_klines: Default::default(),
            reconnect_delay: Duration::from_secs(5),
            ping_interval: Duration::from_secs(180),
        });

        let raw = serde_json::json!({
            "stream": "btcusdt@bookTicker",
            "data": { "u": 123 }
        });

        let events = adapter.parse_message(raw.to_string().as_bytes()).unwrap();
        assert_eq!(events.len(), 0); // 跳过
    }

    // ---- error cases ----

    #[test]
    fn parse_invalid_json_returns_error() {
        let result = parse_aggtrade(&serde_json::json!("not an object"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_field_returns_error() {
        let json = serde_json::json!({
            "e": "aggTrade",
            "s": "BTCUSDT"
            // 缺少 price, quantity 等字段
        });
        let result = parse_aggtrade(&json);
        assert!(result.is_err());
    }
}
