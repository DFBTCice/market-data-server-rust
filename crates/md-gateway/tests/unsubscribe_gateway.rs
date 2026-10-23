//! Gateway WS 退订语义集成测试（1.1.1 新增，对应任务书 §二 A1/A4）。
//!
//! 背景：1.0.0/1.1.0 的 `/ws/v1/data` "unsubscribe" 只从 `topics` 集合移除 stream，
//! 对应的 `broadcast::Receiver` 仍留在连接内；之后再次 subscribe 会再 push 一个新的
//! Receiver，于是同一事件被推送多份（现网探针实测 100% 重复）。
//!
//! 本文件只使用公开 API 与消息级断言（不依赖 1.1.1 新增的任何接口），
//! 因此可原样在修复前的代码上运行，作为"正向对照"（预期失败）。

use futures_util::{SinkExt, StreamExt};
use md_domain::types::{Kline, Tick};
use md_gateway::Gateway;
use md_processor::Processor;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct TestServer {
    base: String,
    processor: Arc<Processor>,
}

async fn start_server() -> TestServer {
    let processor = Arc::new(Processor::new(100, 100));
    let gw = Gateway::new(processor.clone(), Arc::new(RwLock::new(HashMap::new())));
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

async fn connect(srv: &TestServer) -> Ws {
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws/v1/data", srv.base))
        .await
        .unwrap();
    ws
}

async fn next_text(ws: &mut Ws, ms: u64) -> Option<String> {
    match tokio::time::timeout(Duration::from_millis(ms), ws.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => Some(t.to_string()),
        _ => None,
    }
}

/// 在 `window_ms` 内收集所有文本消息
async fn drain(ws: &mut Ws, window_ms: u64) -> Vec<String> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(window_ms);
    while let Ok(Some(Ok(m))) = tokio::time::timeout_at(deadline, ws.next()).await {
        if let Message::Text(t) = m {
            out.push(t.to_string());
        }
    }
    out
}

async fn send(ws: &mut Ws, action: &str, stream: &str) {
    ws.send(Message::Text(format!(
        r#"{{"action":"{action}","streams":["{stream}"]}}"#
    )))
    .await
    .unwrap();
}

fn tick(trade_id: i64) -> Tick {
    Tick {
        exchange: "binance".into(),
        symbol: "BNBUSDT".into(),
        price: "743.90".into(),
        quantity: "1".into(),
        timestamp: 1791531605419,
        trade_id,
        ..Default::default()
    }
}

fn kline(close: &str) -> Kline {
    Kline {
        exchange: "binance".into(),
        symbol: "BNBUSDT".into(),
        interval: "1m".into(),
        close: close.into(),
        ..Default::default()
    }
}

const TICK_TOPIC: &str = "tick.binance.BNBUSDT";

/// A4-1：subscribe → unsubscribe → subscribe，每个事件恰好 1 份
#[tokio::test]
async fn resubscribe_after_unsubscribe_delivers_exactly_one_copy() {
    let srv = start_server().await;
    let mut ws = connect(&srv).await;

    send(&mut ws, "subscribe", TICK_TOPIC).await;
    assert!(next_text(&mut ws, 1000)
        .await
        .unwrap()
        .contains("subscribe"));
    send(&mut ws, "unsubscribe", TICK_TOPIC).await;
    assert!(next_text(&mut ws, 1000)
        .await
        .unwrap()
        .contains("unsubscribe"));
    send(&mut ws, "subscribe", TICK_TOPIC).await;
    assert!(next_text(&mut ws, 1000)
        .await
        .unwrap()
        .contains("subscribe"));

    for id in 1..=3 {
        srv.processor.handle_tick(tick(id));
    }
    let msgs = drain(&mut ws, 400).await;
    assert_eq!(
        msgs.len(),
        3,
        "3 个事件应各收到 1 份，实际收到 {} 条：{:?}",
        msgs.len(),
        msgs
    );
}

/// A4-2：unsubscribe 之后，该 stream 的事件 0 份
#[tokio::test]
async fn no_events_after_unsubscribe() {
    let srv = start_server().await;
    let mut ws = connect(&srv).await;

    send(&mut ws, "subscribe", TICK_TOPIC).await;
    next_text(&mut ws, 1000).await.unwrap();
    srv.processor.handle_tick(tick(1));
    assert_eq!(drain(&mut ws, 300).await.len(), 1, "订阅期间应收到 1 条");

    send(&mut ws, "unsubscribe", TICK_TOPIC).await;
    let ack = next_text(&mut ws, 1000).await.unwrap();
    assert!(ack.contains(r#""action":"unsubscribe""#), "{ack}");

    for id in 2..=4 {
        srv.processor.handle_tick(tick(id));
    }
    let msgs = drain(&mut ws, 400).await;
    assert!(msgs.is_empty(), "退订后不应再收到事件：{:?}", msgs);
}

/// 重复 subscribe（同一连接、同一 stream）不新增接收端、不重复推送
#[tokio::test]
async fn duplicate_subscribe_adds_no_extra_copy() {
    let srv = start_server().await;
    let mut ws = connect(&srv).await;

    send(&mut ws, "subscribe", TICK_TOPIC).await;
    assert!(next_text(&mut ws, 1000).await.is_some(), "首次订阅应有 ack");
    send(&mut ws, "subscribe", TICK_TOPIC).await;
    assert!(
        next_text(&mut ws, 300).await.is_none(),
        "重复订阅不应再 ack（1.0.0 语义）"
    );

    srv.processor.handle_tick(tick(1));
    assert_eq!(drain(&mut ws, 400).await.len(), 1);
}

/// 多轮 subscribe/unsubscribe 抖动（模拟 1stbotv3 的 KLINE 订阅 churn）后仍是每事件 1 份
#[tokio::test]
async fn repeated_churn_still_one_copy_per_event() {
    let srv = start_server().await;
    let mut ws = connect(&srv).await;

    for _ in 0..5 {
        send(&mut ws, "subscribe", "kline.1m.binance.BNBUSDT").await;
        next_text(&mut ws, 1000).await.unwrap();
        send(&mut ws, "unsubscribe", "kline.1m.binance.BNBUSDT").await;
        next_text(&mut ws, 1000).await.unwrap();
    }
    send(&mut ws, "subscribe", "kline.1m.binance.BNBUSDT").await;
    next_text(&mut ws, 1000).await.unwrap();

    srv.processor.handle_kline(kline("743.1"));
    srv.processor.handle_kline(kline("743.2"));
    let msgs = drain(&mut ws, 400).await;
    assert_eq!(
        msgs.len(),
        2,
        "2 个事件应各 1 份，实际 {} 条：{:?}",
        msgs.len(),
        msgs
    );
}

/// 退订只影响自己：另一连接对同一 topic 的订阅不受影响，且仍只收 1 份
#[tokio::test]
async fn unsubscribe_on_one_connection_does_not_affect_another() {
    let srv = start_server().await;
    let mut a = connect(&srv).await;
    let mut b = connect(&srv).await;

    for ws in [&mut a, &mut b] {
        send(ws, "subscribe", TICK_TOPIC).await;
        next_text(ws, 1000).await.unwrap();
    }
    send(&mut a, "unsubscribe", TICK_TOPIC).await;
    next_text(&mut a, 1000).await.unwrap();
    send(&mut a, "subscribe", TICK_TOPIC).await;
    next_text(&mut a, 1000).await.unwrap();

    srv.processor.handle_tick(tick(1));
    assert_eq!(drain(&mut a, 300).await.len(), 1, "A 应恰好 1 份");
    assert_eq!(drain(&mut b, 300).await.len(), 1, "B 应恰好 1 份");
}

/// 退订多个 stream 的混合场景：只保留的那一个继续推送，且输出字节形态不变
#[tokio::test]
async fn partial_unsubscribe_keeps_other_streams_and_bytes() {
    let srv = start_server().await;
    let mut ws = connect(&srv).await;

    send(&mut ws, "subscribe", TICK_TOPIC).await;
    next_text(&mut ws, 1000).await.unwrap();
    send(&mut ws, "subscribe", "kline.1m.binance.BNBUSDT").await;
    next_text(&mut ws, 1000).await.unwrap();
    send(&mut ws, "unsubscribe", TICK_TOPIC).await;
    next_text(&mut ws, 1000).await.unwrap();

    srv.processor.handle_tick(tick(1));
    srv.processor.handle_kline(kline("743.5"));
    let msgs = drain(&mut ws, 400).await;
    assert_eq!(msgs.len(), 1, "{:?}", msgs);
    let v: serde_json::Value = serde_json::from_str(&msgs[0]).unwrap();
    assert_eq!(v["type"], "kline");
    assert_eq!(v["topic"], "kline.1m.binance.BNBUSDT");
    assert_eq!(v["data"]["close"], "743.5");
}
