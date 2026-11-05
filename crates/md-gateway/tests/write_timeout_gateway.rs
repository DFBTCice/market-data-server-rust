//! Gateway WS 写超时集成测试（1.1.1 新增，调查 D1/D2 的根因修复）。
//!
//! 背景：客户端"停读"（进程挂起 / 半开连接）时，服务端对该连接的 `sender.send().await`
//! 在发送缓冲塞满后永久阻塞，连接处理循环不再 poll broadcast Receiver，各主题的 broadcast 环里
//! 事件无法被"最后一个接收者"取走而滞留（每主题最多 `broadcast_capacity` 条）。
//! 旁路实验（evidence/1.1.1/d3-load：c10/c11）：停读 25 分钟 → jemalloc allocated 6.6→36 MiB，
//! musl 下 RSS 24→101 MiB 且客户端断开后**不再回落**。
//! `api_gateway.ws_write_wait`（默认 10s）一直在配置/文档里，但 Rust 版从未使用。1.1.1 起 Gateway 使用它。
//!
//! 本文件包含：
//!  - 正向对照 `without_write_wait_stalled_client_pins_the_connection`：不设写超时（= 1.0.0/1.1.0 行为），
//!    停读客户端把连接永久钉住（消息计数停止增长、连接与接收端都不释放）。
//!  - 修复验证 `stalled_client_is_kicked_after_write_wait`：设置写超时后连接被踢、接收端全部释放。
//!  - 健康客户端不受影响 `healthy_client_is_not_affected_by_write_wait`。

use futures_util::StreamExt;
use md_domain::types::Tick;
use md_gateway::Gateway;
use md_processor::Processor;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

const TOPIC: &str = "tick.binance.BNBUSDT";

struct Srv {
    addr: SocketAddr,
    processor: Arc<Processor>,
}

async fn start_server(write_wait: Option<Duration>) -> Srv {
    let processor = Arc::new(Processor::new(100, 100));
    let mut gw = Gateway::new(processor.clone(), Arc::new(RwLock::new(HashMap::new())));
    if let Some(d) = write_wait {
        gw = gw.with_ws_write_wait(d);
    }
    let router = gw.router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Srv { addr, processor }
}

fn tick(id: i64) -> Tick {
    Tick {
        exchange: "binance".into(),
        symbol: "BNBUSDT".into(),
        price: "743.90".into(),
        quantity: "1".into(),
        timestamp: 1791531605419,
        trade_id: id,
        ..Default::default()
    }
}

/// 裸 TCP + 手写 WebSocket 握手/订阅帧，然后**再也不读**。接收缓冲设得很小，使发送缓冲很快塞满。
async fn stalled_client(addr: SocketAddr) -> TcpStream {
    let sock = TcpSocket::new_v4().unwrap();
    sock.set_recv_buffer_size(4096).unwrap();
    let mut s = sock.connect(addr).await.unwrap();
    let req = format!(
        "GET /ws/v1/data HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    s.write_all(req.as_bytes()).await.unwrap();
    // 读完握手响应头（逐字节读到 \r\n\r\n，避免多读）
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut b).await.unwrap();
        head.push(b[0]);
    }
    assert!(String::from_utf8_lossy(&head).contains("101"));
    // 订阅帧（客户端帧必须带掩码）
    let payload = format!(r#"{{"action":"subscribe","streams":["{TOPIC}"]}}"#).into_bytes();
    assert!(payload.len() < 126);
    let mask = [1u8, 2, 3, 4];
    let mut frame = vec![0x81, 0x80 | payload.len() as u8];
    frame.extend_from_slice(&mask);
    frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    s.write_all(&frame).await.unwrap();
    s
}

async fn wait_until(what: &str, secs: u64, mut cond: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < deadline {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("等待超时: {what}");
}

/// 持续发布 tick，直到 `until` 成立或超时；批量发布后让出调度，保证连接处理循环能运行。
async fn pump_ticks(srv: &Srv, secs: u64, mut until: impl FnMut() -> bool) -> i64 {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    let mut id = 0i64;
    while tokio::time::Instant::now() < deadline {
        for _ in 0..500 {
            id += 1;
            srv.processor.handle_tick(tick(id));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        if until() {
            break;
        }
    }
    id
}

fn active(srv: &Srv) -> u64 {
    srv.processor
        .metrics
        .ws_active_clients
        .load(Ordering::Relaxed)
}
fn subs(srv: &Srv) -> u64 {
    srv.processor
        .metrics
        .ws_subscriptions_active
        .load(Ordering::Relaxed)
}
fn kicked(srv: &Srv) -> u64 {
    srv.processor
        .metrics
        .ws_kicked_write_timeout_total
        .load(Ordering::Relaxed)
}
fn sent(srv: &Srv) -> u64 {
    srv.processor.metrics.snapshot().ws_messages_sent_tick
}

/// 正向对照：不设写超时（= 修复前行为）。停读客户端把连接钉住，不会被释放。
#[tokio::test]
async fn without_write_wait_stalled_client_pins_the_connection() {
    let srv = start_server(None).await;
    let client = stalled_client(srv.addr).await;
    wait_until("订阅生效", 5, || subs(&srv) == 1).await;

    // 一直发到服务端发送缓冲塞满（已发消息数不再增长）
    let mut last = 0;
    let mut stable = 0;
    pump_ticks(&srv, 20, || {
        let now = sent(&srv);
        if now == last {
            stable += 1;
        } else {
            stable = 0;
            last = now;
        }
        stable >= 40 // 连续 ~200ms 无新增发送
    })
    .await;
    assert!(stable >= 40, "未能把服务端发送缓冲塞满（sent={last}）");

    // 再等 3 秒并持续发布：连接依旧存在、接收端依旧被持有、没有被踢
    let before = sent(&srv);
    pump_ticks(&srv, 3, || false).await;
    assert_eq!(sent(&srv), before, "连接应卡死在 send 上，不再推送");
    assert_eq!(active(&srv), 1, "停读客户端把连接钉住了");
    assert_eq!(subs(&srv), 1, "接收端没有释放");
    assert_eq!(kicked(&srv), 0);

    // 客户端真正断开后才会释放（写失败）
    drop(client);
    pump_ticks(&srv, 10, || active(&srv) == 0).await;
    wait_until("客户端关闭后释放", 10, || active(&srv) == 0).await;
}

/// 修复验证：设置写超时后，停读客户端在 write_wait 后被踢出，连接数与接收端全部回落。
#[tokio::test]
async fn stalled_client_is_kicked_after_write_wait() {
    let srv = start_server(Some(Duration::from_millis(500))).await;
    let _client = stalled_client(srv.addr).await; // 持有到测试结束，仍然不读
    wait_until("订阅生效", 5, || subs(&srv) == 1).await;

    pump_ticks(&srv, 20, || kicked(&srv) >= 1).await;
    wait_until("停读客户端被踢", 10, || kicked(&srv) == 1).await;
    wait_until("连接数回落", 5, || active(&srv) == 0).await;
    assert_eq!(subs(&srv), 0, "被踢后 broadcast 接收端必须全部释放");
}

/// 健康客户端：开了写超时也照常收到每一条、不会被踢。
#[tokio::test]
async fn healthy_client_is_not_affected_by_write_wait() {
    let srv = start_server(Some(Duration::from_millis(500))).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws/v1/data", srv.addr))
        .await
        .unwrap();
    use futures_util::SinkExt;
    ws.send(Message::Text(format!(
        r#"{{"action":"subscribe","streams":["{TOPIC}"]}}"#
    )))
    .await
    .unwrap();
    // ack
    let ack = ws.next().await.unwrap().unwrap();
    assert!(matches!(ack, Message::Text(t) if t.contains("\"status\":\"success\"")));

    for id in 1..=200 {
        srv.processor.handle_tick(tick(id));
    }
    let mut got = 0;
    while got < 200 {
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("健康客户端应收到全部消息")
            .unwrap()
            .unwrap();
        if let Message::Text(t) = m {
            if t.contains("\"type\":\"tick\"") {
                got += 1;
            }
        }
    }
    assert_eq!(kicked(&srv), 0);
    assert_eq!(active(&srv), 1);
    assert_eq!(subs(&srv), 1);
}
