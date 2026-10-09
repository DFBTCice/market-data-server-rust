//! 与 1.0.0 的逐字节兼容回放测试（任务书 §三-1、§三-3 的代码级证据）。
//!
//! 数据来源：2026-10-09 在 NAS 测试服务器 192.168.1.198 上的 1.0.0 部署，
//! 通过 `scripts/compat/capture_ws.mjs` 抓取 60 秒的真实推送原始行
//! （tick.binance.BNBUSDT / tick.okx.BNB-USDT-SWAP / kline.1m.binance.BNBUSDT / 旧 `/ws`）。
//!
//! 做法：把基线里每一行的 `data` 反序列化成领域对象，喂给**新版**处理器与网关，
//! 经真实 WebSocket 收回来，要求与基线原始行**逐字节相等**（字段名、字段顺序、数值格式、外层结构）。
//! 这证明：新增 quote 后，tick/kline 的输出格式与 1.0.0 无任何差异。

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

fn fixture_lines(name: &str) -> Vec<String> {
    let path = format!(
        "{}/tests/fixtures/baseline-1.0.0/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("缺少基线文件 {}: {}", path, e));
    let lines: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();
    assert!(
        !lines.is_empty(),
        "基线文件 {} 为空（空回放 = 没有验证任何东西）",
        name
    );
    lines
}

async fn start() -> (String, Arc<Processor>) {
    let processor = Arc::new(Processor::new_full(100, 100, 1024, 100));
    let map: HashMap<String, Arc<dyn md_connector::Connector>> = HashMap::new();
    let router = Gateway::new(processor.clone(), Arc::new(RwLock::new(map))).router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (format!("127.0.0.1:{}", addr.port()), processor)
}

async fn recv_text(ws: &mut Ws) -> String {
    match tokio::time::timeout(Duration::from_secs(2), ws.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => t.to_string(),
        other => panic!("未收到文本消息: {:?}", other),
    }
}

/// 返回第一个不同字节的位置（用于证明比较确实是逐字节的）
fn first_diff(a: &str, b: &str) -> Option<usize> {
    if a == b {
        return None;
    }
    Some(
        a.bytes()
            .zip(b.bytes())
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len())),
    )
}

#[derive(Clone, Copy)]
enum Kind {
    Tick,
    Kline,
}

fn publish(processor: &Processor, kind: Kind, data: &serde_json::Value) {
    match kind {
        Kind::Tick => processor.handle_tick(serde_json::from_value::<Tick>(data.clone()).unwrap()),
        Kind::Kline => {
            processor.handle_kline(serde_json::from_value::<Kline>(data.clone()).unwrap())
        }
    }
}

/// 回放一个基线文件：返回回放行数
async fn replay(
    ws_path: &str,
    sub_msg: String,
    ack_expect: &str,
    topic: &str,
    kind: Kind,
    file: &str,
) -> usize {
    let (addr, processor) = start().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{}{}", addr, ws_path))
        .await
        .unwrap();
    ws.send(Message::Text(sub_msg)).await.unwrap();
    let ack = recv_text(&mut ws).await;
    assert_eq!(ack, ack_expect, "订阅回执与 1.0.0 基线不一致");

    let lines = fixture_lines(file);
    let mut n = 0;
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["topic"], topic);
        publish(&processor, kind, &v["data"]);
        let got = recv_text(&mut ws).await;
        if let Some(pos) = first_diff(&got, line) {
            panic!(
                "与 1.0.0 基线不一致（第 {} 行，首个差异字节 {}）:\n  基线: {}\n  新版: {}",
                n + 1,
                pos,
                line,
                got
            );
        }
        n += 1;
    }
    n
}

fn gateway_sub(topic: &str) -> String {
    format!(r#"{{"action":"subscribe","streams":["{}"]}}"#, topic)
}
fn gateway_ack(topic: &str) -> String {
    format!(
        r#"{{"action":"subscribe","status":"success","stream":"{}"}}"#,
        topic
    )
}

#[tokio::test]
async fn gateway_tick_binance_matches_1_0_0_byte_for_byte() {
    let t = "tick.binance.BNBUSDT";
    let n = replay(
        "/ws/v1/data",
        gateway_sub(t),
        &gateway_ack(t),
        t,
        Kind::Tick,
        "gateway.tick.binance.BNBUSDT.jsonl",
    )
    .await;
    assert!(n >= 50, "回放行数过少: {}", n);
    println!("gateway tick.binance.BNBUSDT 逐字节一致: {} 行", n);
}

#[tokio::test]
async fn gateway_tick_okx_matches_1_0_0_byte_for_byte() {
    let t = "tick.okx.BNB-USDT-SWAP";
    let n = replay(
        "/ws/v1/data",
        gateway_sub(t),
        &gateway_ack(t),
        t,
        Kind::Tick,
        "gateway.tick.okx.BNB-USDT-SWAP.jsonl",
    )
    .await;
    assert!(n >= 5, "回放行数过少: {}", n);
    println!("gateway tick.okx.BNB-USDT-SWAP 逐字节一致: {} 行", n);
}

#[tokio::test]
async fn gateway_kline_matches_1_0_0_byte_for_byte() {
    let t = "kline.1m.binance.BNBUSDT";
    let n = replay(
        "/ws/v1/data",
        gateway_sub(t),
        &gateway_ack(t),
        t,
        Kind::Kline,
        "gateway.kline.1m.binance.BNBUSDT.jsonl",
    )
    .await;
    assert!(n >= 50, "回放行数过少: {}", n);
    println!("gateway kline.1m.binance.BNBUSDT 逐字节一致: {} 行", n);
}

#[tokio::test]
async fn legacy_ws_tick_matches_1_0_0_byte_for_byte() {
    for (t, file) in [
        ("tick.binance.BNBUSDT", "legacy.tick.binance.BNBUSDT.jsonl"),
        (
            "tick.okx.BNB-USDT-SWAP",
            "legacy.tick.okx.BNB-USDT-SWAP.jsonl",
        ),
    ] {
        let sub = format!(r#"{{"op":"subscribe","args":["{}"]}}"#, t);
        let ack = format!(
            r#"{{"event":"subscribe","status":"success","topics":["{}"]}}"#,
            t
        );
        let n = replay("/ws", sub, &ack, t, Kind::Tick, file).await;
        assert!(n >= 5, "回放行数过少: {}", n);
        println!("legacy /ws {} 逐字节一致: {} 行", t, n);
    }
}

/// 正对照：比较逻辑必须能发现"字段顺序/格式"的细微变化（否则上面的"一致"毫无意义）
#[test]
fn positive_control_comparison_detects_field_reorder_and_format_change() {
    let line = fixture_lines("gateway.tick.binance.BNBUSDT.jsonl").remove(0);
    assert_eq!(first_diff(&line, &line), None);

    // 交换 price 与 quantity 的顺序
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    let price = v["data"]["price"].as_str().unwrap();
    let qty = v["data"]["quantity"].as_str().unwrap();
    let from = format!(r#""price":"{}","quantity":"{}""#, price, qty);
    let to = format!(r#""quantity":"{}","price":"{}""#, qty, price);
    assert!(
        line.contains(&from),
        "基线行里 price/quantity 不相邻，无法构造对照"
    );
    let reordered = line.replace(&from, &to);
    assert!(
        first_diff(&reordered, &line).is_some(),
        "字段顺序变化必须被发现"
    );

    // 数值格式变化："744.870" -> "744.87"
    let reformatted = line.replace(
        &format!(r#""price":"{}""#, price),
        &format!(r#""price":"{}""#, price.trim_end_matches('0')),
    );
    assert!(
        first_diff(&reformatted, &line).is_some(),
        "数值格式变化必须被发现"
    );

    // 多一个字段
    let extra = line.replacen(r#""data":{"#, r#""data":{"bid_price":"1","#, 1);
    assert!(first_diff(&extra, &line).is_some(), "多出字段必须被发现");
}
