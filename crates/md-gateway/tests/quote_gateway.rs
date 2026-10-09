//! quote 主题的网关集成测试（1.1.0 新增）。
//!
//! 真实启动 axum 服务（REST + `/ws/v1/data` + `/ws`），用真实 WebSocket 客户端验证：
//! - Gateway WS 可订阅 `quote.*`，收到 `{"type":"quote","topic":...,"data":{...}}`；
//! - Legacy `/ws` 永不推送 quote（任务书 §三-3），且 tick 在同一连接上照常；
//! - REST `GET /api/v1/data/latest/quote/{exchange}/{symbol}`；
//! - `POST/DELETE /api/v1/subscriptions` 接受 `QUOTE`，GET 以 `data_type=3` 列出；
//! - 既有 tick/kline 订阅的响应文案与 1.0.0 一致。

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use md_connector::{Connector, ConnectorError, DataType, SubscriptionTarget};
use md_domain::types::{Quote, Tick};
use md_gateway::Gateway;
use md_processor::Processor;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

/// 内存版连接器：只记录订阅，不连交易所
struct MockConnector {
    name: String,
    subs: RwLock<Vec<SubscriptionTarget>>,
}

impl MockConnector {
    fn boxed(name: &str) -> Arc<dyn Connector> {
        Arc::new(Self {
            name: name.into(),
            subs: RwLock::new(Vec::new()),
        })
    }
}

#[async_trait]
impl Connector for MockConnector {
    fn name(&self) -> &str {
        &self.name
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
        self.subs.write().await.extend(targets);
        Ok(())
    }
    async fn remove_subscriptions(
        &self,
        targets: Vec<SubscriptionTarget>,
    ) -> Result<(), ConnectorError> {
        let mut subs = self.subs.write().await;
        for t in targets {
            subs.retain(|s| s != &t);
        }
        Ok(())
    }
    fn current_subscriptions(&self) -> Vec<SubscriptionTarget> {
        self.subs.try_read().map(|s| s.clone()).unwrap_or_default()
    }
}

struct TestServer {
    base: String,
    processor: Arc<Processor>,
}

async fn start_server(connector_keys: &[&str]) -> TestServer {
    let processor = Arc::new(Processor::new(100, 100));
    let mut map: HashMap<String, Arc<dyn Connector>> = HashMap::new();
    for k in connector_keys {
        map.insert((*k).into(), MockConnector::boxed(k));
    }
    let gw = Gateway::new(processor.clone(), Arc::new(RwLock::new(map)));
    let router = gw.router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    TestServer {
        base: format!("127.0.0.1:{}", addr.port()),
        processor,
    }
}

fn quote(exchange: &str, symbol: &str, bid: &str, last: &str) -> Quote {
    Quote {
        exchange: exchange.into(),
        symbol: symbol.into(),
        bid_price: bid.into(),
        bid_qty: "12.3".into(),
        ask_price: "743.91".into(),
        ask_qty: "8.1".into(),
        last_price: last.into(),
        timestamp: 1791531605419,
        connector_receive_ts: 1791531605962,
    }
}

async fn next_text(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    ms: u64,
) -> Option<String> {
    match tokio::time::timeout(Duration::from_millis(ms), ws.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => Some(t.to_string()),
        _ => None,
    }
}

async fn http(method: &str, url: &str, body: Option<&str>) -> (u16, String) {
    // 极简 HTTP/1.1 客户端，避免为测试再引入 reqwest
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut stream = tokio::net::TcpStream::connect(host).await.unwrap();
    let body = body.unwrap_or("");
    let req = format!(
        "{} /{} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        method,
        path,
        host,
        body.len(),
        body
    );
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8(buf).unwrap();
    let status: u16 = text.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    // 处理 chunked：本测试响应体很小，axum 通常带 content-length；兜底去掉 chunk 头
    let body = if text.to_lowercase().contains("transfer-encoding: chunked") {
        body.lines()
            .filter(|l| l.starts_with('{') || l.starts_with('['))
            .collect::<Vec<_>>()
            .join("")
    } else {
        body
    };
    (status, body)
}

#[tokio::test]
async fn gateway_ws_delivers_quote_with_expected_envelope() {
    let srv = start_server(&[]).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws/v1/data", srv.base))
        .await
        .unwrap();
    ws.send(Message::Text(
        r#"{"action":"subscribe","streams":["quote.binance.BNBUSDT"]}"#.into(),
    ))
    .await
    .unwrap();
    let ack = next_text(&mut ws, 1000).await.expect("ack");
    assert_eq!(
        ack,
        r#"{"action":"subscribe","status":"success","stream":"quote.binance.BNBUSDT"}"#
    );

    srv.processor
        .handle_quote(quote("binance", "BNBUSDT", "743.90", ""));
    let msg = next_text(&mut ws, 1000).await.expect("quote push");
    // 外层与 tick 同风格：{"type":"quote","topic":...,"data":{...}}；币安无 last → last_price 为空串但仍输出
    assert_eq!(
        msg,
        r#"{"type":"quote","topic":"quote.binance.BNBUSDT","data":{"exchange":"binance","symbol":"BNBUSDT","bid_price":"743.90","bid_qty":"12.3","ask_price":"743.91","ask_qty":"8.1","last_price":"","timestamp":1791531605419,"connector_receive_ts":1791531605962}}"#
    );
}

#[tokio::test]
async fn gateway_ws_quote_only_matches_subscribed_topic_and_unsubscribe_works() {
    let srv = start_server(&[]).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws/v1/data", srv.base))
        .await
        .unwrap();
    ws.send(Message::Text(
        r#"{"action":"subscribe","streams":["quote.okx.BNB-USDT-SWAP"]}"#.into(),
    ))
    .await
    .unwrap();
    let _ack = next_text(&mut ws, 1000).await.unwrap();

    // 其它交易所/品种的 quote 不应送达
    srv.processor
        .handle_quote(quote("binance", "BNBUSDT", "1", ""));
    assert!(
        next_text(&mut ws, 300).await.is_none(),
        "不应收到未订阅的 quote"
    );

    // 订阅的主题送达，OKX 带 last
    srv.processor
        .handle_quote(quote("okx", "BNB-USDT-SWAP", "744", "744"));
    let msg = next_text(&mut ws, 1000).await.expect("okx quote");
    assert!(
        msg.starts_with(r#"{"type":"quote","topic":"quote.okx.BNB-USDT-SWAP""#),
        "{}",
        msg
    );
    assert!(msg.contains(r#""last_price":"744""#));

    // 退订后不再推送（等过节流窗口再发）
    ws.send(Message::Text(
        r#"{"action":"unsubscribe","streams":["quote.okx.BNB-USDT-SWAP"]}"#.into(),
    ))
    .await
    .unwrap();
    let unsub_ack = next_text(&mut ws, 1000).await.unwrap();
    assert!(unsub_ack.contains(r#""action":"unsubscribe""#));
    tokio::time::sleep(Duration::from_millis(150)).await;
    srv.processor
        .handle_quote(quote("okx", "BNB-USDT-SWAP", "745", "745"));
    assert!(next_text(&mut ws, 300).await.is_none(), "退订后不应再收到");
}

/// 同一 Gateway 连接同时订阅 tick 与 quote：各自 type 正确、互不串
#[tokio::test]
async fn gateway_ws_tick_and_quote_coexist() {
    let srv = start_server(&[]).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws/v1/data", srv.base))
        .await
        .unwrap();
    ws.send(Message::Text(
        r#"{"action":"subscribe","streams":["tick.binance.BNBUSDT","quote.binance.BNBUSDT"]}"#
            .into(),
    ))
    .await
    .unwrap();
    let _ = next_text(&mut ws, 1000).await.unwrap();
    let _ = next_text(&mut ws, 1000).await.unwrap();

    srv.processor.handle_tick(Tick {
        exchange: "binance".into(),
        symbol: "BNBUSDT".into(),
        price: "744.46".into(),
        timestamp: 1,
        ..Default::default()
    });
    srv.processor
        .handle_quote(quote("binance", "BNBUSDT", "744.4", ""));

    let mut types = Vec::new();
    for _ in 0..2 {
        let m = next_text(&mut ws, 1000).await.expect("push");
        let v: serde_json::Value = serde_json::from_str(&m).unwrap();
        types.push(v["type"].as_str().unwrap().to_string());
    }
    types.sort();
    assert_eq!(types, vec!["quote", "tick"]);
}

/// 兼容约束 3：旧 /ws 订阅 tick 时照常收到 tick（无 type 字段），绝不出现 quote
#[tokio::test]
async fn legacy_ws_never_receives_quote() {
    let srv = start_server(&[]).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", srv.base))
        .await
        .unwrap();
    ws.send(Message::Text(
        r#"{"op":"subscribe","args":["tick.binance.BNBUSDT","quote.binance.BNBUSDT"]}"#.into(),
    ))
    .await
    .unwrap();

    // 先收到 quote 拒绝错误，再收到只含 tick 的 ack
    let mut got_error = false;
    let mut ack = None;
    for _ in 0..2 {
        let m = next_text(&mut ws, 1000).await.expect("control msg");
        if m.contains("quote topics are not supported on legacy /ws") {
            got_error = true;
        } else if m.contains(r#""event":"subscribe""#) {
            ack = Some(m);
        }
    }
    assert!(got_error, "legacy 订阅 quote 必须被明确拒绝");
    assert_eq!(
        ack.expect("ack"),
        r#"{"event":"subscribe","status":"success","topics":["tick.binance.BNBUSDT"]}"#
    );

    srv.processor
        .handle_quote(quote("binance", "BNBUSDT", "744", ""));
    srv.processor.handle_tick(Tick {
        exchange: "binance".into(),
        symbol: "BNBUSDT".into(),
        price: "744.46".into(),
        timestamp: 1,
        ..Default::default()
    });

    let m = next_text(&mut ws, 1000).await.expect("legacy tick");
    assert_eq!(
        m,
        r#"{"topic":"tick.binance.BNBUSDT","data":{"exchange":"binance","symbol":"BNBUSDT","timestamp":1,"price":"744.46"}}"#
    );
    // 此后 500ms 内不得再有任何消息（尤其不能有 quote）
    while let Some(extra) = next_text(&mut ws, 500).await {
        assert!(
            !extra.contains("quote"),
            "legacy /ws 收到了 quote: {}",
            extra
        );
        assert!(!extra.contains(r#""type":"quote""#));
    }
}

#[tokio::test]
async fn rest_latest_quote_found_and_not_found() {
    let srv = start_server(&[]).await;
    let (status, body) = http(
        "GET",
        &format!(
            "http://{}/api/v1/data/latest/quote/binance/BNBUSDT",
            srv.base
        ),
        None,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body, r#"{"error":"no quote data for binance:BNBUSDT"}"#);

    srv.processor
        .handle_quote(quote("binance", "BNBUSDT", "743.90", ""));
    let (status, body) = http(
        "GET",
        &format!(
            "http://{}/api/v1/data/latest/quote/binance/bnbusdt",
            srv.base
        ),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        r#"{"exchange":"binance","symbol":"BNBUSDT","bid_price":"743.90","bid_qty":"12.3","ask_price":"743.91","ask_qty":"8.1","last_price":"","timestamp":1791531605419,"connector_receive_ts":1791531605962}"#
    );
}

#[tokio::test]
async fn rest_subscriptions_quote_post_list_delete() {
    let srv = start_server(&["binance", "binance-quote"]).await;
    let url = format!("http://{}/api/v1/subscriptions", srv.base);

    let (status, body) = http(
        "POST",
        &url,
        Some(r#"{"exchange":"binance","type":"QUOTE","symbols":["BNBUSDT"]}"#),
    )
    .await;
    assert_eq!((status, body.as_str()), (200, r#"{"success":true}"#));

    let (status, body) = http("GET", &url, None).await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        r#"[{"exchange":"binance","data_type":3,"symbol":"BNBUSDT"}]"#
    );

    let (status, _) = http(
        "DELETE",
        &url,
        Some(r#"{"exchange":"binance","type":"quote","symbols":["BNBUSDT"]}"#),
    )
    .await;
    assert_eq!(status, 200);
    let (_, body) = http("GET", &url, None).await;
    assert_eq!(body, "[]");
}

/// 开关关闭（没有 -quote 连接器）：QUOTE 订阅 404，且不会误加到 tick 连接器上
#[tokio::test]
async fn rest_quote_subscription_rejected_when_disabled_and_no_fallback() {
    let srv = start_server(&["binance"]).await;
    let url = format!("http://{}/api/v1/subscriptions", srv.base);
    let (status, body) = http(
        "POST",
        &url,
        Some(r#"{"exchange":"binance","type":"QUOTE","symbols":["BNBUSDT"]}"#),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body, r#"{"error":"quote connector not enabled: binance"}"#);
    let (_, body) = http("GET", &url, None).await;
    assert_eq!(body, "[]", "tick 连接器上不能被悄悄加上 quote 订阅");
}

/// 兼容约束 2：既有 TICK/KLINE 的 REST 行为、data_type 数值、错误文案与 1.0.0 相同
#[tokio::test]
async fn rest_existing_tick_kline_behaviour_unchanged() {
    let srv = start_server(&["binance", "okx", "okx-kline"]).await;
    let url = format!("http://{}/api/v1/subscriptions", srv.base);

    for (body, expect) in [
        (
            r#"{"exchange":"binance","type":"TICK","symbols":["BTCUSDT"]}"#,
            200u16,
        ),
        (
            r#"{"exchange":"okx","type":"KLINE","symbols":["BTC-USDT-SWAP"],"interval":"5m"}"#,
            200,
        ),
    ] {
        let (status, resp) = http("POST", &url, Some(body)).await;
        assert_eq!(status, expect);
        assert_eq!(resp, r#"{"success":true}"#);
    }
    let (_, body) = http("GET", &url, None).await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let mut pairs: Vec<(i64, String)> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["data_type"].as_i64().unwrap(),
                e["symbol"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![(1, "BTCUSDT".to_string()), (2, "BTC-USDT-SWAP".to_string())]
    );

    // 非法 type 的错误文案保持与 1.0.0 逐字节一致
    let (status, body) = http(
        "POST",
        &url,
        Some(r#"{"exchange":"binance","type":"DEPTH","symbols":["BTCUSDT"]}"#),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body, r#"{"error":"type must be TICK or KLINE"}"#);
    // KLINE 缺 interval 的文案不变
    let (status, body) = http(
        "POST",
        &url,
        Some(r#"{"exchange":"okx","type":"KLINE","symbols":["BTC-USDT-SWAP"]}"#),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body, r#"{"error":"interval is required for KLINE"}"#);
}

#[test]
fn data_type_display_is_stable() {
    assert_eq!(DataType::Tick.to_string(), "TICK");
    assert_eq!(DataType::Kline.to_string(), "KLINE");
    assert_eq!(DataType::Quote.to_string(), "QUOTE");
}
