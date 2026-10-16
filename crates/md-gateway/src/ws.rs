use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use md_domain::types::{WsGatewayMessage, WsLegacyMessage};
use md_processor::{BroadcastEvent, Processor, WsSubscriptionGuard};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamMap};
use tracing::{debug, info, warn};

/// 单个客户端连续 broadcast lagged 多少次后主动踢出
/// （默认 3 次：偶发拥塞容忍，持续跟不上就视为僵尸客户端）
const LAGGED_KICK_THRESHOLD: u32 = 3;

/// WebSocket 共享状态
#[derive(Clone)]
pub struct WsState {
    pub processor: Arc<Processor>,
    pub metrics: Arc<md_processor::ProcessorMetrics>,
    /// Gateway WS 单次写超时（`api_gateway.ws_write_wait`）；`None` = 不限。Legacy `/ws` 不使用。
    pub write_wait: Option<Duration>,
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
    routes_with_write_wait(processor, None)
}

/// 构建 WebSocket 路由，并为 Gateway（`/ws/v1/data`）指定写超时（1.1.1）。
/// 客户端停读/半死时，一次写入阻塞超过 `write_wait` 就断开该连接（释放其全部 broadcast 接收端）。
/// `None` = 不限（与 1.0.0/1.1.0 相同）。Legacy `/ws` 不受影响。
pub fn routes_with_write_wait(processor: Arc<Processor>, write_wait: Option<Duration>) -> Router {
    let state = WsState {
        metrics: processor.metrics.clone(),
        processor,
        write_wait,
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
    ws.on_upgrade(move |socket| {
        handle_gateway_ws(socket, state.processor, state.metrics, state.write_wait)
    })
}

/// Legacy WebSocket handler -- 对标 Go 版 /ws
async fn legacy_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<WsState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_legacy_ws(socket, state.processor, state.metrics))
}

/// 写一条消息；写超时则记录并踢出连接（`break $label`）。对端已关闭时与 1.0.0 一致：忽略，由下一轮 `receiver.next()` 发现并退出。
macro_rules! send_or_kick {
    ($sender:expr, $text:expr, $write_wait:expr, $metrics:expr, $label:lifetime) => {
        if matches!(
            send_text(&mut $sender, $text, $write_wait).await,
            SendResult::TimedOut
        ) {
            kick_write_timeout(&$metrics, $write_wait);
            break $label;
        }
    };
}

/// Gateway WebSocket 处理
async fn handle_gateway_ws(
    socket: WebSocket,
    processor: Arc<Processor>,
    metrics: Arc<md_processor::ProcessorMetrics>,
    write_wait: Option<Duration>,
) {
    let (mut sender, mut receiver) = socket.split();
    // 1.1.1：每个已订阅 stream 恰好持有一个接收端（以客户端传入的 stream 字符串为键）。
    // 退订 = 从 map 移除 = Receiver 立即释放；重复订阅不新增接收端；连接结束整体释放。
    let mut subscriptions: StreamMap<String, TopicStream> = StreamMap::new();
    // 全局 md_ws_subscriptions_active 的 RAII 守卫：连接任何方式结束都会把本连接的份额扣回
    let mut sub_guard = WsSubscriptionGuard::new(metrics.clone());
    let mut lagged_count: u32 = 0;

    metrics.ws_client_connected();
    info!(
        "Gateway WebSocket client connected (active={})",
        metrics
            .ws_active_clients
            .load(std::sync::atomic::Ordering::Relaxed)
    );

    'conn: loop {
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
                                            if !subscriptions.contains_key(stream) {
                                                let sub = processor.subscribe(stream);
                                                subscriptions.insert(stream.clone(), TopicStream::new(stream, sub.rx));
                                                sub_guard.sync(subscriptions.len());
                                                debug!(
                                                    "gateway ws subscribe stream={} conn_subscriptions={} total_active={}",
                                                    stream,
                                                    subscriptions.len(),
                                                    metrics.ws_subscriptions_active.load(std::sync::atomic::Ordering::Relaxed)
                                                );
                                                let ack = GatewayAck {
                                                    action: "subscribe".into(),
                                                    status: "success".into(),
                                                    stream: stream.clone(),
                                                };
                                                send_or_kick!(sender, serde_json::to_string(&ack).unwrap(), write_wait, metrics, 'conn);
                                            }
                                        }
                                    }
                                    "unsubscribe" => {
                                        for stream in &req.streams {
                                            // 1.1.1：真正释放该 stream 的接收端（此前只删了 topics 集合，Receiver 泄漏且重订后事件重复推送）
                                            let released = subscriptions.remove(stream).is_some();
                                            sub_guard.sync(subscriptions.len());
                                            debug!(
                                                "gateway ws unsubscribe stream={} released={} conn_subscriptions={} total_active={}",
                                                stream,
                                                released,
                                                subscriptions.len(),
                                                metrics.ws_subscriptions_active.load(std::sync::atomic::Ordering::Relaxed)
                                            );
                                            let ack = GatewayAck {
                                                action: "unsubscribe".into(),
                                                status: "success".into(),
                                                stream: stream.clone(),
                                            };
                                            send_or_kick!(sender, serde_json::to_string(&ack).unwrap(), write_wait, metrics, 'conn);
                                        }
                                    }
                                    _ => {
                                        let err = WsError { error: format!("unknown action: {}", req.action) };
                                        send_or_kick!(sender, serde_json::to_string(&err).unwrap(), write_wait, metrics, 'conn);
                                    }
                                }
                            }
                            Err(e) => {
                                let err = WsError { error: format!("invalid request: {}", e) };
                                send_or_kick!(sender, serde_json::to_string(&err).unwrap(), write_wait, metrics, 'conn);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // 接收广播事件并推送给客户端
            recv_result = recv_from_subscriptions(&mut subscriptions, &metrics) => {
                match recv_result {
                    RecvOutcome::Event(topic_str, broadcast_event) => {
                        lagged_count = 0; // 成功收到一条就重置 lagged 计数
                        let kind = broadcast_event.kind();
                        let emit_at = broadcast_event.emit_instant();
                        let json = build_gateway_message(&topic_str, &broadcast_event);
                        match send_text(&mut sender, json, write_wait).await {
                            SendResult::Ok => {}
                            SendResult::Closed => break 'conn,
                            SendResult::TimedOut => {
                                kick_write_timeout(&metrics, write_wait);
                                break 'conn;
                            }
                        }
                        metrics.ws_message_sent(kind);
                        let elapsed_ms = emit_at.elapsed().as_millis() as u64;
                        metrics.record_gateway_forward_latency_ms(&topic_str, elapsed_ms);
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

    // 释放本连接全部接收端（显式 drop，使 total_active 在日志里立即反映）
    drop(subscriptions);
    sub_guard.sync(0);
    metrics.ws_client_disconnected();
    info!(
        "Gateway WebSocket client disconnected (active={})",
        metrics
            .ws_active_clients
            .load(std::sync::atomic::Ordering::Relaxed)
    );
}

/// 一次写入的结果
enum SendResult {
    Ok,
    /// 对端已关闭 / 写失败
    Closed,
    /// 写入阻塞超过 `write_wait`（客户端停读或半死）
    TimedOut,
}

/// 向客户端写一条文本消息；`write_wait` 为 `Some` 时给这次写入加超时。
/// 超时后调用方必须关闭连接（被取消的 `send` 可能留下半帧）。
async fn send_text<S>(sender: &mut S, text: String, write_wait: Option<Duration>) -> SendResult
where
    S: futures_util::Sink<Message> + Unpin,
{
    let fut = sender.send(Message::Text(text.into()));
    match write_wait {
        None => match fut.await {
            Ok(()) => SendResult::Ok,
            Err(_) => SendResult::Closed,
        },
        Some(d) => match tokio::time::timeout(d, fut).await {
            Ok(Ok(())) => SendResult::Ok,
            Ok(Err(_)) => SendResult::Closed,
            Err(_) => SendResult::TimedOut,
        },
    }
}

fn kick_write_timeout(metrics: &md_processor::ProcessorMetrics, write_wait: Option<Duration>) {
    metrics.ws_client_kicked_write_timeout();
    warn!(
        "Gateway WebSocket client kicked: write blocked longer than ws_write_wait ({:?}); \
         client is not reading (stalled or half-open)",
        write_wait.unwrap_or_default()
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

/// 单个 stream 的接收端：持有预解析的 `Topic`，只放行与该 topic 精确匹配的事件
/// （与 1.0.0 的逐事件 `event_matches_topic` 语义完全一致，但解析只在订阅时做一次）。
struct TopicStream {
    topic: Option<md_domain::topic::Topic>,
    inner: BroadcastStream<BroadcastEvent>,
}

impl TopicStream {
    fn new(stream: &str, rx: broadcast::Receiver<BroadcastEvent>) -> Self {
        Self {
            topic: md_domain::topic::Topic::parse(stream).ok(),
            inner: BroadcastStream::new(rx),
        }
    }
}

impl Stream for TopicStream {
    type Item = Result<BroadcastEvent, BroadcastStreamRecvError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(event))) => {
                    let matched = match self.topic.as_ref() {
                        Some(t) => event_matches_parsed_topic(&event, t),
                        None => false, // 无法解析的 topic 永不匹配（与 1.0.0 一致：ack 成功但无事件）
                    };
                    if matched {
                        return Poll::Ready(Some(Ok(event)));
                    }
                    // 不匹配：丢弃并继续取下一条
                }
                other => return other,
            }
        }
    }
}

/// 从所有已订阅 stream 中接收下一个事件。
///
/// 1.1.1：使用 `StreamMap`，每事件无 `Vec<Box<Future>>` 重建、无 `select_all`、无 `Topic::parse`。
async fn recv_from_subscriptions(
    subscriptions: &mut StreamMap<String, TopicStream>,
    metrics: &md_processor::ProcessorMetrics,
) -> RecvOutcome {
    if subscriptions.is_empty() {
        std::future::pending::<()>().await;
        return RecvOutcome::None;
    }
    match StreamExt::next(subscriptions).await {
        Some((topic_str, Ok(event))) => RecvOutcome::Event(topic_str, event),
        Some((topic_str, Err(BroadcastStreamRecvError::Lagged(n)))) => {
            // 记录 lagged 事件（按该 stream 的 topic 类型分类）
            let kind = topic_kind_from_stream(&topic_str);
            metrics.record_broadcast_lagged(kind);
            warn!(
                "broadcast lagged ({} messages dropped), topic_kind={}",
                n, kind
            );
            RecvOutcome::Lagged
        }
        // 所有流都已结束（channel 关闭）：StreamMap 已自动移除，外层循环会回到 pending
        None => RecvOutcome::Closed,
    }
}

/// 由单个 stream 字符串推断 topic 类型（"tick" / "kline" / "quote"），用于 lagged 指标分类
fn topic_kind_from_stream(stream: &str) -> &'static str {
    if stream.starts_with("tick.") {
        "tick"
    } else if stream.starts_with("kline.") {
        "kline"
    } else if stream.starts_with("quote.") {
        "quote"
    } else {
        "unknown"
    }
}

/// 判断事件是否匹配给定的 topic（精确匹配，非子串匹配）
fn event_matches_topic(event: &BroadcastEvent, topic_str: &str) -> bool {
    match md_domain::topic::Topic::parse(topic_str) {
        Ok(t) => event_matches_parsed_topic(event, &t),
        Err(_) => false,
    }
}

/// 同上，但 topic 已解析（Gateway 在订阅时解析一次，避免逐事件 `Topic::parse`）
fn event_matches_parsed_topic(event: &BroadcastEvent, topic: &md_domain::topic::Topic) -> bool {
    use md_domain::topic::Topic;
    match (event, topic) {
        (BroadcastEvent::Tick(tick, _), Topic::Tick { exchange, symbol }) => {
            tick.exchange == *exchange && tick.symbol == *symbol
        }
        (
            BroadcastEvent::Kline(kline, _),
            Topic::Kline {
                interval,
                exchange,
                symbol,
            },
        ) => kline.interval == *interval && kline.exchange == *exchange && kline.symbol == *symbol,
        (BroadcastEvent::Quote(quote, _), Topic::Quote { exchange, symbol }) => {
            quote.exchange == *exchange && quote.symbol == *symbol
        }
        _ => false,
    }
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
        // Legacy 模式不支持 quote（订阅时已过滤），此处无需处理 quote. 前缀
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
    // 1.1.1：仅做 md_ws_subscriptions_active 计数（Legacy 订阅/退订语义保持 1.0.0 不变）
    let mut sub_guard = WsSubscriptionGuard::new(metrics.clone());

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
                                            // quote.* 仅 Gateway 模式（/ws/v1/data）支持；Legacy 明确拒绝，
                                            // 避免旧消费者收到不认识的消息，也避免静默吞订阅
                                            if is_quote_topic(topic) {
                                                let err = WsError {
                                                    error: format!(
                                                        "quote topics are not supported on legacy /ws, use /ws/v1/data: {}",
                                                        topic
                                                    ),
                                                };
                                                let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap())).await;
                                                continue;
                                            }
                                            let sub = processor.subscribe(topic);
                                            subscriptions.push(sub.rx);
                                            topics.push(topic.clone());
                                        }
                                        sub_guard.sync(subscriptions.len());
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
                                        sub_guard.sync(0);
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
                        let json = match legacy_payload(&topic_str, &broadcast_event) {
                            Some(j) => j,
                            // quote 事件不得发给 Legacy 客户端（双保险：订阅时已过滤）
                            None => continue,
                        };
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                        metrics.ws_message_sent(kind);
                        let elapsed_ms = emit_at.elapsed().as_millis() as u64;
                        metrics.record_gateway_forward_latency_ms(&topic_str, elapsed_ms);
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
        BroadcastEvent::Quote(quote, _) => {
            let msg = WsGatewayMessage {
                msg_type: "quote".into(),
                topic: topic_str.to_string(),
                data: quote.as_ref().clone(),
            };
            serde_json::to_string(&msg).unwrap()
        }
    }
}

/// 构建 Legacy 格式的推送消息（签名与 1.0.0 一致）
///
/// 注意：Legacy `/ws` 永不推送 quote。调用方必须经 `legacy_payload` 过滤；
/// 此处 Quote 分支仅为 match 穷尽，不会被 Legacy 发送路径触达。
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
        BroadcastEvent::Quote(quote, _) => {
            let msg = WsLegacyMessage {
                topic: topic_str.to_string(),
                data: quote.as_ref().clone(),
            };
            serde_json::to_string(&msg).unwrap()
        }
    }
}

/// Legacy 发送路径的唯一出口：quote 事件返回 `None`（旧消费者无法过滤主题，
/// 任务书 §三-3 要求旧 `/ws` 不得出现 quote 消息），其余与 `build_legacy_message` 相同。
fn legacy_payload(topic_str: &str, event: &BroadcastEvent) -> Option<String> {
    match event {
        BroadcastEvent::Quote(..) => None,
        _ => Some(build_legacy_message(topic_str, event)),
    }
}

/// 该 topic 是否为 quote 主题（Legacy `/ws` 不支持）
fn is_quote_topic(topic_str: &str) -> bool {
    topic_str.starts_with("quote.")
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
    fn topic_kind_from_stream_works() {
        assert_eq!(topic_kind_from_stream("tick.binance.BTCUSDT"), "tick");
        assert_eq!(topic_kind_from_stream("kline.1m.binance.BTCUSDT"), "kline");
        assert_eq!(topic_kind_from_stream("quote.binance.BNBUSDT"), "quote");
        assert_eq!(topic_kind_from_stream("whatever"), "unknown");
        assert_eq!(topic_kind_from_stream(""), "unknown");
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

    // ---- quote（1.1.0 新增）----

    #[test]
    fn gateway_quote_message_format() {
        let quote = md_domain::types::Quote {
            exchange: "okx".into(),
            symbol: "BNB-USDT-SWAP".into(),
            bid_price: "744".into(),
            ask_price: "744.1".into(),
            last_price: "744".into(),
            ..Default::default()
        };
        let event = BroadcastEvent::Quote(Arc::new(quote), std::time::Instant::now());
        let json = build_gateway_message("quote.okx.BNB-USDT-SWAP", &event);
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["type"], "quote");
        assert_eq!(val["topic"], "quote.okx.BNB-USDT-SWAP");
        assert_eq!(val["data"]["bid_price"], "744");
        assert_eq!(val["data"]["last_price"], "744");
    }

    /// 兼容约束 3：Legacy 发送出口对 quote 恒为 None；tick/kline 与 1.0.0 一致
    #[test]
    fn legacy_payload_never_emits_quote() {
        let quote_ev = BroadcastEvent::Quote(
            Arc::new(md_domain::types::Quote::default()),
            std::time::Instant::now(),
        );
        assert!(legacy_payload("quote.binance.BNBUSDT", &quote_ev).is_none());

        let tick_ev = BroadcastEvent::Tick(
            Arc::new(Tick {
                exchange: "binance".into(),
                symbol: "BNBUSDT".into(),
                ..Default::default()
            }),
            std::time::Instant::now(),
        );
        let json = legacy_payload("tick.binance.BNBUSDT", &tick_ev).expect("tick must pass");
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(val.get("type").is_none());
        assert_eq!(val["topic"], "tick.binance.BNBUSDT");
    }

    #[test]
    fn event_matches_quote_topic_exactly() {
        let ev = BroadcastEvent::Quote(
            Arc::new(md_domain::types::Quote {
                exchange: "binance".into(),
                symbol: "BNBUSDT".into(),
                ..Default::default()
            }),
            std::time::Instant::now(),
        );
        assert!(event_matches_topic(&ev, "quote.binance.BNBUSDT"));
        assert!(!event_matches_topic(&ev, "quote.binance.BTCUSDT"));
        assert!(!event_matches_topic(&ev, "quote.okx.BNBUSDT"));
        // quote 事件不会匹配 tick 主题（反之亦然）
        assert!(!event_matches_topic(&ev, "tick.binance.BNBUSDT"));
    }

    // ---- 1.1.1：TopicStream（逐 stream 接收端）语义 ----

    fn tick_event(exchange: &str, symbol: &str, id: i64) -> BroadcastEvent {
        BroadcastEvent::Tick(
            Arc::new(Tick {
                exchange: exchange.into(),
                symbol: symbol.into(),
                trade_id: id,
                ..Default::default()
            }),
            std::time::Instant::now(),
        )
    }

    /// 与 1.0.0 的 `event_matches_topic` 语义一致：只放行精确匹配的事件，其余静默丢弃
    #[tokio::test]
    async fn topic_stream_filters_non_matching_events_like_1_0_0() {
        let (tx, rx) = broadcast::channel(16);
        let mut map: StreamMap<String, TopicStream> = StreamMap::new();
        map.insert(
            "tick.binance.BTCUSDT".into(),
            TopicStream::new("tick.binance.BTCUSDT", rx),
        );
        tx.send(tick_event("binance", "ETHUSDT", 1)).unwrap(); // symbol 不同
        tx.send(tick_event("okx", "BTCUSDT", 2)).unwrap(); // 交易所不同
        tx.send(tick_event("binance", "BTCUSDT", 3)).unwrap(); // 匹配
        let (key, ev) = StreamExt::next(&mut map).await.unwrap();
        assert_eq!(key, "tick.binance.BTCUSDT");
        match ev.unwrap() {
            BroadcastEvent::Tick(t, _) => assert_eq!(t.trade_id, 3),
            _ => panic!("unexpected event kind"),
        }
    }

    /// 无法解析的 topic：永不匹配（ack 成功但无事件，与 1.0.0 一致）
    #[tokio::test]
    async fn topic_stream_with_unparseable_topic_never_matches() {
        let (tx, rx) = broadcast::channel(16);
        let mut map: StreamMap<String, TopicStream> = StreamMap::new();
        map.insert("garbage".into(), TopicStream::new("garbage", rx));
        tx.send(tick_event("binance", "BTCUSDT", 1)).unwrap();
        let r = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            StreamExt::next(&mut map),
        )
        .await;
        assert!(r.is_err(), "不应产出任何事件");
    }

    /// 移除 stream 即释放接收端（receiver_count 归零），这是 A1 的最直接判据
    #[tokio::test]
    async fn removing_stream_releases_receiver() {
        let (tx, rx) = broadcast::channel::<BroadcastEvent>(16);
        let mut map: StreamMap<String, TopicStream> = StreamMap::new();
        map.insert(
            "tick.binance.BTCUSDT".into(),
            TopicStream::new("tick.binance.BTCUSDT", rx),
        );
        assert_eq!(tx.receiver_count(), 1);
        assert!(map.remove("tick.binance.BTCUSDT").is_some());
        assert_eq!(tx.receiver_count(), 0);
        // 对未订阅的 stream 移除是无害的
        assert!(map.remove("tick.binance.BTCUSDT").is_none());
    }

    /// 发布端落后时，Lagged 经 StreamMap 透传（供 lagged 指标 + 踢人逻辑使用）
    #[tokio::test]
    async fn topic_stream_surfaces_lagged() {
        let (tx, rx) = broadcast::channel(2);
        let mut map: StreamMap<String, TopicStream> = StreamMap::new();
        map.insert(
            "tick.binance.BTCUSDT".into(),
            TopicStream::new("tick.binance.BTCUSDT", rx),
        );
        for i in 0..5 {
            tx.send(tick_event("binance", "BTCUSDT", i)).unwrap();
        }
        let (_, r) = StreamExt::next(&mut map).await.unwrap();
        assert!(matches!(r, Err(BroadcastStreamRecvError::Lagged(_))));
    }
}
