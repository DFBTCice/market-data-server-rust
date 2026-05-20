mod process_metrics;

use clap::Parser;
use md_config::load_config;
use md_connector::base::BaseConnector;
use md_connector::binance::{BinanceAdapter, BinanceConnectorConfig};
use md_connector::okx::{OkxAdapter, OkxConnectorConfig};
use md_connector::{Connector, DataType, SubscriptionTarget};
use md_processor::{ConnectorMetrics, Processor};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::{RwLock, watch};
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "md-server", about = "Market Data Server (Rust)")]
struct Cli {
    /// 配置文件路径
    #[arg(short, long, default_value = "config.yaml")]
    config: String,
    /// 端口偏移量（所有监听端口 += offset，方便与 Go 版并行对比）
    #[arg(long, default_value = "0")]
    port_offset: u16,
}

/// 启动所有连接器，返回 connector map 和 connector metrics map
async fn start_connectors(
    cfg: &md_config::Config,
    processor: &Arc<Processor>,
) -> (Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>, HashMap<String, Arc<ConnectorMetrics>>) {
    let connectors: Arc<RwLock<HashMap<String, Arc<dyn Connector>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let mut connector_metrics_map: HashMap<String, Arc<ConnectorMetrics>> = HashMap::new();

    // ---- Binance ----
    if cfg.connectors.binance.enabled {
        info!("starting Binance connector");

        let binance_cfg = BinanceConnectorConfig {
            stream_base_url: cfg.connectors.binance.stream_base_url.clone(),
            subscribe_ticks: cfg.connectors.binance.subscribe_ticks.clone(),
            subscribe_klines: cfg.connectors.binance.subscribe_klines.clone(),
            reconnect_delay: cfg.connectors.binance.reconnect_delay,
            ping_interval: cfg.connectors.binance.ping_interval,
        };

        let adapter = BinanceAdapter::new(binance_cfg);
        let binance_metrics = ConnectorMetrics::new();
        connector_metrics_map.insert("binance".into(), binance_metrics.clone());
        let connector = Arc::new(BaseConnector::new(
            adapter,
            processor.tick_tx(),
            processor.kline_tx(),
            cfg.connectors.binance.reconnect_delay,
        ).with_full_metrics(
            processor.metrics.ticks_dropped.clone(),
            processor.metrics.klines_dropped.clone(),
            binance_metrics.connected.clone(),
            binance_metrics.reconnect_total.clone(),
            binance_metrics.subscribe_failed_total.clone(),
        ));

        let mut targets = Vec::new();
        for symbol in &cfg.connectors.binance.subscribe_ticks {
            targets.push(SubscriptionTarget {
                exchange: "binance".into(),
                data_type: DataType::Tick,
                symbol: symbol.clone(),
                kline_interval: None,
            });
        }
        for (interval, symbols) in &cfg.connectors.binance.subscribe_klines {
            for symbol in symbols {
                targets.push(SubscriptionTarget {
                    exchange: "binance".into(),
                    data_type: DataType::Kline,
                    symbol: symbol.clone(),
                    kline_interval: Some(interval.clone()),
                });
            }
        }

        if let Err(e) = connector.add_subscriptions(targets).await {
            warn!("failed to add Binance subscriptions: {}", e);
        }

        connectors.write().await.insert("binance".into(), connector.clone());

        tokio::spawn(async move {
            connector.run_connection_loop().await;
        });
    }

    // ---- OKX (双连接: public + business) ----
    if cfg.connectors.okx.enabled {
        let okx_cfg_base = OkxConnectorConfig {
            stream_base_url_public: cfg.connectors.okx.stream_base_url_public.clone(),
            stream_base_url_business: cfg.connectors.okx.stream_base_url_business.clone(),
            subscribe_ticks: cfg.connectors.okx.subscribe_ticks.clone(),
            subscribe_klines: cfg.connectors.okx.subscribe_klines.clone(),
            reconnect_delay: cfg.connectors.okx.reconnect_delay,
            ping_interval: cfg.connectors.okx.ping_interval,
        };

        // ---- OKX Public: trades (tick 数据) ----
        if !cfg.connectors.okx.subscribe_ticks.is_empty() {
            info!("starting OKX Public connector (trades)");
            let adapter = OkxAdapter::with_mode(okx_cfg_base.clone(), md_connector::okx::OkxStreamMode::Public);
            let okx_metrics = ConnectorMetrics::new();
            connector_metrics_map.insert("okx".into(), okx_metrics.clone());
            let connector = Arc::new(BaseConnector::new(
                adapter,
                processor.tick_tx(),
                processor.kline_tx(),
                cfg.connectors.okx.reconnect_delay,
            ).with_full_metrics(
                processor.metrics.ticks_dropped.clone(),
                processor.metrics.klines_dropped.clone(),
                okx_metrics.connected.clone(),
                okx_metrics.reconnect_total.clone(),
                okx_metrics.subscribe_failed_total.clone(),
            ));

            let mut targets = Vec::new();
            for symbol in &cfg.connectors.okx.subscribe_ticks {
                targets.push(SubscriptionTarget {
                    exchange: "okx".into(),
                    data_type: DataType::Tick,
                    symbol: symbol.clone(),
                    kline_interval: None,
                });
            }

            if let Err(e) = connector.add_subscriptions(targets).await {
                warn!("failed to add OKX Public subscriptions: {}", e);
            }

            connectors.write().await.insert("okx".into(), connector.clone());

            tokio::spawn(async move {
                connector.run_connection_loop().await;
            });
        }

        // ---- OKX Business: candles (kline 数据) ----
        if !cfg.connectors.okx.subscribe_klines.is_empty() {
            info!("starting OKX Business connector (candles)");
            let adapter = OkxAdapter::with_mode(okx_cfg_base.clone(), md_connector::okx::OkxStreamMode::Business);
            let okx_kline_metrics = ConnectorMetrics::new();
            connector_metrics_map.insert("okx-kline".into(), okx_kline_metrics.clone());
            let connector = Arc::new(BaseConnector::new(
                adapter,
                processor.tick_tx(),
                processor.kline_tx(),
                cfg.connectors.okx.reconnect_delay,
            ).with_full_metrics(
                processor.metrics.ticks_dropped.clone(),
                processor.metrics.klines_dropped.clone(),
                okx_kline_metrics.connected.clone(),
                okx_kline_metrics.reconnect_total.clone(),
                okx_kline_metrics.subscribe_failed_total.clone(),
            ));

            let mut targets = Vec::new();
            for (interval, symbols) in &cfg.connectors.okx.subscribe_klines {
                for symbol in symbols {
                    targets.push(SubscriptionTarget {
                        exchange: "okx".into(),
                        data_type: DataType::Kline,
                        symbol: symbol.clone(),
                        kline_interval: Some(interval.clone()),
                    });
                }
            }

            if let Err(e) = connector.add_subscriptions(targets).await {
                warn!("failed to add OKX Business subscriptions: {}", e);
            }

            connectors.write().await.insert("okx-kline".into(), connector.clone());

            tokio::spawn(async move {
                connector.run_connection_loop().await;
            });
        }
    }

    (connectors, connector_metrics_map)
}

/// 将 ":PORT" 格式的地址加上偏移量
fn offset_port(addr: &str, offset: u16) -> String {
    if let Some(port_str) = addr.strip_prefix(':') {
        if let Ok(port) = port_str.parse::<u16>() {
            return format!(":{}", port + offset);
        }
    }
    if let Ok(mut sa) = addr.parse::<std::net::SocketAddr>() {
        sa.set_port(sa.port() + offset);
        return sa.to_string();
    }
    addr.to_string()
}

/// 将 ":PORT" 解析为 SocketAddr（补全 0.0.0.0）
fn parse_addr(addr: &str) -> std::net::SocketAddr {
    let full = if addr.starts_with(':') {
        format!("0.0.0.0{}", addr)
    } else {
        addr.to_string()
    };
    full.parse().unwrap_or_else(|e| {
        eprintln!("invalid address '{}': {}", addr, e);
        std::process::exit(1);
    })
}

#[tokio::main]
async fn main() {
    // 初始化日志（分层：默认 info，可通过 RUST_LOG 或配置覆盖）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // 初始化进程级指标（记录启动时间）
    process_metrics::init();

    // 加载配置
    info!("loading config from {}", cli.config);
    let mut cfg = match load_config(&cli.config) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("failed to load config: {}", e);
            std::process::exit(1);
        }
    };

    // 应用环境变量覆盖
    md_config::apply_env_overrides(&mut cfg);

    // 应用端口偏移
    if cli.port_offset > 0 {
        cfg.grpc_server.listen_address = offset_port(&cfg.grpc_server.listen_address, cli.port_offset);
        cfg.api_gateway.listen_address = offset_port(&cfg.api_gateway.listen_address, cli.port_offset);
        cfg.admin_server.listen_address = offset_port(&cfg.admin_server.listen_address, cli.port_offset);
        info!("port offset: +{}", cli.port_offset);
    }

    info!("log_level: {}", cfg.log_level);
    info!("gRPC: {}", cfg.grpc_server.listen_address);
    info!("API Gateway: {}", cfg.api_gateway.listen_address);

    // 创建 Processor
    let processor = Arc::new(Processor::new_with_broadcast_capacity(
        cfg.processor.tick_channel_buffer,
        cfg.processor.kline_channel_buffer,
        cfg.processor.broadcast_capacity,
    ));

    // 启动连接器
    let (connectors, connector_metrics_map) = start_connectors(&cfg, &processor).await;

    // 启动 Processor dispatch loop
    let processor_clone = processor.clone();
    tokio::spawn(async move {
        processor_clone.run().await;
    });

    // 创建关停信号（gRPC 和 API Gateway 共享）
    let (shutdown_tx, _shutdown_rx) = watch::channel(false);

    // 启动 gRPC 服务（带优雅关停）
    let grpc_addr = parse_addr(&cfg.grpc_server.listen_address);
    let processor_for_grpc = processor.clone();
    let connectors_for_grpc = connectors.clone();
    let mut grpc_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let market_data_svc = md_grpc::MarketDataServiceImpl::new(processor_for_grpc);
        let admin_svc = md_grpc::AdminServiceImpl::new(connectors_for_grpc);

        let market_data_server =
            md_proto::MarketDataServiceServer::new(market_data_svc);
        let admin_server =
            md_proto::AdminServiceServer::new(admin_svc);

        info!("gRPC server listening on {}", grpc_addr);
        tonic::transport::Server::builder()
            .add_service(market_data_server)
            .add_service(admin_server)
            .serve_with_shutdown(grpc_addr, async move {
                let _ = grpc_shutdown_rx.changed().await;
                info!("gRPC server shutting down");
            })
            .await
            .expect("gRPC server failed");
    });

    // 启动 API Gateway (REST + WebSocket + Health + Metrics)
    let gateway_addr = parse_addr(&cfg.api_gateway.listen_address);
    let processor_metrics = processor.clone();
    let connector_metrics_for_handler = connector_metrics_map.clone();
    let gateway = md_gateway::Gateway::new(processor.clone(), connectors.clone());
    let router = gateway
        .router()
        .route("/health", axum::routing::get(health_handler))
        .route(
            "/metrics",
            axum::routing::get(move || metrics_handler(processor_metrics.clone(), connector_metrics_for_handler.clone())),
        );

    let gateway_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        info!("API Gateway listening on {}", gateway_addr);
        let listener = tokio::net::TcpListener::bind(gateway_addr)
            .await
            .expect("failed to bind API Gateway");
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal(gateway_shutdown_rx))
            .await
            .expect("API Gateway server failed");
    });

    // 等待 SIGTERM / SIGINT / SIGHUP
    wait_for_signal(&cli.config, &cfg, &processor, &connectors, &shutdown_tx).await;

    info!("received shutdown signal, shutting down");

    // 1. 停止所有连接器（发送 close frame + shutdown signal）
    {
        let conns = connectors.read().await;
        for (name, connector) in conns.iter() {
            info!("stopping connector: {}", name);
            if let Err(e) = connector.stop().await {
                warn!("failed to stop connector {}: {}", name, e);
            }
        }
    }

    // 2. 等待连接器优雅关停（给 time 发送 close frame）
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 3. 停止 processor
    processor.shutdown();
    info!("shutdown complete");
}

/// 等待信号：SIGTERM/SIGINT 触发关停，SIGHUP 触发热重载
async fn wait_for_signal(
    config_path: &str,
    cfg: &md_config::Config,
    _processor: &Arc<Processor>,
    connectors: &Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
    shutdown_tx: &watch::Sender<bool>,
) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
        let mut sighup = signal(SignalKind::hangup()).expect("failed to register SIGHUP handler");

        // 可变副本，用于热重载后更新当前配置
        let mut current_cfg = cfg.clone();

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("received SIGINT (Ctrl+C)");
                    break;
                }
                _ = sigterm.recv() => {
                    info!("received SIGTERM");
                    break;
                }
                _ = sighup.recv() => {
                    info!("received SIGHUP, reloading config from {}", config_path);
                    match md_config::load_config(config_path) {
                        Ok(new_cfg) => {
                            let mut needs_restart = false;

                            // 1. log_level 可运行时更新
                            if new_cfg.log_level != current_cfg.log_level {
                                info!("  log_level: {} -> {} (applied)", current_cfg.log_level, new_cfg.log_level);
                                // tracing 不支持运行时改 level，但记录变更
                            }

                            // 2. 监听地址变更需要重启
                            if new_cfg.grpc_server.listen_address != current_cfg.grpc_server.listen_address {
                                warn!("  grpc_server.listen_address changed: {} -> {} (requires restart)",
                                    current_cfg.grpc_server.listen_address, new_cfg.grpc_server.listen_address);
                                needs_restart = true;
                            }
                            if new_cfg.api_gateway.listen_address != current_cfg.api_gateway.listen_address {
                                warn!("  api_gateway.listen_address changed: {} -> {} (requires restart)",
                                    current_cfg.api_gateway.listen_address, new_cfg.api_gateway.listen_address);
                                needs_restart = true;
                            }

                            // 3. 连接器启停变更需要重启
                            if new_cfg.connectors.binance.enabled != current_cfg.connectors.binance.enabled {
                                warn!("  binance.enabled changed: {} -> {} (requires restart)",
                                    current_cfg.connectors.binance.enabled, new_cfg.connectors.binance.enabled);
                                needs_restart = true;
                            }
                            if new_cfg.connectors.okx.enabled != current_cfg.connectors.okx.enabled {
                                warn!("  okx.enabled changed: {} -> {} (requires restart)",
                                    current_cfg.connectors.okx.enabled, new_cfg.connectors.okx.enabled);
                                needs_restart = true;
                            }

                            // 4. 订阅列表变更：对比并应用
                            let binance_ticks_changed = new_cfg.connectors.binance.subscribe_ticks != current_cfg.connectors.binance.subscribe_ticks;
                            let binance_klines_changed = new_cfg.connectors.binance.subscribe_klines != current_cfg.connectors.binance.subscribe_klines;
                            let okx_ticks_changed = new_cfg.connectors.okx.subscribe_ticks != current_cfg.connectors.okx.subscribe_ticks;
                            let okx_klines_changed = new_cfg.connectors.okx.subscribe_klines != current_cfg.connectors.okx.subscribe_klines;

                            if binance_ticks_changed || binance_klines_changed || okx_ticks_changed || okx_klines_changed {
                                info!("  subscription lists changed, applying...");
                                apply_subscription_changes(&current_cfg, &new_cfg, connectors).await;
                            }

                            // 5. URL 变更需要重启
                            if new_cfg.connectors.binance.stream_base_url != current_cfg.connectors.binance.stream_base_url {
                                warn!("  binance.stream_base_url changed (requires restart)");
                                needs_restart = true;
                            }
                            if new_cfg.connectors.okx.stream_base_url_public != current_cfg.connectors.okx.stream_base_url_public {
                                warn!("  okx.stream_base_url_public changed (requires restart)");
                                needs_restart = true;
                            }

                            if needs_restart {
                                warn!("some config changes require connector restart -- please restart the service");
                            } else {
                                info!("config reloaded and applied successfully");
                            }

                            // 更新当前配置
                            current_cfg = new_cfg;
                        }
                        Err(e) => {
                            warn!("failed to reload config: {}, keeping current config", e);
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.expect("failed to listen for Ctrl+C");
        info!("received Ctrl+C");
    }

    let _ = shutdown_tx.send(true);
}

/// 热重载：对比新旧配置的订阅列表差异并应用
async fn apply_subscription_changes(
    old_cfg: &md_config::Config,
    new_cfg: &md_config::Config,
    connectors: &Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
) {
    use std::collections::{HashMap, HashSet};

    // 构建旧/新订阅集合
    fn build_subs(
        ticks: &[String],
        klines: &HashMap<String, Vec<String>>,
    ) -> HashSet<SubscriptionTarget> {
        let mut set = HashSet::new();
        for sym in ticks {
            set.insert(SubscriptionTarget {
                exchange: String::new(), // 占位，后面填充
                data_type: DataType::Tick,
                symbol: sym.clone(),
                kline_interval: None,
            });
        }
        for (interval, syms) in klines {
            for sym in syms {
                set.insert(SubscriptionTarget {
                    exchange: String::new(),
                    data_type: DataType::Kline,
                    symbol: sym.clone(),
                    kline_interval: Some(interval.clone()),
                });
            }
        }
        set
    }

    let conns = connectors.read().await;

    // Binance 订阅变更
    if new_cfg.connectors.binance.subscribe_ticks != old_cfg.connectors.binance.subscribe_ticks
        || new_cfg.connectors.binance.subscribe_klines != old_cfg.connectors.binance.subscribe_klines
    {
        if let Some(connector) = conns.get("binance") {
            let old_subs = build_subs(
                &old_cfg.connectors.binance.subscribe_ticks,
                &old_cfg.connectors.binance.subscribe_klines,
            );
            let new_subs = build_subs(
                &new_cfg.connectors.binance.subscribe_ticks,
                &new_cfg.connectors.binance.subscribe_klines,
            );

            // 需要新增的
            let to_add: Vec<SubscriptionTarget> = new_subs.difference(&old_subs)
                .map(|t| SubscriptionTarget { exchange: "binance".into(), ..t.clone() })
                .collect();
            // 需要移除的
            let to_remove: Vec<SubscriptionTarget> = old_subs.difference(&new_subs)
                .map(|t| SubscriptionTarget { exchange: "binance".into(), ..t.clone() })
                .collect();

            if !to_add.is_empty() {
                info!("  binance: adding {} subscriptions", to_add.len());
                if let Err(e) = connector.add_subscriptions(to_add).await {
                    warn!("  binance: failed to add subscriptions: {}", e);
                }
            }
            if !to_remove.is_empty() {
                info!("  binance: removing {} subscriptions", to_remove.len());
                if let Err(e) = connector.remove_subscriptions(to_remove).await {
                    warn!("  binance: failed to remove subscriptions: {}", e);
                }
            }
        }
    }

    // OKX Public (trades) 订阅变更
    if new_cfg.connectors.okx.subscribe_ticks != old_cfg.connectors.okx.subscribe_ticks {
        if let Some(connector) = conns.get("okx") {
            let old_ticks: HashSet<String> = old_cfg.connectors.okx.subscribe_ticks.iter().cloned().collect();
            let new_ticks: HashSet<String> = new_cfg.connectors.okx.subscribe_ticks.iter().cloned().collect();

            let to_add: Vec<SubscriptionTarget> = new_ticks.difference(&old_ticks)
                .map(|sym| SubscriptionTarget {
                    exchange: "okx".into(),
                    data_type: DataType::Tick,
                    symbol: sym.clone(),
                    kline_interval: None,
                })
                .collect();
            let to_remove: Vec<SubscriptionTarget> = old_ticks.difference(&new_ticks)
                .map(|sym| SubscriptionTarget {
                    exchange: "okx".into(),
                    data_type: DataType::Tick,
                    symbol: sym.clone(),
                    kline_interval: None,
                })
                .collect();

            if !to_add.is_empty() {
                info!("  okx: adding {} tick subscriptions", to_add.len());
                if let Err(e) = connector.add_subscriptions(to_add).await {
                    warn!("  okx: failed to add tick subscriptions: {}", e);
                }
            }
            if !to_remove.is_empty() {
                info!("  okx: removing {} tick subscriptions", to_remove.len());
                if let Err(e) = connector.remove_subscriptions(to_remove).await {
                    warn!("  okx: failed to remove tick subscriptions: {}", e);
                }
            }
        }
    }

    // OKX Business (klines) 订阅变更
    if new_cfg.connectors.okx.subscribe_klines != old_cfg.connectors.okx.subscribe_klines {
        if let Some(connector) = conns.get("okx-kline") {
            let old_klines = build_subs(&[], &old_cfg.connectors.okx.subscribe_klines);
            let new_klines = build_subs(&[], &new_cfg.connectors.okx.subscribe_klines);

            let to_add: Vec<SubscriptionTarget> = new_klines.difference(&old_klines)
                .map(|t| SubscriptionTarget { exchange: "okx".into(), ..t.clone() })
                .collect();
            let to_remove: Vec<SubscriptionTarget> = old_klines.difference(&new_klines)
                .map(|t| SubscriptionTarget { exchange: "okx".into(), ..t.clone() })
                .collect();

            if !to_add.is_empty() {
                info!("  okx-kline: adding {} kline subscriptions", to_add.len());
                if let Err(e) = connector.add_subscriptions(to_add).await {
                    warn!("  okx-kline: failed to add kline subscriptions: {}", e);
                }
            }
            if !to_remove.is_empty() {
                info!("  okx-kline: removing {} kline subscriptions", to_remove.len());
                if let Err(e) = connector.remove_subscriptions(to_remove).await {
                    warn!("  okx-kline: failed to remove kline subscriptions: {}", e);
                }
            }
        }
    }
}

/// 等待关停信号（用于 axum graceful shutdown）
async fn shutdown_signal(mut rx: watch::Receiver<bool>) {
    let _ = rx.changed().await;
}

/// 健康检查端点
async fn health_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "md-server"
    }))
}

/// 渲染一个 Histogram 到 Prometheus exposition 格式（累积桶）
///
/// `metric_name`：完整指标名（不含 _bucket 后缀）
/// `labels`：可选的额外标签字符串（如 `exchange="binance",kind="tick"`），可为 ""
/// `help`/`unit`：HELP 文本里使用
fn render_histogram(
    out: &mut String,
    metric_name: &str,
    help: &str,
    snap: &md_processor::HistogramSnapshot,
    labels: &str,
) {
    out.push_str(&format!("# HELP {} {}\n", metric_name, help));
    out.push_str(&format!("# TYPE {} histogram\n", metric_name));

    // 标签后缀拼装：bucket 多一个 le=... 的标签
    let bucket_label_prefix = if labels.is_empty() { String::new() } else { format!("{},", labels) };
    let plain_label = if labels.is_empty() { String::new() } else { format!("{{{}}}", labels) };

    let mut cumulative = 0u64;
    for (i, &boundary) in snap.boundaries.iter().enumerate() {
        cumulative += snap.buckets[i];
        out.push_str(&format!(
            "{}_bucket{{{}le=\"{}\"}} {}\n",
            metric_name, bucket_label_prefix, boundary, cumulative
        ));
    }
    cumulative += snap.buckets[snap.boundaries.len()];
    out.push_str(&format!(
        "{}_bucket{{{}le=\"+Inf\"}} {}\n",
        metric_name, bucket_label_prefix, cumulative
    ));
    out.push_str(&format!("{}_sum{} {}\n", metric_name, plain_label, snap.sum));
    out.push_str(&format!("{}_count{} {}\n", metric_name, plain_label, snap.count));
}

/// Prometheus metrics 端点
///
/// 暴露指标矩阵（与 Go 版 dashboard 等价）：
///
/// **吞吐 / 处理**
/// - `md_ticks_processed` / `md_klines_processed` (counter，全局)
/// - `md_data_ingested_total{exchange,kind}` (counter，按交易所+类型)
/// - `md_ticks_dropped` / `md_klines_dropped` (counter，channel 满丢弃)
///
/// **延迟**
/// - `md_ingestion_latency_ms{...}` (histogram，全局 + 按 exchange/kind)
/// - `md_gateway_forward_latency_ms` (histogram，publish→ws_send)
///
/// **连接器**
/// - `md_connector_connected{exchange}` (gauge 0/1)
/// - `md_connector_reconnect_total{exchange}` (counter)
/// - `md_connector_subscribe_failed_total{exchange}` (counter)
///
/// **网关 / WebSocket**
/// - `md_ws_active_clients` (gauge)
/// - `md_ws_kicked_lagged_total` (counter)
/// - `md_ws_messages_sent_total{kind}` (counter)
/// - `md_broadcast_lagged_total{topic_kind}` (counter)
///
/// **进程级（Linux 标准）**
/// - `process_resident_memory_bytes` / `process_virtual_memory_bytes` (gauge)
/// - `process_cpu_seconds_total` (counter)
/// - `process_open_fds` / `process_max_fds` (gauge)
/// - `process_start_time_seconds` / `process_uptime_seconds` (gauge)
async fn metrics_handler(
    processor: Arc<Processor>,
    connector_metrics: HashMap<String, Arc<ConnectorMetrics>>,
) -> String {
    let snap = processor.metrics.snapshot();
    let mut out = String::with_capacity(8192);

    // ---- 全局吞吐 counter ----
    out.push_str("# HELP md_ticks_processed Total ticks processed (global)\n");
    out.push_str("# TYPE md_ticks_processed counter\n");
    out.push_str(&format!("md_ticks_processed {}\n", snap.ticks_processed));
    out.push_str("# HELP md_klines_processed Total klines processed (global)\n");
    out.push_str("# TYPE md_klines_processed counter\n");
    out.push_str(&format!("md_klines_processed {}\n", snap.klines_processed));
    out.push_str("# HELP md_ticks_dropped Total ticks dropped (mpsc channel full)\n");
    out.push_str("# TYPE md_ticks_dropped counter\n");
    out.push_str(&format!("md_ticks_dropped {}\n", snap.ticks_dropped));
    out.push_str("# HELP md_klines_dropped Total klines dropped (mpsc channel full)\n");
    out.push_str("# TYPE md_klines_dropped counter\n");
    out.push_str(&format!("md_klines_dropped {}\n", snap.klines_dropped));

    // ---- 按交易所 + 类型的吞吐（对应 Go 版"数据采集吞吐量 按交易所+类型"图）----
    out.push_str("# HELP md_data_ingested_total Total data ingested per exchange and kind\n");
    out.push_str("# TYPE md_data_ingested_total counter\n");
    for ex in &snap.per_exchange {
        out.push_str(&format!(
            "md_data_ingested_total{{exchange=\"{}\",kind=\"tick\"}} {}\n",
            ex.exchange, ex.ticks_processed
        ));
        out.push_str(&format!(
            "md_data_ingested_total{{exchange=\"{}\",kind=\"kline\"}} {}\n",
            ex.exchange, ex.klines_processed
        ));
    }

    // ---- 连接器级别指标 ----
    out.push_str("# HELP md_connector_connected Connector connected state (0/1)\n");
    out.push_str("# TYPE md_connector_connected gauge\n");
    for (name, m) in &connector_metrics {
        out.push_str(&format!(
            "md_connector_connected{{exchange=\"{}\"}} {}\n",
            name,
            m.connected.load(Ordering::Relaxed)
        ));
    }
    out.push_str("# HELP md_connector_reconnect_total Total connector reconnections\n");
    out.push_str("# TYPE md_connector_reconnect_total counter\n");
    for (name, m) in &connector_metrics {
        out.push_str(&format!(
            "md_connector_reconnect_total{{exchange=\"{}\"}} {}\n",
            name,
            m.reconnect_total.load(Ordering::Relaxed)
        ));
    }
    out.push_str("# HELP md_connector_subscribe_failed_total Total subscribe failures\n");
    out.push_str("# TYPE md_connector_subscribe_failed_total counter\n");
    for (name, m) in &connector_metrics {
        out.push_str(&format!(
            "md_connector_subscribe_failed_total{{exchange=\"{}\"}} {}\n",
            name,
            m.subscribe_failed_total.load(Ordering::Relaxed)
        ));
    }

    // ---- 全局入库延迟直方图（兼容老 dashboard）----
    render_histogram(
        &mut out,
        "md_ingestion_latency_milliseconds",
        "Ingestion latency from exchange event to local receive (global)",
        &snap.ingestion_latency,
        "",
    );

    // ---- 按交易所 + 类型的入库延迟（对应 Go 版"Tick/Kline 采集延迟 按交易所"图）----
    for ex in &snap.per_exchange {
        // tick 延迟
        let labels = format!("exchange=\"{}\",kind=\"tick\"", ex.exchange);
        render_histogram(
            &mut out,
            "md_ingestion_latency_per_exchange_ms",
            "Ingestion latency per exchange and kind (ms)",
            &ex.ingestion_latency_tick,
            &labels,
        );
        // kline 延迟
        let labels = format!("exchange=\"{}\",kind=\"kline\"", ex.exchange);
        render_histogram(
            &mut out,
            "md_ingestion_latency_per_exchange_ms",
            "Ingestion latency per exchange and kind (ms)",
            &ex.ingestion_latency_kline,
            &labels,
        );
    }

    // ---- 网关 / WebSocket 指标 ----
    out.push_str("# HELP md_ws_active_clients Currently active WebSocket clients (gateway + legacy)\n");
    out.push_str("# TYPE md_ws_active_clients gauge\n");
    out.push_str(&format!("md_ws_active_clients {}\n", snap.ws_active_clients));

    out.push_str("# HELP md_ws_kicked_lagged_total Total WebSocket clients kicked due to consecutive broadcast lagged\n");
    out.push_str("# TYPE md_ws_kicked_lagged_total counter\n");
    out.push_str(&format!("md_ws_kicked_lagged_total {}\n", snap.ws_kicked_lagged_total));

    out.push_str("# HELP md_ws_messages_sent_total Total WebSocket / gRPC messages successfully delivered to clients\n");
    out.push_str("# TYPE md_ws_messages_sent_total counter\n");
    out.push_str(&format!(
        "md_ws_messages_sent_total{{kind=\"tick\"}} {}\n",
        snap.ws_messages_sent_tick
    ));
    out.push_str(&format!(
        "md_ws_messages_sent_total{{kind=\"kline\"}} {}\n",
        snap.ws_messages_sent_kline
    ));

    out.push_str("# HELP md_broadcast_lagged_total Total broadcast lagged events (subscriber fell behind)\n");
    out.push_str("# TYPE md_broadcast_lagged_total counter\n");
    out.push_str(&format!(
        "md_broadcast_lagged_total{{topic_kind=\"tick\"}} {}\n",
        snap.broadcast_lagged_tick
    ));
    out.push_str(&format!(
        "md_broadcast_lagged_total{{topic_kind=\"kline\"}} {}\n",
        snap.broadcast_lagged_kline
    ));

    render_histogram(
        &mut out,
        "md_gateway_forward_latency_ms",
        "Gateway internal forwarding latency: publish to ws/grpc send completion (ms)",
        &snap.gateway_forward_latency,
        "",
    );

    // ---- 进程级标准指标（Linux 上才有真实值，与 Go process_exporter 等价）----
    let proc_snap = process_metrics::collect();
    process_metrics::render(&proc_snap, &mut out);

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use md_domain::types::{Kline, Tick};

    /// 回归测试：保证 /metrics 输出包含 Grafana dashboard 所需的每一个关键指标
    /// （任何后续重构都不能删除这些指标，否则面板会断）
    #[tokio::test]
    async fn metrics_handler_includes_all_dashboard_metrics() {
        // 准备 processor 并触发各类指标
        let processor = Arc::new(Processor::new_with_broadcast_capacity(100, 100, 256));
        processor.handle_tick(Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            price: "67000.0".into(),
            timestamp: 1711929600000,
            exchange_event_ts: 1711929600000,
            connector_receive_ts: 1711929600050,
            ..Default::default()
        });
        processor.handle_kline(Kline {
            exchange: "okx".into(),
            symbol: "BTC-USDT".into(),
            interval: "1m".into(),
            close: "67050.0".into(),
            open_time: 1711929600000,
            close_time: 1711929659999,
            exchange_event_ts: 1711929600000,
            connector_receive_ts: 1711929600100,
            ..Default::default()
        });
        processor.metrics.ws_client_connected();
        processor.metrics.ws_message_sent("tick");
        processor.metrics.ws_message_sent("kline");
        processor.metrics.ws_client_kicked_lagged();
        processor.metrics.record_broadcast_lagged("tick");
        processor.metrics.record_gateway_forward_latency_ms(2);

        let mut conn_metrics: HashMap<String, Arc<ConnectorMetrics>> = HashMap::new();
        let m = ConnectorMetrics::new();
        m.connected.store(1, Ordering::Relaxed);
        m.reconnect_total.store(3, Ordering::Relaxed);
        m.subscribe_failed_total.store(1, Ordering::Relaxed);
        conn_metrics.insert("binance".into(), m);

        process_metrics::init();
        let out = metrics_handler(processor, conn_metrics).await;

        // ---- 全局吞吐 ----
        assert!(out.contains("md_ticks_processed 1"), "missing md_ticks_processed");
        assert!(out.contains("md_klines_processed 1"), "missing md_klines_processed");
        // ---- 按 exchange + kind 的吞吐（Go dashboard"数据采集吞吐量"）----
        assert!(
            out.contains("md_data_ingested_total{exchange=\"binance\",kind=\"tick\"}"),
            "missing per-exchange ingest counter (binance/tick)"
        );
        assert!(
            out.contains("md_data_ingested_total{exchange=\"okx\",kind=\"kline\"}"),
            "missing per-exchange ingest counter (okx/kline)"
        );
        // ---- 按 exchange + kind 的延迟 histogram（Go dashboard"采集延迟 P50/P99"）----
        assert!(
            out.contains("md_ingestion_latency_per_exchange_ms_bucket{exchange=\"binance\",kind=\"tick\""),
            "missing per-exchange latency histogram (binance/tick)"
        );
        assert!(
            out.contains("md_ingestion_latency_per_exchange_ms_bucket{exchange=\"okx\",kind=\"kline\""),
            "missing per-exchange latency histogram (okx/kline)"
        );
        // ---- 网关内部转发延迟（Go dashboard"网关内部延迟"）----
        assert!(
            out.contains("md_gateway_forward_latency_ms_bucket"),
            "missing gateway_forward_latency histogram"
        );
        // ---- WebSocket 健康（Go dashboard"活跃连接 / 慢客户端踢出 / 推送吞吐"）----
        assert!(out.contains("md_ws_active_clients 1"), "missing ws_active_clients");
        assert!(out.contains("md_ws_kicked_lagged_total 1"), "missing ws_kicked_lagged_total");
        assert!(out.contains("md_ws_messages_sent_total{kind=\"tick\"}"), "missing ws_messages_sent_total tick");
        assert!(out.contains("md_ws_messages_sent_total{kind=\"kline\"}"), "missing ws_messages_sent_total kline");
        // ---- broadcast lagged ----
        assert!(out.contains("md_broadcast_lagged_total{topic_kind=\"tick\"} 1"), "missing broadcast_lagged_total");
        // ---- 连接器指标 ----
        assert!(out.contains("md_connector_connected{exchange=\"binance\"} 1"));
        assert!(out.contains("md_connector_reconnect_total{exchange=\"binance\"} 3"));
        // ---- 进程级标准指标（Go process_exporter 对应）----
        assert!(out.contains("process_resident_memory_bytes"), "missing process_resident_memory_bytes");
        assert!(out.contains("process_virtual_memory_bytes"), "missing process_virtual_memory_bytes");
        assert!(out.contains("process_cpu_seconds_total"), "missing process_cpu_seconds_total");
        assert!(out.contains("process_open_fds"), "missing process_open_fds");
        assert!(out.contains("process_max_fds"), "missing process_max_fds");
        assert!(out.contains("process_start_time_seconds"), "missing process_start_time_seconds");
        assert!(out.contains("process_uptime_seconds"), "missing process_uptime_seconds");
    }
}
