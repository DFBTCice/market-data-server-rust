use async_trait::async_trait;
use md_domain::types::{Kline, Tick};
use serde::Deserialize;
use std::time::Duration;

use crate::{DataEvent, DataType, ExchangeAdapter, ParseError, SubscribeErrorInfo, SubscriptionTarget};

/// OKX 连接器配置
#[derive(Debug, Clone)]
pub struct OkxConnectorConfig {
    pub stream_base_url_public: String,
    pub stream_base_url_business: String,
    pub subscribe_ticks: Vec<String>,
    pub subscribe_klines: std::collections::HashMap<String, Vec<String>>,
    pub reconnect_delay: Duration,
    pub ping_interval: Duration,
}

/// OKX 连接模式 -- 对标 Go 版 public/business 双连接
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OkxStreamMode {
    /// Public WebSocket：处理 trades（tick 数据）
    Public,
    /// Business WebSocket：处理 candle（kline 数据）
    Business,
}

/// OKX 适配器
pub struct OkxAdapter {
    config: OkxConnectorConfig,
    mode: OkxStreamMode,
}

impl OkxAdapter {
    pub fn new(config: OkxConnectorConfig) -> Self {
        Self { config, mode: OkxStreamMode::Public }
    }

    /// 创建指定模式的 OKX 适配器
    pub fn with_mode(config: OkxConnectorConfig, mode: OkxStreamMode) -> Self {
        Self { config, mode }
    }
}

// ---- OKX JSON 消息结构 ----

#[derive(Debug, Deserialize)]
struct OkxWsMessage {
    arg: Option<OkxArg>,
    data: Option<serde_json::Value>,
    event: Option<String>,
    #[allow(dead_code)]
    code: Option<String>,
    #[allow(dead_code)]
    msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OkxArg {
    channel: String,
    #[serde(rename = "instId")]
    inst_id: String,
}

// OKX trades 数据
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OkxTrade {
    #[serde(rename = "instId")]
    inst_id: String,
    #[serde(rename = "px")]
    price: String,
    #[serde(rename = "sz")]
    size: String,
    #[serde(rename = "ts")]
    timestamp: String,
    #[serde(rename = "tradeId")]
    trade_id: String,
    #[serde(rename = "side")]
    side: String,
}

/// 将 OKX symbol 归一化（统一使用 md_domain::topic::normalize_symbol）
fn normalize_okx_symbol(symbol: &str) -> String {
    md_domain::topic::normalize_symbol(symbol)
}

/// 解析 OKX trades 消息为 Tick
pub fn parse_trade(data: &serde_json::Value, inst_id: &str) -> Result<Tick, ParseError> {
    let trade: OkxTrade = serde_json::from_value(data.clone())
        .map_err(|e| ParseError::InvalidJson(e.to_string()))?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let ts: i64 = trade.timestamp.parse().unwrap_or_else(|e| {
        tracing::warn!("OKX trade timestamp parse failed: '{}', using now_ms. error={}", trade.timestamp, e);
        now_ms
    });

    Ok(Tick {
        exchange: "okx".to_string(),
        symbol: normalize_okx_symbol(inst_id),
        timestamp: ts,
        price: trade.price,
        quantity: trade.size,
        trade_id: trade.trade_id.parse().unwrap_or(0),
        // OKX side: "sell" = 主动卖方 = 买方挂单成交 → is_buyer_maker=true
        // OKX side: "buy"  = 主动买方 = 卖方挂单成交 → is_buyer_maker=false
        // 与 Binance 语义一致：is_buyer_maker 表示买方是否为 maker
        is_buyer_maker: trade.side == "sell",
        best_bid_price: String::new(),
        best_bid_quantity: String::new(),
        best_ask_price: String::new(),
        best_ask_quantity: String::new(),
        exchange_event_ts: ts,
        connector_receive_ts: now_ms,
    })
}

/// OKX interval 转毫秒（对标 Go 版 intervalToMs）
/// OKX 使用大写时间单位：1m, 5m, 1H, 4H, 1D, 1W
fn interval_to_ms(interval: &str) -> i64 {
    match interval {
        "1m" => 60_000,
        "3m" => 180_000,
        "5m" => 300_000,
        "15m" => 900_000,
        "30m" => 1_800_000,
        "1H" | "1h" => 3_600_000,
        "2H" | "2h" => 7_200_000,
        "4H" | "4h" => 14_400_000,
        "6H" | "6h" => 21_600_000,
        "12H" | "12h" => 43_200_000,
        "1D" | "1d" => 86_400_000,
        "1W" | "1w" => 604_800_000,
        _ => 60_000, // fallback to 1m
    }
}

/// 解析 OKX candle 消息为 Kline
/// candle 数据格式: [ts, o, h, l, c, vol, volCcy, volCcyQuote, confirm]
pub fn parse_candle(data: &serde_json::Value, inst_id: &str, channel: &str) -> Result<Kline, ParseError> {
    let arr = data.as_array()
        .ok_or_else(|| ParseError::InvalidJson("candle data is not an array".into()))?;

    if arr.len() < 9 {
        return Err(ParseError::InvalidJson(format!("candle array too short: {}", arr.len())));
    }

    let ts_str = arr[0].as_str().unwrap_or("0");
    let open = arr[1].as_str().unwrap_or("0").to_string();
    let high = arr[2].as_str().unwrap_or("0").to_string();
    let low = arr[3].as_str().unwrap_or("0").to_string();
    let close = arr[4].as_str().unwrap_or("0").to_string();
    let vol = arr[5].as_str().unwrap_or("0").to_string();
    let vol_ccy = arr[6].as_str().unwrap_or("0").to_string();
    let confirm = arr[8].as_str().unwrap_or("0");

    let ts: i64 = ts_str.parse().unwrap_or_else(|e| {
        tracing::warn!("OKX candle timestamp parse failed: '{}', defaulting to 0. error={}", ts_str, e);
        0
    });

    // 从 channel 推断 interval: "candle1m" -> "1m", "candle5m" -> "5m"
    let interval = channel.strip_prefix("candle").unwrap_or("1m").to_string();

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    Ok(Kline {
        exchange: "okx".to_string(),
        symbol: normalize_okx_symbol(inst_id),
        interval: interval.clone(),
        open_time: ts,
        open,
        high,
        low,
        close,
        volume: vol,
        close_time: ts + interval_to_ms(&interval) - 1, // 按 interval 计算 close_time
        quote_asset_volume: vol_ccy,
        number_of_trades: 0, // OKX candle 不提供成交笔数
        is_closed: confirm == "1",
        exchange_event_ts: ts,
        connector_receive_ts: now_ms,
    })
}

/// 将 SubscriptionTarget 映射为 OKX channel + instId
/// TICK -> channel: "trades", instId: "BTC-USDT-SWAP"
/// KLINE -> channel: "candle1m", instId: "BTC-USDT-SWAP"
pub fn target_to_streams(target: &SubscriptionTarget) -> Vec<String> {
    let sym = &target.symbol; // OKX symbol 保持原样（大小写敏感）
    match target.data_type {
        DataType::Tick => vec![format!("trades:{}", sym)],
        DataType::Kline => {
            let interval = target.kline_interval.as_deref().unwrap_or("1m");
            vec![format!("candle{}:{}", interval, sym)]
        }
    }
}

/// 构建 OKX WebSocket 订阅消息
pub fn build_subscribe_msg(channels: &[String]) -> String {
    let args: Vec<serde_json::Value> = channels
        .iter()
        .map(|ch| {
            let parts: Vec<&str> = ch.splitn(2, ':').collect();
            if parts.len() == 2 {
                serde_json::json!({
                    "channel": parts[0],
                    "instId": parts[1]
                })
            } else {
                serde_json::json!({
                    "channel": ch,
                    "instId": ""
                })
            }
        })
        .collect();

    serde_json::json!({
        "op": "subscribe",
        "args": args
    })
    .to_string()
}

/// 构建 OKX WebSocket 取消订阅消息
pub fn build_unsubscribe_msg(channels: &[String]) -> String {
    let args: Vec<serde_json::Value> = channels
        .iter()
        .map(|ch| {
            let parts: Vec<&str> = ch.splitn(2, ':').collect();
            if parts.len() == 2 {
                serde_json::json!({
                    "channel": parts[0],
                    "instId": parts[1]
                })
            } else {
                serde_json::json!({
                    "channel": ch,
                    "instId": ""
                })
            }
        })
        .collect();

    serde_json::json!({
        "op": "unsubscribe",
        "args": args
    })
    .to_string()
}

#[async_trait]
impl ExchangeAdapter for OkxAdapter {
    fn name(&self) -> &str {
        "okx"
    }

    fn ws_url(&self) -> &str {
        match self.mode {
            OkxStreamMode::Public => &self.config.stream_base_url_public,
            OkxStreamMode::Business => &self.config.stream_base_url_business,
        }
    }

    fn build_subscribe_msg(&self, streams: &[String]) -> String {
        build_subscribe_msg(streams)
    }

    fn build_unsubscribe_msg(&self, streams: &[String]) -> String {
        build_unsubscribe_msg(streams)
    }

    fn parse_message(&self, raw: &[u8]) -> Result<Vec<DataEvent>, ParseError> {
        let msg: OkxWsMessage = serde_json::from_slice(raw)
            .map_err(|e| ParseError::InvalidJson(e.to_string()))?;

        // 处理事件消息（subscribe/unsubscribe 确认、error 等）
        if let Some(ref event) = msg.event {
            match event.as_str() {
                "error" => {
                    // OKX 错误事件：提取 stream 信息并返回 SubscribeError
                    let code = msg.code.clone().unwrap_or_default();
                    let message = msg.msg.clone().unwrap_or_default();
                    let stream = if let Some(ref arg) = msg.arg {
                        format!("{}:{}", arg.channel, arg.inst_id)
                    } else {
                        String::new()
                    };
                    tracing::warn!("OKX subscribe error: code={}, msg={}, stream={}", code, message, stream);
                    // 只有特定错误码需要移除 stream（60018=channel not found, 60012=instId not found）
                    if code == "60018" || code == "60012" {
                        return Ok(vec![DataEvent::SubscribeError(SubscribeErrorInfo {
                            code,
                            message,
                            stream,
                        })]);
                    }
                }
                "subscribe" | "unsubscribe" => {
                    // 订阅确认：debug 日志
                    tracing::debug!("OKX {} event: {:?}", event, msg.arg);
                }
                _ => {
                    tracing::debug!("ignoring OKX event message: {:?}", msg.event);
                }
            }
            return Ok(Vec::new());
        }

        let arg = match &msg.arg {
            Some(a) => a,
            None => {
                tracing::debug!("ignoring OKX message without arg");
                return Ok(Vec::new());
            }
        };

        let data = match &msg.data {
            Some(d) => d,
            None => {
                tracing::debug!("ignoring OKX message without data");
                return Ok(Vec::new());
            }
        };

        let mut events = Vec::new();

        if arg.channel == "trades" {
            // trades 数据是数组，每条是一个 trade
            if let Some(arr) = data.as_array() {
                for item in arr {
                    match parse_trade(item, &arg.inst_id) {
                        Ok(tick) => events.push(DataEvent::Tick(tick)),
                        Err(e) => {
                            tracing::warn!("failed to parse OKX trade: {}", e);
                        }
                    }
                }
            }
        } else if arg.channel.starts_with("candle") {
            // candle 数据是数组，每条是一个 candle 数组
            if let Some(arr) = data.as_array() {
                for item in arr {
                    match parse_candle(item, &arg.inst_id, &arg.channel) {
                        Ok(kline) => events.push(DataEvent::Kline(kline)),
                        Err(e) => {
                            tracing::warn!("failed to parse OKX candle: {}", e);
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    fn target_to_streams(&self, target: &SubscriptionTarget) -> Vec<String> {
        target_to_streams(target)
    }

    fn heartbeat_message(&self) -> Option<String> {
        Some("ping".to_string())
    }

    fn ping_interval(&self) -> Duration {
        self.config.ping_interval
    }

    /// OKX pong 超时：60 秒（对标 Go 版）
    fn pong_timeout(&self) -> Duration {
        Duration::from_secs(60)
    }

    /// OKX 优雅关停超时：20 秒（对标 Go 版）
    fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(20)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tick_target(symbol: &str) -> SubscriptionTarget {
        SubscriptionTarget {
            exchange: "okx".into(),
            data_type: DataType::Tick,
            symbol: symbol.into(),
            kline_interval: None,
        }
    }

    fn make_kline_target(symbol: &str, interval: &str) -> SubscriptionTarget {
        SubscriptionTarget {
            exchange: "okx".into(),
            data_type: DataType::Kline,
            symbol: symbol.into(),
            kline_interval: Some(interval.into()),
        }
    }

    // ---- target_to_streams ----

    #[test]
    fn target_to_streams_tick() {
        let t = make_tick_target("BTC-USDT-SWAP");
        let streams = target_to_streams(&t);
        assert_eq!(streams, vec!["trades:BTC-USDT-SWAP"]);
    }

    #[test]
    fn target_to_streams_kline() {
        let t = make_kline_target("BTC-USDT-SWAP", "1m");
        let streams = target_to_streams(&t);
        assert_eq!(streams, vec!["candle1m:BTC-USDT-SWAP"]);
    }

    #[test]
    fn target_to_streams_kline_5m() {
        let t = make_kline_target("ETH-USDT-SWAP", "5m");
        let streams = target_to_streams(&t);
        assert_eq!(streams, vec!["candle5m:ETH-USDT-SWAP"]);
    }

    // ---- build messages ----

    #[test]
    fn build_subscribe_message() {
        let msg = build_subscribe_msg(&["trades:BTC-USDT-SWAP".into()]);
        let val: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(val["op"], "subscribe");
        assert_eq!(val["args"][0]["channel"], "trades");
        assert_eq!(val["args"][0]["instId"], "BTC-USDT-SWAP");
    }

    #[test]
    fn build_unsubscribe_message() {
        let msg = build_unsubscribe_msg(&["candle1m:BTC-USDT-SWAP".into()]);
        let val: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(val["op"], "unsubscribe");
        assert_eq!(val["args"][0]["channel"], "candle1m");
    }

    // ---- parse_trade ----

    #[test]
    fn parse_trade_to_tick() {
        let json = serde_json::json!({
            "instId": "BTC-USDT-SWAP",
            "px": "67000.50",
            "sz": "0.1",
            "ts": "1711929600000",
            "tradeId": "12345",
            "side": "sell"
        });

        let tick = parse_trade(&json, "BTC-USDT-SWAP").unwrap();
        assert_eq!(tick.exchange, "okx");
        assert_eq!(tick.symbol, "BTC-USDT-SWAP");
        assert_eq!(tick.price, "67000.50");
        assert_eq!(tick.quantity, "0.1");
        assert_eq!(tick.trade_id, 12345);
        assert!(tick.is_buyer_maker); // sell side = buyer maker
    }

    #[test]
    fn parse_trade_buy_side() {
        let json = serde_json::json!({
            "instId": "ETH-USDT-SWAP",
            "px": "3500.00",
            "sz": "1.0",
            "ts": "1711929600000",
            "tradeId": "99999",
            "side": "buy"
        });

        let tick = parse_trade(&json, "ETH-USDT-SWAP").unwrap();
        assert!(!tick.is_buyer_maker); // buy side = not buyer maker
    }

    // ---- parse_candle ----

    #[test]
    fn parse_candle_to_kline() {
        let json = serde_json::json!(["1711929600000", "67000.00", "67100.00", "66900.00", "67050.00", "100.5", "6730000.0", "6730000.0", "1"]);

        let kline = parse_candle(&json, "BTC-USDT-SWAP", "candle1m").unwrap();
        assert_eq!(kline.exchange, "okx");
        assert_eq!(kline.symbol, "BTC-USDT-SWAP");
        assert_eq!(kline.interval, "1m");
        assert_eq!(kline.open, "67000.00");
        assert_eq!(kline.close, "67050.00");
        assert!(kline.is_closed);
    }

    #[test]
    fn parse_candle_not_closed() {
        let json = serde_json::json!(["1711929600000", "67000.00", "67100.00", "66900.00", "67050.00", "100.5", "6730000.0", "6730000.0", "0"]);

        let kline = parse_candle(&json, "BTC-USDT-SWAP", "candle5m").unwrap();
        assert_eq!(kline.interval, "5m");
        assert!(!kline.is_closed);
    }

    // ---- adapter parse_message ----

    #[test]
    fn adapter_parse_trades_message() {
        let adapter = OkxAdapter::new(OkxConnectorConfig {
            stream_base_url_public: "wss://test".into(),
            stream_base_url_business: "wss://test".into(),
            subscribe_ticks: vec![],
            subscribe_klines: Default::default(),
            reconnect_delay: Duration::from_secs(10),
            ping_interval: Duration::from_secs(25),
        });

        let raw = serde_json::json!({
            "arg": { "channel": "trades", "instId": "BTC-USDT-SWAP" },
            "data": [
                { "instId": "BTC-USDT-SWAP", "px": "67000.50", "sz": "0.1", "ts": "1711929600000", "tradeId": "12345", "side": "sell" }
            ]
        });

        let events = adapter.parse_message(raw.to_string().as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Tick(t) => {
                assert_eq!(t.symbol, "BTC-USDT-SWAP");
                assert_eq!(t.price, "67000.50");
            }
            _ => panic!("expected Tick"),
        }
    }

    #[test]
    fn adapter_parse_candle_message() {
        let adapter = OkxAdapter::new(OkxConnectorConfig {
            stream_base_url_public: "wss://test".into(),
            stream_base_url_business: "wss://test".into(),
            subscribe_ticks: vec![],
            subscribe_klines: Default::default(),
            reconnect_delay: Duration::from_secs(10),
            ping_interval: Duration::from_secs(25),
        });

        let raw = serde_json::json!({
            "arg": { "channel": "candle1m", "instId": "BTC-USDT-SWAP" },
            "data": [
                ["1711929600000", "67000.00", "67100.00", "66900.00", "67050.00", "100.5", "6730000.0", "6730000.0", "1"]
            ]
        });

        let events = adapter.parse_message(raw.to_string().as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Kline(k) => {
                assert_eq!(k.symbol, "BTC-USDT-SWAP");
                assert_eq!(k.interval, "1m");
            }
            _ => panic!("expected Kline"),
        }
    }

    #[test]
    fn adapter_ignores_event_message() {
        let adapter = OkxAdapter::new(OkxConnectorConfig {
            stream_base_url_public: "wss://test".into(),
            stream_base_url_business: "wss://test".into(),
            subscribe_ticks: vec![],
            subscribe_klines: Default::default(),
            reconnect_delay: Duration::from_secs(10),
            ping_interval: Duration::from_secs(25),
        });

        let raw = serde_json::json!({
            "event": "subscribe",
            "arg": { "channel": "trades", "instId": "BTC-USDT-SWAP" },
            "code": "0"
        });

        let events = adapter.parse_message(raw.to_string().as_bytes()).unwrap();
        assert_eq!(events.len(), 0);
    }

    // ---- heartbeat ----

    #[test]
    fn heartbeat_is_ping() {
        let adapter = OkxAdapter::new(OkxConnectorConfig {
            stream_base_url_public: "wss://test".into(),
            stream_base_url_business: "wss://test".into(),
            subscribe_ticks: vec![],
            subscribe_klines: Default::default(),
            reconnect_delay: Duration::from_secs(10),
            ping_interval: Duration::from_secs(25),
        });

        assert_eq!(adapter.heartbeat_message(), Some("ping".to_string()));
    }
}
