//! 运行期订阅闲置退订与按需补订的集成测试（1.2.0，任务书 §四 集成测试各条）。
//!
//! 真实链路：假上游 WS（币安 / OKX 两种协议）← 真实 `BaseConnector` → `Processor` → 真实 axum Gateway（REST + `/ws/v1/data` + `/ws`）
//! ← 真实 WebSocket 客户端；运行期订阅管理用缩短的参数（闲置 600ms、检查 100ms）。
//! 假上游只给"当前已订阅"的 stream 推送，所以"上游收到退订 / 补订"和"推送恢复、不翻倍"都能直接观察。

use futures_util::{SinkExt, StreamExt};
use md_connector::base::BaseConnector;
use md_connector::binance::{BinanceAdapter, BinanceConnectorConfig};
use md_connector::okx::{OkxAdapter, OkxConnectorConfig, OkxStreamMode};
use md_connector::{Connector, DataType, SubscriptionTarget};
use md_gateway::runtime_subs::{ConfigView, RuntimeSubscriptions, Settings, SubKey};
use md_gateway::Gateway;
use md_processor::Processor;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::Message;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const IDLE: Duration = Duration::from_millis(600);
const CHECK: Duration = Duration::from_millis(100);

fn fast_settings() -> Settings {
    Settings {
        enabled: true,
        idle: IDLE,
        check_interval: CHECK,
        startup_grace: Duration::ZERO,
        max_runtime_subscriptions: 50,
        demand_batch_delay: Duration::from_millis(50),
        reject_backoff_initial: Duration::from_secs(60),
        reject_backoff_max: Duration::from_secs(600),
        upstream_burst: 100,
        upstream_refill: Duration::from_millis(10),
    }
}

// ---------------------------------------------------------------------------
// 假上游
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Proto {
    Binance,
    Okx,
}

/// 记录收到的订阅帧；只向订阅了对应 stream 的连接推送
#[derive(Clone)]
struct FakeUpstream {
    url: String,
    frames: Arc<Mutex<Vec<serde_json::Value>>>,
    connections: Arc<Mutex<usize>>,
    push_tx: broadcast::Sender<(String, String)>,
}

impl FakeUpstream {
    async fn start(proto: Proto, reject: &[&str]) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "ws://127.0.0.1:{}/ws",
            listener.local_addr().unwrap().port()
        );
        let (push_tx, _) = broadcast::channel(1024);
        let up = Self {
            url,
            frames: Arc::new(Mutex::new(Vec::new())),
            connections: Arc::new(Mutex::new(0)),
            push_tx,
        };
        let reject: HashSet<String> = reject.iter().map(|s| s.to_string()).collect();
        let me = up.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let me = me.clone();
                let reject = reject.clone();
                tokio::spawn(async move {
                    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    *me.connections.lock().unwrap() += 1;
                    me.serve(ws, proto, reject).await;
                });
            }
        });
        up
    }

    async fn serve(
        &self,
        mut ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        proto: Proto,
        reject: HashSet<String>,
    ) {
        let mut subs: HashSet<String> = HashSet::new();
        let mut push_rx = self.push_tx.subscribe();
        loop {
            tokio::select! {
                msg = ws.next() => {
                    let Some(Ok(Message::Text(text))) = msg else {
                        if matches!(msg, Some(Ok(_))) { continue; }
                        return;
                    };
                    if text.as_str() == "ping" {
                        let _ = ws.send(Message::Text("pong".into())).await;
                        continue;
                    }
                    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                    self.frames.lock().unwrap().push(v.clone());
                    for reply in apply_frame(proto, &v, &mut subs, &reject) {
                        let _ = ws.send(Message::Text(reply)).await;
                    }
                }
                pushed = push_rx.recv() => {
                    let Ok((stream, payload)) = pushed else { continue };
                    if subs.contains(&stream) {
                        let _ = ws.send(Message::Text(payload)).await;
                    }
                }
            }
        }
    }

    /// 收到的、op 为 `op`（SUBSCRIBE/UNSUBSCRIBE 或 subscribe/unsubscribe）且包含 `stream` 的帧数
    fn count(&self, op: &str, stream: &str) -> usize {
        self.frames
            .lock()
            .unwrap()
            .iter()
            .filter(|f| {
                frame_op(f).eq_ignore_ascii_case(op) && frame_streams(f).iter().any(|s| s == stream)
            })
            .count()
    }

    fn push_binance_trade(&self, symbol: &str, id: i64) {
        let stream = format!("{}@aggTrade", symbol.to_lowercase());
        let payload = serde_json::json!({
            "stream": stream,
            "data": {"e":"aggTrade","E":1791531605419i64 + id,"s":symbol,"a":id,"p":"150.10","q":"1","f":id,"l":id,"T":1791531605400i64 + id,"m":true}
        });
        let _ = self.push_tx.send((stream, payload.to_string()));
    }

    fn push_okx_trade(&self, inst_id: &str, id: i64) {
        let stream = format!("trades:{}", inst_id);
        let payload = serde_json::json!({
            "arg": {"channel":"trades","instId":inst_id},
            "data": [{"instId":inst_id,"tradeId":id.to_string(),"px":"150.1","sz":"1","side":"buy","ts":(1791531605419i64 + id).to_string()}]
        });
        let _ = self.push_tx.send((stream, payload.to_string()));
    }
}

fn frame_op(f: &serde_json::Value) -> String {
    f.get("method")
        .or_else(|| f.get("op"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// 帧里的 stream 键：币安 `params`；OKX `args` → `channel:instId`
fn frame_streams(f: &serde_json::Value) -> Vec<String> {
    if let Some(p) = f.get("params").and_then(|v| v.as_array()) {
        return p
            .iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect();
    }
    f.get("args")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|x| {
                    format!(
                        "{}:{}",
                        x["channel"].as_str().unwrap_or(""),
                        x["instId"].as_str().unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn apply_frame(
    proto: Proto,
    f: &serde_json::Value,
    subs: &mut HashSet<String>,
    reject: &HashSet<String>,
) -> Vec<String> {
    let op = frame_op(f).to_lowercase();
    let mut replies = Vec::new();
    for s in frame_streams(f) {
        let inst = s
            .split_once(':')
            .map(|(_, i)| i.to_string())
            .unwrap_or_default();
        if proto == Proto::Okx && op == "subscribe" && reject.contains(&inst) {
            let (channel, _) = s.split_once(':').unwrap();
            // OKX 实测格式：不带 arg，频道与合约只在 msg 里（evidence/1.2.0/upstream-probe）
            replies.push(serde_json::json!({
                "event": "error",
                "msg": format!("Subscribe failed, wrong URL or channel:{channel},instId:{inst} doesn't exist. Please use the correct URL, channel and parameters referring to API document."),
                "code": "60018",
                "connId": "fake"
            }).to_string());
            continue;
        }
        if op == "subscribe" {
            subs.insert(s.clone());
        } else {
            subs.remove(&s);
        }
        if proto == Proto::Okx {
            let (channel, inst) = s.split_once(':').unwrap();
            replies.push(serde_json::json!({"event": op, "arg": {"channel": channel, "instId": inst}, "connId": "fake"}).to_string());
        }
    }
    if proto == Proto::Binance {
        replies.push(r#"{"result":null,"id":1}"#.to_string());
    }
    replies
}

// ---------------------------------------------------------------------------
// 测试服务
// ---------------------------------------------------------------------------

struct Rig {
    base: String,
    processor: Arc<Processor>,
    runtime: Arc<RuntimeSubscriptions>,
    binance: FakeUpstream,
    okx: FakeUpstream,
    started: Instant,
}

fn tick_target(ex: &str, sym: &str) -> SubscriptionTarget {
    SubscriptionTarget {
        exchange: ex.into(),
        data_type: DataType::Tick,
        symbol: sym.into(),
        kline_interval: None,
    }
}

/// `config_ticks`：币安配置名单里的 tick 币对（受保护）；`okx_reject`：OKX 假上游拒绝的合约
async fn rig(settings: Settings, config_ticks: &[&str], okx_reject: &[&str]) -> Rig {
    let processor = Arc::new(Processor::new(1000, 1000));
    let p = processor.clone();
    tokio::spawn(async move { p.run().await });

    let binance = FakeUpstream::start(Proto::Binance, &[]).await;
    let okx = FakeUpstream::start(Proto::Okx, okx_reject).await;
    let (reject_tx, reject_rx) = tokio::sync::mpsc::unbounded_channel();

    let b = Arc::new(
        BaseConnector::new(
            BinanceAdapter::new(BinanceConnectorConfig {
                stream_base_url: binance.url.clone(),
                subscribe_ticks: vec![],
                subscribe_klines: HashMap::new(),
                reconnect_delay: Duration::from_millis(200),
                ping_interval: Duration::from_secs(30),
            }),
            processor.tick_tx(),
            processor.kline_tx(),
            Duration::from_millis(200),
        )
        .with_reject_sink(reject_tx.clone()),
    );
    let config_targets: Vec<SubscriptionTarget> = config_ticks
        .iter()
        .map(|s| tick_target("binance", s))
        .collect();
    b.add_subscriptions(config_targets.clone()).await.unwrap();
    let o = Arc::new(
        BaseConnector::new(
            OkxAdapter::with_mode(
                OkxConnectorConfig {
                    stream_base_url_public: okx.url.clone(),
                    stream_base_url_business: okx.url.clone(),
                    subscribe_ticks: vec![],
                    subscribe_klines: HashMap::new(),
                    reconnect_delay: Duration::from_millis(200),
                    ping_interval: Duration::from_secs(25),
                },
                OkxStreamMode::Public,
            ),
            processor.tick_tx(),
            processor.kline_tx(),
            Duration::from_millis(200),
        )
        .with_reject_sink(reject_tx),
    );
    let mut map: HashMap<String, Arc<dyn Connector>> = HashMap::new();
    map.insert("binance".into(), b.clone());
    map.insert("okx".into(), o.clone());
    let connectors = Arc::new(RwLock::new(map));
    tokio::spawn(async move { b.run_connection_loop().await });
    tokio::spawn(async move { o.run_connection_loop().await });

    let protected = config_targets.iter().map(SubKey::of).collect();
    let runtime = RuntimeSubscriptions::new(
        processor.clone(),
        connectors.clone(),
        ConfigView {
            settings,
            protected,
        },
        reject_rx,
    );
    tokio::spawn(runtime.clone().run());
    let started = Instant::now();

    let router = Gateway::new(processor.clone(), connectors)
        .with_runtime_subscriptions(runtime.clone())
        .router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let r = Rig {
        base,
        processor,
        runtime,
        binance,
        okx,
        started,
    };
    // 两条上游连接都建立后再开始
    wait_until(Duration::from_secs(5), || {
        *r.binance.connections.lock().unwrap() > 0 && *r.okx.connections.lock().unwrap() > 0
    })
    .await;
    for s in config_ticks {
        let stream = format!("{}@aggTrade", s.to_lowercase());
        assert!(
            wait_until(Duration::from_secs(5), || r
                .binance
                .count("SUBSCRIBE", &stream)
                > 0)
            .await
        );
    }
    r
}

async fn wait_until(limit: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    cond()
}

async fn http(method: &str, url: &str, body: Option<&str>) -> (u16, String) {
    let (host, path) = url.split_once('/').unwrap();
    let mut stream = tokio::net::TcpStream::connect(host).await.unwrap();
    let body = body.unwrap_or("");
    let req = format!(
        "{method} /{path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8(buf).unwrap();
    let status: u16 = text.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (status, body)
}

impl Rig {
    async fn post_tick(&self, ex: &str, sym: &str) {
        let (status, body) = http(
            "POST",
            &format!("{}/api/v1/subscriptions", self.base),
            Some(&format!(
                r#"{{"exchange":"{ex}","type":"TICK","symbols":["{sym}"]}}"#
            )),
        )
        .await;
        assert_eq!((status, body.as_str()), (200, r#"{"success":true}"#));
    }

    async fn subscriptions(&self) -> Vec<serde_json::Value> {
        let (status, body) =
            http("GET", &format!("{}/api/v1/subscriptions", self.base), None).await;
        assert_eq!(status, 200);
        serde_json::from_str(&body).unwrap()
    }

    async fn subscriptions_raw(&self) -> String {
        http("GET", &format!("{}/api/v1/subscriptions", self.base), None)
            .await
            .1
    }

    async fn has_sub(&self, sym: &str) -> bool {
        self.subscriptions()
            .await
            .iter()
            .any(|s| s["symbol"] == sym)
    }

    async fn ws(&self, path: &str) -> Ws {
        tokio_tungstenite::connect_async(format!("ws://{}{}", self.base, path))
            .await
            .unwrap()
            .0
    }

    async fn gateway_ws(&self, topic: &str) -> Ws {
        let mut ws = self.ws("/ws/v1/data").await;
        send(&mut ws, "subscribe", topic).await;
        let ack = next_text(&mut ws, 1000).await.unwrap();
        assert!(ack.contains(r#""action":"subscribe""#), "{ack}");
        ws
    }

    fn metric(&self, name: &str) -> u64 {
        let mut out = String::new();
        self.runtime.metrics().render_prometheus(&mut out);
        out.lines()
            .find_map(|l| l.strip_prefix(&format!("{name} ")))
            .unwrap_or_else(|| panic!("missing {name}"))
            .parse()
            .unwrap()
    }
}

async fn send(ws: &mut Ws, action: &str, topic: &str) {
    ws.send(Message::Text(format!(
        r#"{{"action":"{action}","streams":["{topic}"]}}"#
    )))
    .await
    .unwrap();
}

async fn next_text(ws: &mut Ws, ms: u64) -> Option<String> {
    match tokio::time::timeout(Duration::from_millis(ms), ws.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => Some(t.to_string()),
        _ => None,
    }
}

async fn drain(ws: &mut Ws, window_ms: u64) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(window_ms);
    while let Ok(Some(Ok(m))) = tokio::time::timeout_at(deadline, ws.next()).await {
        if let Message::Text(t) = m {
            out.push(serde_json::from_str(t.as_str()).unwrap());
        }
    }
    out
}

const SOL: &str = "solusdt@aggTrade";

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

/// 运行期订阅无人接收超过闲置时长 → 上游收到退订、desired_subs 移除；实际闲置不短于 idle
#[tokio::test]
async fn idle_runtime_subscription_is_unsubscribed_upstream() {
    let r = rig(fast_settings(), &["BTCUSDT"], &[]).await;
    let posted = Instant::now();
    r.post_tick("binance", "SOLUSDT").await;
    assert!(
        wait_until(Duration::from_secs(2), || r.binance.count("SUBSCRIBE", SOL)
            == 1)
        .await
    );

    tokio::time::sleep(IDLE / 2).await;
    assert_eq!(
        r.binance.count("UNSUBSCRIBE", SOL),
        0,
        "未到闲置时长不得退订"
    );

    assert!(
        wait_until(Duration::from_secs(3), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await,
        "闲置后上游应收到一次退订"
    );
    assert!(
        posted.elapsed() >= IDLE,
        "实际闲置 {:?} 短于 {:?}",
        posted.elapsed(),
        IDLE
    );
    assert!(!r.has_sub("SOLUSDT").await, "desired_subs 应已移除");
    assert!(r.has_sub("BTCUSDT").await);
    assert_eq!(
        r.metric("md_runtime_subscriptions_idle_unsubscribed_total"),
        1
    );
    assert_eq!(r.metric("md_runtime_subscriptions"), 0);
    assert_eq!(r.binance.count("UNSUBSCRIBE", "btcusdt@aggTrade"), 0);
}

/// 配置名单订阅无人接收 → 永不退订；配置与运行期同时登记（大小写不同）→ 按配置处理
#[tokio::test]
async fn config_subscription_is_never_unsubscribed() {
    let r = rig(fast_settings(), &["BTCUSDT", "ETHUSDT"], &[]).await;
    r.post_tick("binance", "btcusdt").await;

    tokio::time::sleep(IDLE * 5).await;
    for s in ["btcusdt@aggTrade", "ethusdt@aggTrade"] {
        assert_eq!(r.binance.count("UNSUBSCRIBE", s), 0, "{s} 不得被退订");
    }
    let subs = r.subscriptions().await;
    assert_eq!(subs.len(), 3, "{subs:?}");
    assert_eq!(
        r.metric("md_runtime_subscriptions_idle_unsubscribed_total"),
        0
    );
    assert_eq!(r.metric("md_runtime_subscriptions"), 0);
}

/// 有 WS 接收方 → 不退；断开后在闲置时长内重连 → 不退；彻底离开 → 闲置后退
#[tokio::test]
async fn ws_receiver_and_quick_reconnect_keep_subscription() {
    let r = rig(fast_settings(), &[], &[]).await;
    r.post_tick("binance", "SOLUSDT").await;
    let ws = r.gateway_ws("tick.binance.SOLUSDT").await;

    tokio::time::sleep(IDLE * 3).await;
    assert_eq!(r.binance.count("UNSUBSCRIBE", SOL), 0, "有接收方时不得退订");

    drop(ws);
    tokio::time::sleep(IDLE / 3).await;
    let ws = r.gateway_ws("tick.binance.SOLUSDT").await;
    tokio::time::sleep(IDLE * 2).await;
    assert_eq!(
        r.binance.count("UNSUBSCRIBE", SOL),
        0,
        "闲置时长内重连不得退订"
    );

    drop(ws);
    let left = Instant::now();
    assert!(
        wait_until(Duration::from_secs(3), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await
    );
    assert!(
        left.elapsed() >= IDLE,
        "离开后 {:?} 就退订了",
        left.elapsed()
    );
}

/// 旧 /ws 的订阅同样算接收方
#[tokio::test]
async fn legacy_ws_receiver_keeps_subscription() {
    let r = rig(fast_settings(), &[], &[]).await;
    r.post_tick("binance", "SOLUSDT").await;
    let mut ws = r.ws("/ws").await;
    ws.send(Message::Text(
        r#"{"op":"subscribe","args":["tick.binance.SOLUSDT"]}"#.into(),
    ))
    .await
    .unwrap();
    assert!(next_text(&mut ws, 1000)
        .await
        .unwrap()
        .contains("subscribe"));

    tokio::time::sleep(IDLE * 3).await;
    assert_eq!(r.binance.count("UNSUBSCRIBE", SOL), 0);
    drop(ws);
    assert!(
        wait_until(Duration::from_secs(3), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await
    );
}

/// 启动保护期内 → 不退；保护期过后才退
#[tokio::test]
async fn startup_grace_defers_idle_unsubscribe() {
    let grace = Duration::from_millis(2000);
    let r = rig(
        Settings {
            startup_grace: grace,
            idle: Duration::from_millis(300),
            ..fast_settings()
        },
        &[],
        &[],
    )
    .await;
    r.post_tick("binance", "SOLUSDT").await;

    let until_grace = grace.saturating_sub(r.started.elapsed());
    tokio::time::sleep(until_grace.saturating_sub(Duration::from_millis(200))).await;
    assert_eq!(r.binance.count("UNSUBSCRIBE", SOL), 0, "保护期内不得退订");
    assert!(
        wait_until(Duration::from_secs(3), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await
    );
    assert!(r.started.elapsed() >= grace);
}

/// 只读 REST 查询 → 刷新闲置计时（路径写法不区分大小写）；停止查询后闲置到期退订
#[tokio::test]
async fn rest_reads_refresh_idle_timer() {
    let r = rig(fast_settings(), &[], &[]).await;
    r.post_tick("binance", "SOLUSDT").await;

    let reading = Instant::now();
    while reading.elapsed() < IDLE * 4 {
        let (status, _) = http(
            "GET",
            &format!("{}/api/v1/data/latest/tick/binance/solusdt", r.base),
            None,
        )
        .await;
        assert_eq!(status, 404, "无数据时照旧 404");
        tokio::time::sleep(IDLE / 3).await;
    }
    assert_eq!(
        r.binance.count("UNSUBSCRIBE", SOL),
        0,
        "持续有 REST 查询时不得退订"
    );

    let stopped = Instant::now();
    assert!(
        wait_until(Duration::from_secs(3), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await
    );
    assert!(stopped.elapsed() >= IDLE / 2);
}

/// 被退订后同一连接再订阅该话题 → 自动补订、推送恢复、每条恰好 1 份（订 → 自动退 → 补订全程同一连接）
#[tokio::test]
async fn auto_unsubscribe_then_on_demand_resubscribe_on_same_connection_without_duplicates() {
    let r = rig(fast_settings(), &[], &[]).await;
    r.post_tick("binance", "SOLUSDT").await;
    let mut ws = r.gateway_ws("tick.binance.SOLUSDT").await;
    assert!(
        wait_until(Duration::from_secs(2), || r.binance.count("SUBSCRIBE", SOL)
            == 1)
        .await
    );

    r.binance.push_binance_trade("SOLUSDT", 1);
    let got = drain(&mut ws, 400).await;
    assert_eq!(got.len(), 1, "{got:?}");

    send(&mut ws, "unsubscribe", "tick.binance.SOLUSDT").await;
    assert!(next_text(&mut ws, 1000)
        .await
        .unwrap()
        .contains("unsubscribe"));
    assert!(
        wait_until(Duration::from_secs(3), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await,
        "接收方退订后应闲置退订上游"
    );
    r.binance.push_binance_trade("SOLUSDT", 2);
    assert!(drain(&mut ws, 300).await.is_empty());

    send(&mut ws, "subscribe", "tick.binance.SOLUSDT").await;
    assert!(next_text(&mut ws, 1000)
        .await
        .unwrap()
        .contains("subscribe"));
    assert!(
        wait_until(Duration::from_secs(2), || r.binance.count("SUBSCRIBE", SOL)
            == 2)
        .await,
        "应自动补订（POST 一次 + 补订一次）"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    for id in 3..=5 {
        r.binance.push_binance_trade("SOLUSDT", id);
    }
    let got = drain(&mut ws, 500).await;
    let ids: Vec<i64> = got
        .iter()
        .map(|m| m["data"]["trade_id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![3, 4, 5], "每条恰好 1 份：{got:?}");
    assert_eq!(
        r.metric("md_runtime_subscriptions_demand_subscribed_total"),
        1
    );
    let subs = r.subscriptions().await;
    let sol = subs
        .iter()
        .find(|s| s["symbol"] == "SOLUSDT")
        .expect("补订后应列出");
    assert_eq!(sol["source"], "runtime");
}

/// 从未登记过的话题（OMS 交易新币对）：订阅即自动补订，推送到达
#[tokio::test]
async fn subscribing_a_topic_missing_upstream_subscribes_on_demand() {
    let r = rig(fast_settings(), &[], &[]).await;
    let mut ws = r.gateway_ws("tick.okx.SOL-USDT-SWAP").await;
    assert!(
        wait_until(Duration::from_secs(2), || r
            .okx
            .count("subscribe", "trades:SOL-USDT-SWAP")
            == 1)
        .await
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    r.okx.push_okx_trade("SOL-USDT-SWAP", 7);
    let got = drain(&mut ws, 500).await;
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["topic"], "tick.okx.SOL-USDT-SWAP");

    // 不规范写法的话题收不到推送，也不会触发补订
    let _lower = r.gateway_ws("tick.okx.sol-usdt-swap").await;
    let _bad = r.gateway_ws("tick.binance").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(r.okx.count("subscribe", "trades:sol-usdt-swap"), 0);
    assert_eq!(
        r.metric("md_runtime_subscriptions_demand_subscribed_total"),
        1
    );
}

/// 上游拒绝的币对 → 撤回、计失败、退避期内不再重试（接收方一直在、还反复重订也不重试）
#[tokio::test]
async fn upstream_rejected_symbol_is_not_retried() {
    let r = rig(fast_settings(), &[], &["NOSUCH-USDT-SWAP"]).await;
    let mut ws = r.gateway_ws("tick.okx.NOSUCH-USDT-SWAP").await;
    let stream = "trades:NOSUCH-USDT-SWAP";
    assert!(
        wait_until(Duration::from_secs(2), || r.okx.count("subscribe", stream)
            == 1)
        .await
    );
    assert!(
        wait_until(Duration::from_secs(2), || r
            .metric("md_runtime_subscriptions_demand_failed_total")
            == 1)
        .await
    );
    assert!(!r.has_sub("NOSUCH-USDT-SWAP").await, "被拒的按需订阅应撤回");

    for _ in 0..3 {
        send(&mut ws, "unsubscribe", "tick.okx.NOSUCH-USDT-SWAP").await;
        next_text(&mut ws, 500).await;
        send(&mut ws, "subscribe", "tick.okx.NOSUCH-USDT-SWAP").await;
        next_text(&mut ws, 500).await;
    }
    tokio::time::sleep(CHECK * 10).await;
    assert_eq!(r.okx.count("subscribe", stream), 1, "退避期内不得重试");
    assert_eq!(r.metric("md_runtime_subscriptions_demand_failed_total"), 1);
    assert_eq!(
        r.metric("md_runtime_subscriptions_demand_subscribed_total"),
        1
    );
}

/// 超过上限 → 不补订、计数；名额空出来后才补订
#[tokio::test]
async fn limit_refuses_on_demand_and_counts() {
    let r = rig(
        Settings {
            max_runtime_subscriptions: 1,
            ..fast_settings()
        },
        &[],
        &[],
    )
    .await;
    r.post_tick("binance", "SOLUSDT").await;
    let sol = r.gateway_ws("tick.binance.SOLUSDT").await;
    let _xrp = r.gateway_ws("tick.binance.XRPUSDT").await;

    tokio::time::sleep(CHECK * 5).await;
    assert_eq!(
        r.binance.count("SUBSCRIBE", "xrpusdt@aggTrade"),
        0,
        "达到上限不得补订"
    );
    assert!(r.metric("md_runtime_subscriptions_limit_rejected_total") >= 1);
    assert_eq!(r.metric("md_runtime_subscriptions"), 1);

    // SOL 的接收方离开 → 闲置退订 → 名额空出 → XRP 补订
    drop(sol);
    assert!(
        wait_until(Duration::from_secs(3), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await
    );
    assert!(
        wait_until(Duration::from_secs(2), || r
            .binance
            .count("SUBSCRIBE", "xrpusdt@aggTrade")
            == 1)
        .await
    );
}

/// DELETE 已被自动退订的目标 → 成功（幂等）
#[tokio::test]
async fn delete_of_auto_unsubscribed_target_succeeds() {
    let r = rig(fast_settings(), &[], &[]).await;
    r.post_tick("binance", "SOLUSDT").await;
    assert!(
        wait_until(Duration::from_secs(3), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await
    );
    let (status, body) = http(
        "DELETE",
        &format!("{}/api/v1/subscriptions", r.base),
        Some(r#"{"exchange":"binance","type":"TICK","symbols":["SOLUSDT"]}"#),
    )
    .await;
    assert_eq!((status, body.as_str()), (200, r#"{"success":true}"#));
    assert!(!r.has_sub("SOLUSDT").await);
}

/// DELETE 一个仍有接收方的目标：按需补订立即补回（不切断其他接收方）
#[tokio::test]
async fn delete_while_receiving_is_restored_on_demand() {
    let r = rig(fast_settings(), &[], &[]).await;
    r.post_tick("binance", "SOLUSDT").await;
    let mut ws = r.gateway_ws("tick.binance.SOLUSDT").await;
    let (status, _) = http(
        "DELETE",
        &format!("{}/api/v1/subscriptions", r.base),
        Some(r#"{"exchange":"binance","type":"TICK","symbols":["SOLUSDT"]}"#),
    )
    .await;
    assert_eq!(status, 200);
    assert!(
        wait_until(Duration::from_secs(2), || r.binance.count("SUBSCRIBE", SOL)
            == 2)
        .await
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    r.binance.push_binance_trade("SOLUSDT", 9);
    assert_eq!(drain(&mut ws, 400).await.len(), 1);
}

/// GET 订阅列表：开关打开时每条追加 source / last_active_at（只追加，已有字段与顺序不变）
#[tokio::test]
async fn get_subscriptions_reports_source_and_last_active_when_enabled() {
    let r = rig(fast_settings(), &["BTCUSDT"], &[]).await;
    r.post_tick("binance", "SOLUSDT").await;
    let _ws = r.gateway_ws("tick.binance.SOLUSDT").await;
    tokio::time::sleep(CHECK * 3).await;

    let raw = r.subscriptions_raw().await;
    assert!(
        raw.contains(r#"{"exchange":"binance","data_type":1,"symbol":"SOLUSDT","source":"runtime","last_active_at":"#),
        "{raw}"
    );
    assert!(
        raw.contains(r#"{"exchange":"binance","data_type":1,"symbol":"BTCUSDT","source":"config","last_active_at":null}"#),
        "从没人用过的订阅 last_active_at 为 null：{raw}"
    );
    let subs: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
    let sol = subs.iter().find(|s| s["symbol"] == "SOLUSDT").unwrap();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!((now_ms - sol["last_active_at"].as_i64().unwrap()).abs() < 2000);
}

/// 开关关闭：不退订、不补订、GET 不追加字段、指标 enabled=0
#[tokio::test]
async fn switch_off_changes_nothing() {
    let r = rig(
        Settings {
            enabled: false,
            ..fast_settings()
        },
        &["BTCUSDT"],
        &[],
    )
    .await;
    r.post_tick("binance", "SOLUSDT").await;
    let _xrp = r.gateway_ws("tick.binance.XRPUSDT").await;
    let _ = http(
        "GET",
        &format!("{}/api/v1/data/latest/tick/binance/SOLUSDT", r.base),
        None,
    )
    .await;

    tokio::time::sleep(IDLE * 4).await;
    assert_eq!(r.binance.count("UNSUBSCRIBE", SOL), 0);
    assert_eq!(r.binance.count("SUBSCRIBE", "xrpusdt@aggTrade"), 0);
    let raw = r.subscriptions_raw().await;
    assert!(
        !raw.contains("source") && !raw.contains("last_active_at"),
        "{raw}"
    );
    let subs: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
    assert_eq!(subs.len(), 2);
    assert_eq!(r.metric("md_runtime_subscriptions_enabled"), 0);
    let s = r.processor.take_usage_signals();
    assert!(
        s.receivers.is_empty() && s.reads.is_empty(),
        "关闭时不收集使用信号"
    );
}

/// 运行中打开 / 关闭（SIGHUP 热更走的就是 update_config）：打开后才开始管理，保护期从打开时算；关闭后立即停手
#[tokio::test]
async fn hot_toggle_takes_effect_without_restart() {
    let off = Settings {
        enabled: false,
        ..fast_settings()
    };
    let r = rig(off.clone(), &[], &[]).await;
    r.post_tick("binance", "SOLUSDT").await;
    tokio::time::sleep(IDLE * 2).await;
    assert_eq!(r.binance.count("UNSUBSCRIBE", SOL), 0);

    let grace = Duration::from_millis(800);
    let enabled_at = Instant::now();
    r.runtime.update_config(ConfigView {
        settings: Settings {
            startup_grace: grace,
            ..fast_settings()
        },
        protected: HashSet::new(),
    });
    assert!(
        wait_until(Duration::from_secs(4), || r
            .binance
            .count("UNSUBSCRIBE", SOL)
            == 1)
        .await
    );
    assert!(enabled_at.elapsed() >= grace, "保护期应从打开时算");
    assert_eq!(r.metric("md_runtime_subscriptions_enabled"), 1);

    r.runtime.update_config(ConfigView {
        settings: off,
        protected: HashSet::new(),
    });
    r.post_tick("binance", "SOLUSDT").await;
    tokio::time::sleep(IDLE * 3).await;
    assert_eq!(r.binance.count("UNSUBSCRIBE", SOL), 1, "关闭后不得再退订");
    assert_eq!(r.metric("md_runtime_subscriptions_enabled"), 0);
}
