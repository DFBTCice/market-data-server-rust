//! `md_ws_subscriptions_active` gauge 集成测试（1.1.1 新增，任务书 §二 A3）。
//!
//! 该指标 = 全部 WS 连接（gateway + legacy）当前持有的 broadcast 接收端总数，
//! 退订 / 断线必须回落；"只增不减"即泄漏信号。

use futures_util::{SinkExt, StreamExt};
use md_gateway::Gateway;
use md_processor::Processor;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn start() -> (String, Arc<Processor>) {
    let processor = Arc::new(Processor::new(100, 100));
    let gw = Gateway::new(processor.clone(), Arc::new(RwLock::new(HashMap::new())));
    let router = gw.router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("127.0.0.1:{}", addr.port()), processor)
}

async fn connect(base: &str, path: &str) -> Ws {
    tokio_tungstenite::connect_async(format!("ws://{base}{path}"))
        .await
        .unwrap()
        .0
}

async fn ack(ws: &mut Ws) {
    let m = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("ack timeout");
    assert!(matches!(m, Some(Ok(Message::Text(_)))));
}

fn gauge(p: &Processor) -> u64 {
    p.metrics.ws_subscriptions_active.load(Ordering::Relaxed)
}

/// 轮询等待 gauge 到达期望值（服务端处理是异步的）
async fn wait_gauge(p: &Processor, want: u64) {
    for _ in 0..100 {
        if gauge(p) == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("gauge 期望 {want}，实际 {}", gauge(p));
}

#[tokio::test]
async fn gauge_follows_subscribe_unsubscribe_and_disconnect() {
    let (base, p) = start().await;
    assert_eq!(gauge(&p), 0);

    let mut ws = connect(&base, "/ws/v1/data").await;
    ws.send(Message::Text(
        r#"{"action":"subscribe","streams":["tick.binance.BTCUSDT","kline.1m.binance.BTCUSDT"]}"#
            .into(),
    ))
    .await
    .unwrap();
    ack(&mut ws).await;
    ack(&mut ws).await;
    wait_gauge(&p, 2).await;

    // 重复订阅：不新增接收端
    ws.send(Message::Text(
        r#"{"action":"subscribe","streams":["tick.binance.BTCUSDT"]}"#.into(),
    ))
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(gauge(&p), 2, "重复订阅不应增加接收端");

    // 退订一个：立即回落
    ws.send(Message::Text(
        r#"{"action":"unsubscribe","streams":["tick.binance.BTCUSDT"]}"#.into(),
    ))
    .await
    .unwrap();
    ack(&mut ws).await;
    wait_gauge(&p, 1).await;

    // 退订未订阅过的 stream：仍 ack，gauge 不变（1.0.0 语义）
    ws.send(Message::Text(
        r#"{"action":"unsubscribe","streams":["tick.okx.NOPE"]}"#.into(),
    ))
    .await
    .unwrap();
    ack(&mut ws).await;
    assert_eq!(gauge(&p), 1);

    // 断线：全部释放
    ws.close(None).await.unwrap();
    drop(ws);
    wait_gauge(&p, 0).await;
}

#[tokio::test]
async fn gauge_sums_across_gateway_and_legacy_connections() {
    let (base, p) = start().await;

    let mut gw = connect(&base, "/ws/v1/data").await;
    gw.send(Message::Text(
        r#"{"action":"subscribe","streams":["tick.binance.BTCUSDT"]}"#.into(),
    ))
    .await
    .unwrap();
    ack(&mut gw).await;

    let mut legacy = connect(&base, "/ws").await;
    legacy
        .send(Message::Text(
            r#"{"op":"subscribe","args":["tick.binance.BTCUSDT","tick.binance.ETHUSDT"]}"#.into(),
        ))
        .await
        .unwrap();
    ack(&mut legacy).await;
    wait_gauge(&p, 3).await;

    // legacy 的 unsubscribe 清空自己的接收端
    legacy
        .send(Message::Text(r#"{"op":"unsubscribe","args":[]}"#.into()))
        .await
        .unwrap();
    ack(&mut legacy).await;
    wait_gauge(&p, 1).await;

    drop(legacy);
    drop(gw);
    wait_gauge(&p, 0).await;
}

/// 反复 subscribe/unsubscribe 抖动后 gauge 不漂移（泄漏的直接判据）
#[tokio::test]
async fn gauge_does_not_drift_under_churn() {
    let (base, p) = start().await;
    let mut ws = connect(&base, "/ws/v1/data").await;
    for _ in 0..50 {
        ws.send(Message::Text(
            r#"{"action":"subscribe","streams":["kline.5m.okx.BTC-USDT-SWAP"]}"#.into(),
        ))
        .await
        .unwrap();
        ack(&mut ws).await;
        ws.send(Message::Text(
            r#"{"action":"unsubscribe","streams":["kline.5m.okx.BTC-USDT-SWAP"]}"#.into(),
        ))
        .await
        .unwrap();
        ack(&mut ws).await;
    }
    wait_gauge(&p, 0).await;
    drop(ws);
    wait_gauge(&p, 0).await;
}
