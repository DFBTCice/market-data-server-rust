use crate::{Connector, ConnectorError, DataEvent, ExchangeAdapter, SubscriptionTarget, WsCommand};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use md_domain::types::{Kline, Tick};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream};
use tracing::{debug, error, info, warn};

/// 连接超时时间
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// TCP keepalive 参数
const TCP_KEEPALIVE_IDLE: Duration = Duration::from_secs(60);
const TCP_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);
const TCP_KEEPALIVE_RETRIES: u32 = 4;

/// BaseConnector -- 封装公共 WebSocket 管理逻辑
///
/// 泛型参数 A: ExchangeAdapter，定义交易所特定行为
pub struct BaseConnector<A: ExchangeAdapter> {
    adapter: A,
    tick_tx: mpsc::Sender<Tick>,
    kline_tx: mpsc::Sender<Kline>,
    state: Arc<ConnectorState>,
    reconnect_delay: Duration,
    /// 可选的 metrics 计数器（与 Processor 共享）
    ticks_dropped: Option<Arc<AtomicU64>>,
    klines_dropped: Option<Arc<AtomicU64>>,
    /// 连接器级别指标（与 Processor 共享）
    connected: Option<Arc<AtomicU64>>,
    reconnect_total: Option<Arc<AtomicU64>>,
    subscribe_failed_total: Option<Arc<AtomicU64>>,
}

struct ConnectorState {
    /// 期望的订阅列表（用户请求的）
    desired_subs: RwLock<HashSet<SubscriptionTarget>>,
    /// 当前活跃的 stream keys
    active_streams: RwLock<HashSet<String>>,
    /// 断连期间缓存的操作
    pending_ops: Mutex<HashMap<String, PendingOp>>,
    /// 关闭信号
    shutdown_tx: watch::Sender<bool>,
    /// 写命令 channel（连接时设置，断开时清除）
    ws_write_tx: RwLock<Option<mpsc::Sender<WsCommand>>>,
    /// 最后一次收到 pong 的时间
    last_pong_at: Mutex<std::time::Instant>,
}

#[derive(Debug, Clone)]
enum PendingOp {
    Subscribe,
    Unsubscribe,
}

impl<A: ExchangeAdapter> BaseConnector<A> {
    pub fn new(
        adapter: A,
        tick_tx: mpsc::Sender<Tick>,
        kline_tx: mpsc::Sender<Kline>,
        reconnect_delay: Duration,
    ) -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            adapter,
            tick_tx,
            kline_tx,
            state: Arc::new(ConnectorState {
                desired_subs: RwLock::new(HashSet::new()),
                active_streams: RwLock::new(HashSet::new()),
                pending_ops: Mutex::new(HashMap::new()),
                shutdown_tx,
                ws_write_tx: RwLock::new(None),
                last_pong_at: Mutex::new(std::time::Instant::now()),
            }),
            reconnect_delay,
            ticks_dropped: None,
            klines_dropped: None,
            connected: None,
            reconnect_total: None,
            subscribe_failed_total: None,
        }
    }

    /// 设置 metrics 计数器（可选）
    pub fn with_metrics(
        mut self,
        ticks_dropped: Arc<AtomicU64>,
        klines_dropped: Arc<AtomicU64>,
    ) -> Self {
        self.ticks_dropped = Some(ticks_dropped);
        self.klines_dropped = Some(klines_dropped);
        self
    }

    /// 设置完整 metrics（包含连接器级别指标）
    pub fn with_full_metrics(
        mut self,
        ticks_dropped: Arc<AtomicU64>,
        klines_dropped: Arc<AtomicU64>,
        connected: Arc<AtomicU64>,
        reconnect_total: Arc<AtomicU64>,
        subscribe_failed_total: Arc<AtomicU64>,
    ) -> Self {
        self.ticks_dropped = Some(ticks_dropped);
        self.klines_dropped = Some(klines_dropped);
        self.connected = Some(connected);
        self.reconnect_total = Some(reconnect_total);
        self.subscribe_failed_total = Some(subscribe_failed_total);
        self
    }

    /// 主连接循环 -- 对标 Go 版 manageConnection
    /// 固定延迟重连（与 Go 版一致）
    pub async fn run_connection_loop(&self) {
        let mut shutdown_rx = self.state.shutdown_tx.subscribe();
        let adapter_name = self.adapter.name().to_string();

        loop {
            // 检查关闭信号
            if *shutdown_rx.borrow() {
                info!(
                    "[{}] shutdown requested, exiting connection loop",
                    adapter_name
                );
                return;
            }

            info!("[{}] connecting to {}", adapter_name, self.adapter.ws_url());

            // 尝试连接（带超时）
            let ws_stream =
                match tokio::time::timeout(CONNECT_TIMEOUT, connect_async(self.adapter.ws_url()))
                    .await
                {
                    Ok(Ok((stream, _))) => stream,
                    Ok(Err(e)) => {
                        warn!(
                            "[{}] connection failed: {}, retrying in {:?}",
                            adapter_name, e, self.reconnect_delay
                        );
                        if let Some(ref m) = self.connected {
                            m.store(0, Ordering::Relaxed);
                        }
                        if let Some(ref m) = self.reconnect_total {
                            m.fetch_add(1, Ordering::Relaxed);
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(self.reconnect_delay) => {}
                            _ = shutdown_rx.changed() => { return; }
                        }
                        continue;
                    }
                    Err(_) => {
                        warn!(
                            "[{}] connection timeout ({:?}), retrying in {:?}",
                            adapter_name, CONNECT_TIMEOUT, self.reconnect_delay
                        );
                        if let Some(ref m) = self.connected {
                            m.store(0, Ordering::Relaxed);
                        }
                        if let Some(ref m) = self.reconnect_total {
                            m.fetch_add(1, Ordering::Relaxed);
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(self.reconnect_delay) => {}
                            _ = shutdown_rx.changed() => { return; }
                        }
                        continue;
                    }
                };

            // 设置 TCP keepalive
            if let MaybeTlsStream::Plain(ref tcp_stream) = ws_stream.get_ref() {
                if let Err(e) = set_tcp_keepalive(tcp_stream) {
                    warn!("[{}] failed to set TCP keepalive: {}", adapter_name, e);
                }
            }

            let (write, mut read) = ws_stream.split();

            // 创建写多路复用器
            let (ws_write_tx, mut ws_write_rx) = mpsc::channel::<WsCommand>(64);
            {
                let mut tx = self.state.ws_write_tx.write().await;
                *tx = Some(ws_write_tx.clone());
            }

            // 启动写循环任务
            let write_name = adapter_name.clone();
            let write_handle = tokio::spawn(async move {
                let mut write = write;
                while let Some(cmd) = ws_write_rx.recv().await {
                    match cmd {
                        WsCommand::Text(msg) => {
                            if write.send(Message::Text(msg.into())).await.is_err() {
                                warn!("[{}] write failed", write_name);
                                break;
                            }
                        }
                        WsCommand::Ping(data) => {
                            if write.send(Message::Ping(data)).await.is_err() {
                                warn!("[{}] ping write failed", write_name);
                                break;
                            }
                        }
                        WsCommand::Close => {
                            let _ = write.send(Message::Close(None)).await;
                            break;
                        }
                    }
                }
            });

            // 连接成功后同步订阅
            if let Err(e) = self.sync_subscriptions_on_connect(&ws_write_tx).await {
                warn!(
                    "[{}] subscription sync failed: {}, reconnecting",
                    adapter_name, e
                );
                if let Some(ref m) = self.subscribe_failed_total {
                    m.fetch_add(1, Ordering::Relaxed);
                }
                if let Some(ref m) = self.connected {
                    m.store(0, Ordering::Relaxed);
                }
                if let Some(ref m) = self.reconnect_total {
                    m.fetch_add(1, Ordering::Relaxed);
                }
                // 清除写 channel
                {
                    let mut tx = self.state.ws_write_tx.write().await;
                    *tx = None;
                }
                write_handle.abort();
                tokio::select! {
                    _ = tokio::time::sleep(self.reconnect_delay) => {}
                    _ = shutdown_rx.changed() => { return; }
                }
                continue;
            }

            info!("[{}] connected and synced", adapter_name);

            // 设置连接状态为已连接
            if let Some(ref m) = self.connected {
                m.store(1, Ordering::Relaxed);
            }

            // 重置 pong 时间
            {
                let mut last_pong = self.state.last_pong_at.lock().await;
                *last_pong = std::time::Instant::now();
            }

            // 启动心跳
            let ping_interval = self.adapter.ping_interval();
            let heartbeat_msg = self.adapter.heartbeat_message();
            let ws_write_tx_heartbeat = ws_write_tx.clone();
            let heartbeat_name = adapter_name.clone();

            let heartbeat_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(ping_interval);
                loop {
                    interval.tick().await;
                    if let Some(ref msg) = heartbeat_msg {
                        // OKX: 发送文本 "ping"
                        if ws_write_tx_heartbeat
                            .send(WsCommand::Text(msg.clone()))
                            .await
                            .is_err()
                        {
                            warn!("[{}] heartbeat send failed", heartbeat_name);
                            break;
                        }
                    } else {
                        // Binance: 发送协议层 Ping
                        if ws_write_tx_heartbeat
                            .send(WsCommand::Ping(vec![]))
                            .await
                            .is_err()
                        {
                            warn!("[{}] heartbeat send failed", heartbeat_name);
                            break;
                        }
                    }
                }
            });

            // 读消息循环（带 pong 超时）
            let pong_timeout = self.adapter.pong_timeout();
            let read_result = self.read_messages(&mut read, pong_timeout).await;

            // 清理
            heartbeat_handle.abort();
            write_handle.abort();

            // 清除写 channel
            {
                let mut tx = self.state.ws_write_tx.write().await;
                *tx = None;
            }

            // 清空活跃订阅
            {
                let mut active = self.state.active_streams.write().await;
                active.clear();
            }

            match read_result {
                ReadResult::Disconnected => {
                    warn!(
                        "[{}] disconnected, reconnecting in {:?}",
                        adapter_name, self.reconnect_delay
                    );
                }
                ReadResult::Shutdown => {
                    if let Some(ref m) = self.connected {
                        m.store(0, Ordering::Relaxed);
                    }
                    info!("[{}] shutdown signal received", adapter_name);
                    return;
                }
            }

            // 断开连接，准备重连
            if let Some(ref m) = self.connected {
                m.store(0, Ordering::Relaxed);
            }
            if let Some(ref m) = self.reconnect_total {
                m.fetch_add(1, Ordering::Relaxed);
            }

            tokio::select! {
                _ = tokio::time::sleep(self.reconnect_delay) => {}
                _ = shutdown_rx.changed() => { return; }
            }
        }
    }

    /// 读消息循环 -- 带 pong 超时检测
    async fn read_messages<S>(&self, read: &mut S, pong_timeout: Duration) -> ReadResult
    where
        S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        let mut shutdown_rx = self.state.shutdown_tx.subscribe();
        let adapter_name = self.adapter.name().to_string();

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            debug!("[{}] received text ({} bytes)", adapter_name, text.len());

                            // 检测 OKX 文本 pong
                            if text.trim() == "pong" {
                                let mut last_pong = self.state.last_pong_at.lock().await;
                                *last_pong = std::time::Instant::now();
                                debug!("[{}] text pong received", adapter_name);
                                continue;
                            }

                            // 解析消息（带 panic 防御）
                            let parse_result = std::panic::catch_unwind(
                                std::panic::AssertUnwindSafe(|| self.adapter.parse_message(text.as_bytes()))
                            );
                            match parse_result {
                                Ok(Ok(events)) => {
                                    for event in events {
                                        self.dispatch_event(event).await;
                                    }
                                }
                                Ok(Err(e)) => {
                                    warn!("[{}] parse error: {}", adapter_name, e);
                                }
                                Err(_) => {
                                    error!("[{}] panic in parse_message, skipping message", adapter_name);
                                }
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            debug!("[{}] received binary ({} bytes)", adapter_name, data.len());
                            let parse_result = std::panic::catch_unwind(
                                std::panic::AssertUnwindSafe(|| self.adapter.parse_message(&data))
                            );
                            match parse_result {
                                Ok(Ok(events)) => {
                                    for event in events {
                                        self.dispatch_event(event).await;
                                    }
                                }
                                Ok(Err(e)) => {
                                    warn!("[{}] parse error: {}", adapter_name, e);
                                }
                                Err(_) => {
                                    error!("[{}] panic in parse_message, skipping message", adapter_name);
                                }
                            }
                        }
                        Some(Ok(Message::Ping(_))) => {
                            // 自动 Pong 由 tungstenite 处理
                        }
                        Some(Ok(Message::Pong(_))) => {
                            // 更新 pong 时间
                            let mut last_pong = self.state.last_pong_at.lock().await;
                            *last_pong = std::time::Instant::now();
                            debug!("[{}] pong received", adapter_name);
                        }
                        Some(Ok(Message::Close(_))) => {
                            warn!("[{}] received close frame", adapter_name);
                            return ReadResult::Disconnected;
                        }
                        Some(Err(e)) => {
                            warn!("[{}] read error: {}", adapter_name, e);
                            return ReadResult::Disconnected;
                        }
                        None => {
                            return ReadResult::Disconnected;
                        }
                        _ => {}
                    }
                }
                _ = tokio::time::sleep(pong_timeout) => {
                    // 检查是否真的超时
                    let last_pong = self.state.last_pong_at.lock().await;
                    if last_pong.elapsed() > pong_timeout {
                        warn!("[{}] pong timeout ({:?}), reconnecting", adapter_name, pong_timeout);
                        return ReadResult::Disconnected;
                    }
                }
                _ = shutdown_rx.changed() => {
                    return ReadResult::Shutdown;
                }
            }
        }
    }

    /// 分发事件到 channel（带 panic 防御和指标更新）
    async fn dispatch_event(&self, event: DataEvent) {
        let adapter_name = self.adapter.name();
        match event {
            DataEvent::Tick(tick) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if self.tick_tx.try_send(tick).is_err() {
                        warn!("[{}] tick channel full, dropping", adapter_name);
                        return true; // tick dropped
                    }
                    false
                }));
                match result {
                    Ok(true) => {
                        if let Some(ref counter) = self.ticks_dropped {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Ok(false) => {}
                    Err(_) => {
                        error!("[{}] panic in dispatch_event (tick)", adapter_name);
                    }
                }
            }
            DataEvent::Kline(kline) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if self.kline_tx.try_send(kline).is_err() {
                        warn!("[{}] kline channel full, dropping", adapter_name);
                        return true; // kline dropped
                    }
                    false
                }));
                match result {
                    Ok(true) => {
                        if let Some(ref counter) = self.klines_dropped {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Ok(false) => {}
                    Err(_) => {
                        error!("[{}] panic in dispatch_event (kline)", adapter_name);
                    }
                }
            }
            DataEvent::SubscribeError(err) => {
                // 订阅失败：从 active_streams 移除，更新指标
                warn!(
                    "[{}] subscribe error: code={}, stream={}, msg={}",
                    adapter_name, err.code, err.stream, err.message
                );
                if !err.stream.is_empty() {
                    let mut active = self.state.active_streams.write().await;
                    active.remove(&err.stream);
                }
                if let Some(ref counter) = self.subscribe_failed_total {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// 连接后同步订阅 -- 对标 Go 版 syncSubscriptions
    async fn sync_subscriptions_on_connect(
        &self,
        ws_write_tx: &mpsc::Sender<WsCommand>,
    ) -> Result<(), ConnectorError> {
        let adapter_name = self.adapter.name().to_string();

        // 1. 收集需要订阅/取消订阅的 streams
        let mut subs_to_send: Vec<String> = Vec::new();
        let mut unsubs_to_send: Vec<String> = Vec::new();

        // 处理 pending ops
        {
            let mut ops = self.state.pending_ops.lock().await;
            for (stream, op) in ops.drain() {
                match op {
                    PendingOp::Subscribe => subs_to_send.push(stream),
                    PendingOp::Unsubscribe => unsubs_to_send.push(stream),
                }
            }
        }

        // 对比 desired vs active
        let desired = self.state.desired_subs.read().await;
        let active = self.state.active_streams.read().await;

        for target in desired.iter() {
            for stream in self.adapter.target_to_streams(target) {
                if !active.contains(&stream) && !subs_to_send.contains(&stream) {
                    subs_to_send.push(stream);
                }
            }
        }
        drop(desired);
        drop(active);

        // 2. 发送取消订阅消息
        if !unsubs_to_send.is_empty() {
            info!(
                "[{}] unsubscribing from {} streams",
                adapter_name,
                unsubs_to_send.len()
            );
            for stream in &unsubs_to_send {
                let msg = self.adapter.build_unsubscribe_msg(&[stream.clone()]);
                ws_write_tx
                    .send(WsCommand::Text(msg))
                    .await
                    .map_err(|e| ConnectorError::SubscribeFailed(e.to_string()))?;
            }
        }

        // 3. 发送订阅消息
        if !subs_to_send.is_empty() {
            info!(
                "[{}] subscribing to {} streams",
                adapter_name,
                subs_to_send.len()
            );
            let msg = self.adapter.build_subscribe_msg(&subs_to_send);
            ws_write_tx
                .send(WsCommand::Text(msg))
                .await
                .map_err(|e| ConnectorError::SubscribeFailed(e.to_string()))?;

            // 更新活跃订阅
            let mut active = self.state.active_streams.write().await;
            for stream in &subs_to_send {
                active.insert(stream.clone());
            }
        }

        // 4. 处理取消订阅
        if !unsubs_to_send.is_empty() {
            let mut active = self.state.active_streams.write().await;
            for stream in &unsubs_to_send {
                active.remove(stream);
            }
        }

        info!("[{}] subscription sync complete", adapter_name);
        Ok(())
    }
}

enum ReadResult {
    Disconnected,
    Shutdown,
}

/// 设置 TCP keepalive（仅对 Plain TCP 连接有效）
fn set_tcp_keepalive(
    tcp_stream: &tokio::net::TcpStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use socket2::Socket;
    use std::os::unix::io::{AsRawFd, FromRawFd};

    let fd = tcp_stream.as_raw_fd();
    // Safety: 我们只是用 socket2 来设置 socket 选项，不会关闭 fd
    let socket = unsafe { Socket::from_raw_fd(fd) };

    let keepalive = socket2::TcpKeepalive::new()
        .with_time(TCP_KEEPALIVE_IDLE)
        .with_interval(TCP_KEEPALIVE_INTERVAL)
        .with_retries(TCP_KEEPALIVE_RETRIES);

    socket.set_tcp_keepalive(&keepalive)?;

    // 防止 Socket drop 时关闭 fd
    std::mem::forget(socket);

    Ok(())
}

#[async_trait]
impl<A: ExchangeAdapter> Connector for BaseConnector<A> {
    fn name(&self) -> &str {
        self.adapter.name()
    }

    async fn start(&self) -> Result<(), ConnectorError> {
        info!("[{}] starting connector", self.adapter.name());
        Ok(())
    }

    async fn stop(&self) -> Result<(), ConnectorError> {
        info!("[{}] stopping connector", self.adapter.name());
        // 发送关闭帧
        if let Some(tx) = self.state.ws_write_tx.read().await.as_ref() {
            let _ = tx.send(WsCommand::Close).await;
        }
        // 发送 shutdown 信号
        let _ = self.state.shutdown_tx.send(true);
        Ok(())
    }

    async fn add_subscriptions(
        &self,
        targets: Vec<SubscriptionTarget>,
    ) -> Result<(), ConnectorError> {
        let mut desired = self.state.desired_subs.write().await;
        let mut pending = self.state.pending_ops.lock().await;

        let mut streams_to_subscribe = Vec::new();
        for target in targets {
            desired.insert(target.clone());
            for stream in self.adapter.target_to_streams(&target) {
                pending.insert(stream.clone(), PendingOp::Subscribe);
                streams_to_subscribe.push(stream);
            }
        }
        drop(desired);
        drop(pending);

        // 尝试立即发送（如果连接在线）
        if let Some(tx) = self.state.ws_write_tx.read().await.as_ref() {
            let msg = self.adapter.build_subscribe_msg(&streams_to_subscribe);
            if tx.send(WsCommand::Text(msg)).await.is_err() {
                // 连接已断开，pending_ops 会在重连时处理
                debug!(
                    "[{}] immediate subscribe failed (disconnected), will retry on reconnect",
                    self.adapter.name()
                );
            }
        }

        Ok(())
    }

    async fn remove_subscriptions(
        &self,
        targets: Vec<SubscriptionTarget>,
    ) -> Result<(), ConnectorError> {
        let mut desired = self.state.desired_subs.write().await;
        let mut pending = self.state.pending_ops.lock().await;

        let mut streams_to_unsubscribe = Vec::new();
        for target in targets {
            desired.remove(&target);
            for stream in self.adapter.target_to_streams(&target) {
                pending.insert(stream.clone(), PendingOp::Unsubscribe);
                streams_to_unsubscribe.push(stream);
            }
        }
        drop(desired);
        drop(pending);

        // 尝试立即发送（如果连接在线）
        if let Some(tx) = self.state.ws_write_tx.read().await.as_ref() {
            let msg = self.adapter.build_unsubscribe_msg(&streams_to_unsubscribe);
            if tx.send(WsCommand::Text(msg)).await.is_err() {
                debug!(
                    "[{}] immediate unsubscribe failed (disconnected), will retry on reconnect",
                    self.adapter.name()
                );
            }
        }

        Ok(())
    }

    fn current_subscriptions(&self) -> Vec<SubscriptionTarget> {
        match self.state.desired_subs.try_read() {
            Ok(desired) => desired.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }
}
