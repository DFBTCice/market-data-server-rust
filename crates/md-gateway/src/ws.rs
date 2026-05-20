use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use md_domain::types::{WsGatewayMessage, WsLegacyMessage};
use md_processor::{BroadcastEvent, Processor};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// 单个客户端连续 broadcast lagged 多少次后主动踢出
/// （默认 3 次：偶发拥塞容忍，持续跟不上就视为僵尸客户端）
const LAGGED_KICK_THRESHOLD: u32 = 3;

/// WebSocket 共享状态
#[derive(Clone)]
pub struct WsState {
    pub processor: Arc<Processor>,
    pub metrics: Arc<md_processor::ProcessorMetrics>,
}

/// 客户端请求格式 -- Gateway 模式
#[derive(Deserialize)]
struct GatewayWsRequest {
    action: String,
    streams: Vec<String>,
}

/// 客户端请求格式 -- Legacy 模式
#[derive(Deserialize)]
struct LegacyWsRequest {
    op: String,
    args: Vec<String>,
}

/// 订阅确认响应 -- Gateway 格式
#[derive(Serialize)]
struct GatewayAck {
    action: String,
    status: String,
    stream: String,
}

/// 订阅确认响应 -- Legacy 格式
#[derive(Serialize)]
struct LegacyAck {
    event: String,
    status: String,
    topics: Vec<String>,
}

/// 取消订阅确认 -- Legacy 格式
#[derive(Serialize)]
struct LegacyUnsubAck {
    event: String,
    status: String,
    topics_unsubscribed: String,
}

/// 错误响应
#[derive(Serialize)]
struct WsError {
    error: String,
}

/// 构建 WebSocket 路由
pub fn routes(processor: Arc<Processor>) -> Router {
    let state = WsState {
        metrics: processor.metrics.clone(),
        processor,
    };

    Router::new()
        .route("/ws/v1/data", axum::routing::get(gateway_ws_handler))
        .route("/ws", axum::routing::get(legacy_ws_handler))
        .with_state(state)
}

/// Gateway WebSocket handler -- 对标 Go 版 /ws/v1/data
async fn gateway_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<WsState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_gateway_ws(socket, state.processor, state.metrics))
}

/// Legacy WebSocket handler -- 对标 Go 版 /ws
async fn legacy_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<WsState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_legacy_ws(socket, state.processor, state.metrics))
}

/// Gateway WebSocket 处理
async fn handle_gateway_ws(
    socket: WebSocket,
    processor: Arc<Processor>,
    metrics: Arc<md_processor::ProcessorMetrics>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut subscriptions: Vec<tokio::sync::broadcast::Receiver<BroadcastEvent>> = Vec::new();
    let mut topics: HashSet<String> = HashSet::new();
    let mut lagged_count: u32 = 0;

    metrics.ws_client_connected();
    info!(
        "Gateway WebSocket client connected (active={})",
        metrics
            .ws_active_clients
            .load(std::sync::atomic::Ordering::Relaxed)
    );

    loop {
        tokio::select! {
            // 接收客户端消息
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<GatewayWsRequest>(&text) {
                            Ok(req) => {
                                match req.action.as_str() {
                                    "subscribe" => {
                                        for stream in &req.streams {
                                            if topics.insert(stream.clone()) {
                                                let sub = processor.subscribe(stream);
                                                subscriptions.push(sub.rx);
                                                let ack = GatewayAck {
                                                    action: "subscribe".into(),
                                                    status: "success".into(),
                                                    stream: stream.clone(),
                                                };
                                                let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
                                            }
                                        }
                                    }
                                    "unsubscribe" => {
                                        for stream in &req.streams {
                                            topics.remove(stream);
                                            let ack = GatewayAck {
                                                action: "unsubscribe".into(),
                                                status: "success".into(),
                                                stream: stream.clone(),
                                            };
                                            let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
                                        }
                                    }
                                    _ => {
                                        let err = WsError { error: format!("unknown action: {}", req.action) };
                                        let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                                    }
                                }
                            }
                            Err(e) => {
                                let err = WsError { error: format!("invalid request: {}", e) };
                                let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // 接收广播事件并推送给客户端
            recv_result = recv_from_subscriptions(&mut subscriptions, &topics, &metrics) => {
                match recv_result {
                    RecvOutcome::Event(topic_str, broadcast_event) => {
                        lagged_count = 0; // 成功收到一条就重置 lagged 计数
                        let kind = broadcast_event.kind();
                        let emit_at = broadcast_event.emit_instant();
                        let json = build_gateway_message(&topic_str, &broadcast_event);
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                        metrics.ws_message_sent(kind);
                        let elapsed_ms = emit_at.elapsed().as_millis() as u64;
                        metrics.record_gateway_forward_latency_ms(elapsed_ms);
                    }
                    RecvOutcome::Lagged => {
                        lagged_count = lagged_count.saturating_add(1);
                        if lagged_count >= LAGGED_KICK_THRESHOLD {
                            metrics.ws_client_kicked_lagged();
                            warn!("Gateway WebSocket client kicked: lagged {} consecutive times", lagged_count);
                            break;
                        }
                    }
                    RecvOutcome::Closed | RecvOutcome::None => {}
                }
            }
        }
    }

    metrics.ws_client_disconnected();
    info!(
        "Gateway WebSocket client disconnected (active={})",
        metrics
            .ws_active_clients
            .load(std::sync::atomic::Ordering::Relaxed)
    );
}

/// 接收结果分类（用于上层判断是否需要踢出客户端）
enum RecvOutcome {
    Event(String, BroadcastEvent),
    Lagged,
    Closed,
    /// 收到了事件但 topic 不匹配
    None,
}

/// 从所有 subscription 中接收下一个匹配 topics 的事件
async fn recv_from_subscriptions(
    subscriptions: &mut Vec<tokio::sync::broadcast::Receiver<BroadcastEvent>>,
    topics: &HashSet<String>,
    metrics: &md_processor::ProcessorMetrics,
) -> RecvOutcome {
    if subscriptions.is_empty() {
        std::future::pending::<()>().await;
        return RecvOutcome::None;
    }
    // 构建 boxed futures for select_all
    let futures: Vec<_> = subscriptions
        .iter_mut()
        .map(|rx| Box::pin(async { rx.recv().await }))
        .collect();
    let (result, _idx, _remaining) = futures_util::future::select_all(futures).await;
    match result {
        Ok(event) => {
            // 匹配 topic：遍历已订阅的 topics，找到匹配的
            for topic_str in topics {
                if event_matches_topic(&event, topic_str) {
                    return RecvOutcome::Event(topic_str.clone(), event);
                }
            }
            RecvOutcome::None
        }
        Err(broadcast::error::RecvError::Lagged(n)) => {
            // 记录 lagged 事件（按 topic 类型分类）
            let kind = detect_topic_kind_from_subscriptions(topics);
            metrics.record_broadcast_lagged(kind);
            warn!(
                "broadcast lagged ({} messages dropped), topic_kind={}",
                n, kind
            );
            RecvOutcome::Lagged
        }
        Err(broadcast::error::RecvError::Closed) => RecvOutcome::Closed,
    }
}

/// 判断事件是否匹配给定的 topic（精确匹配，非子串匹配）
fn event_matches_topic(event: &BroadcastEvent, topic_str: &str) -> bool {
    let topic = match md_domain::topic::Topic::parse(topic_str) {
        Ok(t) => t,
        Err(_) => return false,
    };
    match (event, topic) {
        (BroadcastEvent::Tick(tick, _), md_domain::topic::Topic::Tick { exchange, symbol }) => {
            tick.exchange == exchange && tick.symbol == symbol
        }
        (
            BroadcastEvent::Kline(kline, _),
            md_domain::topic::Topic::Kline {
                interval,
                exchange,
                symbol,
            },
        ) => kline.interval == interval && kline.exchange == exchange && kline.symbol == symbol,
        _ => false,
    }
}

/// 从订阅 topic 集合推断 topic 类型（"tick" 或 "kline"），用于 lagged 指标分类
fn detect_topic_kind_from_subscriptions(topics: &HashSet<String>) -> &str {
    for t in topics {
        if t.starts_with("tick.") {
            return "tick";
        }
        if t.starts_with("kline.") {
            return "kline";
        }
    }
    "unknown"
}

/// 从订阅 topic 列表推断 topic 类型（Legacy 模式）
fn detect_topic_kind_from_topics(topics: &[String]) -> &str {
    for t in topics {
        if t.starts_with("tick.") {
            return "tick";
        }
        if t.starts_with("kline.") {
            return "kline";
        }
    }
    "unknown"
}

/// Legacy WebSocket 处理
async fn handle_legacy_ws(
    socket: WebSocket,
    processor: Arc<Processor>,
    metrics: Arc<md_processor::ProcessorMetrics>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut subscriptions: Vec<tokio::sync::broadcast::Receiver<BroadcastEvent>> = Vec::new();
    let mut topics: Vec<String> = Vec::new();
    let mut lagged_count: u32 = 0;

    metrics.ws_client_connected();
    info!(
        "Legacy WebSocket client connected (active={})",
        metrics
            .ws_active_clients
            .load(std::sync::atomic::Ordering::Relaxed)
    );

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<LegacyWsRequest>(&text) {
                            Ok(req) => {
                                match req.op.as_str() {
                                    "subscribe" => {
                                        // Legacy 模式：替换所有订阅
                                        subscriptions.clear();
                                        topics.clear();
                                        for topic in &req.args {
                                            let sub = processor.subscribe(topic);
                                            subscriptions.push(sub.rx);
                                            topics.push(topic.clone());
                                        }
                                        let ack = LegacyAck {
                                            event: "subscribe".into(),
                                            status: "success".into(),
                                            topics: topics.clone(),
                                        };
                                        let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
                                    }
                                    "unsubscribe" => {
                                        subscriptions.clear();
                                        topics.clear();
                                        let ack = LegacyUnsubAck {
                                            event: "unsubscribe".into(),
                                            status: "success".into(),
                                            topics_unsubscribed: "all".into(),
                                        };
                                        let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
                                    }
                                    _ => {
                                        let err = WsError { error: format!("unknown op: {}", req.op) };
                                        let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                                    }
                                }
                            }
                            Err(e) => {
                                let err = WsError { error: format!("invalid request: {}", e) };
                                let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // 接收广播事件并推送给客户端
            recv_result = recv_from_subscriptions_legacy(&mut subscriptions, &topics, &metrics) => {
                match recv_result {
                    RecvOutcome::Event(topic_str, broadcast_event) => {
                        lagged_count = 0;
                        let kind = broadcast_event.kind();
                        let emit_at = broadcast_event.emit_instant();
                        let json = build_legacy_message(&topic_str, &broadcast_event);
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                        metrics.ws_message_sent(kind);
                        let elapsed_ms = emit_at.elapsed().as_millis() as u64;
                        metrics.record_gateway_forward_latency_ms(elapsed_ms);
                    }
                    RecvOutcome::Lagged => {
                        lagged_count = lagged_count.saturating_add(1);
                        if lagged_count >= LAGGED_KICK_THRESHOLD {
                            metrics.ws_client_kicked_lagged();
                            warn!("Legacy WebSocket client kicked: lagged {} consecutive times", lagged_count);
                            break;
                        }
                    }
                    RecvOutcome::Closed | RecvOutcome::None => {}
                }
            }
        }
    }

    metrics.ws_client_disconnected();
    info!(
        "Legacy WebSocket client disconnected (active={})",
        metrics
            .ws_active_clients
            .load(std::sync::atomic::Ordering::Relaxed)
    );
}

/// 从所有 subscription 中接收下一个事件（Legacy 模式，返回第一个匹配的 topic）
async fn recv_from_subscriptions_legacy(
    subscriptions: &mut Vec<tokio::sync::broadcast::Receiver<BroadcastEvent>>,
    topics: &[String],
    metrics: &md_processor::ProcessorMetrics,
) -> RecvOutcome {
    if subscriptions.is_empty() {
        std::future::pending::<()>().await;
        return RecvOutcome::None;
    }
    let futures: Vec<_> = subscriptions
        .iter_mut()
        .map(|rx| Box::pin(async { rx.recv().await }))
        .collect();
    let (result, _idx, _remaining) = futures_util::future::select_all(futures).await;
    match result {
        Ok(event) => {
            for topic_str in topics {
                if event_matches_topic(&event, topic_str) {
                    return RecvOutcome::Event(topic_str.clone(), event);
                }
            }
            RecvOutcome::None
        }
        Err(broadcast::error::RecvError::Lagged(n)) => {
            let kind = detect_topic_kind_from_topics(topics);
            metrics.record_broadcast_lagged(kind);
            warn!(
                "broadcast lagged ({} messages dropped), topic_kind={}",
                n, kind
            );
            RecvOutcome::Lagged
        }
        Err(broadcast::error::RecvError::Closed) => RecvOutcome::Closed,
    }
}

/// 构建 Gateway 格式的推送消息
fn build_gateway_message(topic_str: &str, event: &BroadcastEvent) -> String {
    match event {
        BroadcastEvent::Tick(tick, _) => {
            let msg = WsGatewayMessage {
                msg_type: "tick".into(),
                topic: topic_str.to_string(),
                data: tick.as_ref().clone(),
            };
            serde_json::to_string(&msg).unwrap()
        }
        BroadcastEvent::Kline(kline, _) => {
            let msg = WsGatewayMessage {
                msg_type: "kline".into(),
                topic: topic_str.to_string(),
                data: kline.as_ref().clone(),
            };
            serde_json::to_string(&msg).unwrap()
        }
    }
}

/// 构建 Legacy 格式的推送消息
fn build_legacy_message(topic_str: &str, event: &BroadcastEvent) -> String {
    match event {
        BroadcastEvent::Tick(tick, _) => {
            let msg = WsLegacyMessage {
                topic: topic_str.to_string(),
                data: tick.as_ref().clone(),
            };
            serde_json::to_string(&msg).unwrap()
        }
        BroadcastEvent::Kline(kline, _) => {
            let msg = WsLegacyMessage {
                topic: topic_str.to_string(),
                data: kline.as_ref().clone(),
            };
            serde_json::to_string(&msg).unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use md_domain::types::{Kline, Tick};

    #[test]
    fn gateway_message_format() {
        let tick = Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            price: "67000.50".into(),
            timestamp: 1711929600000,
            ..Default::default()
        };

        let event = BroadcastEvent::Tick(Arc::new(tick), std::time::Instant::now());
        let json = build_gateway_message("tick.binance.BTCUSDT", &event);

        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["type"], "tick");
        assert_eq!(val["topic"], "tick.binance.BTCUSDT");
        assert_eq!(val["data"]["exchange"], "binance");
        assert_eq!(val["data"]["price"], "67000.50");
    }

    #[test]
    fn legacy_message_format() {
        let tick = Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            price: "67000.50".into(),
            ..Default::default()
        };

        let event = BroadcastEvent::Tick(Arc::new(tick), std::time::Instant::now());
        let json = build_legacy_message("tick.binance.BTCUSDT", &event);

        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["topic"], "tick.binance.BTCUSDT");
        assert_eq!(val["data"]["exchange"], "binance");
        // Legacy 格式没有 "type" 字段
        assert!(val.get("type").is_none());
    }

    #[test]
    fn gateway_kline_message_format() {
        let kline = Kline {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            close: "67050.00".into(),
            ..Default::default()
        };

        let event = BroadcastEvent::Kline(Arc::new(kline), std::time::Instant::now());
        let json = build_gateway_message("kline.1m.binance.BTCUSDT", &event);

        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["type"], "kline");
        assert_eq!(val["topic"], "kline.1m.binance.BTCUSDT");
        assert_eq!(val["data"]["interval"], "1m");
    }

    #[test]
    fn legacy_kline_message_format() {
        let kline = Kline {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            close: "67050.00".into(),
            ..Default::default()
        };

        let event = BroadcastEvent::Kline(Arc::new(kline), std::time::Instant::now());
        let json = build_legacy_message("kline.1m.binance.BTCUSDT", &event);

        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["topic"], "kline.1m.binance.BTCUSDT");
        assert!(val.get("type").is_none());
    }

    #[test]
    fn detect_topic_kind_from_subscriptions_works() {
        let mut topics = HashSet::new();
        topics.insert("tick.binance.BTCUSDT".into());
        assert_eq!(detect_topic_kind_from_subscriptions(&topics), "tick");

        let mut topics = HashSet::new();
        topics.insert("kline.1m.binance.BTCUSDT".into());
        assert_eq!(detect_topic_kind_from_subscriptions(&topics), "kline");

        let topics = HashSet::new();
        assert_eq!(detect_topic_kind_from_subscriptions(&topics), "unknown");
    }

    #[test]
    fn detect_topic_kind_from_topics_works() {
        let topics = vec!["tick.binance.BTCUSDT".into()];
        assert_eq!(detect_topic_kind_from_topics(&topics), "tick");

        let topics = vec!["kline.1m.binance.BTCUSDT".into()];
        assert_eq!(detect_topic_kind_from_topics(&topics), "kline");

        let topics: Vec<String> = vec![];
        assert_eq!(detect_topic_kind_from_topics(&topics), "unknown");
    }
}
