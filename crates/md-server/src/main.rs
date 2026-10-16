mod alloc_stats;
mod process_metrics;
mod reload;

use clap::Parser;
use md_connector::base::BaseConnector;
use md_connector::binance::{BinanceAdapter, BinanceConnectorConfig};
use md_connector::okx::{OkxAdapter, OkxConnectorConfig};
use md_connector::{Connector, DataType, SubscriptionTarget};
use md_processor::{ConnectorMetrics, Processor};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
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
) -> (
    Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
    HashMap<String, Arc<ConnectorMetrics>>,
) {
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
        let connector = Arc::new(
            BaseConnector::new(
                adapter,
                processor.tick_tx(),
                processor.kline_tx(),
                cfg.connectors.binance.reconnect_delay,
            )
            .with_full_metrics(
                processor.metrics.ticks_dropped.clone(),
                processor.metrics.klines_dropped.clone(),
                binance_metrics.connected.clone(),
                binance_metrics.reconnect_total.clone(),
                binance_metrics.subscribe_failed_total.clone(),
            ),
        );

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

        connectors
            .write()
            .await
            .insert("binance".into(), connector.clone());

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
            let adapter = OkxAdapter::with_mode(
                okx_cfg_base.clone(),
                md_connector::okx::OkxStreamMode::Public,
            );
            let okx_metrics = ConnectorMetrics::new();
            connector_metrics_map.insert("okx".into(), okx_metrics.clone());
            let connector = Arc::new(
                BaseConnector::new(
                    adapter,
                    processor.tick_tx(),
                    processor.kline_tx(),
                    cfg.connectors.okx.reconnect_delay,
                )
                .with_full_metrics(
                    processor.metrics.ticks_dropped.clone(),
                    processor.metrics.klines_dropped.clone(),
                    okx_metrics.connected.clone(),
                    okx_metrics.reconnect_total.clone(),
                    okx_metrics.subscribe_failed_total.clone(),
                ),
            );

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

            connectors
                .write()
                .await
                .insert("okx".into(), connector.clone());

            tokio::spawn(async move {
                connector.run_connection_loop().await;
            });
        }

        // ---- OKX Business: candles (kline 数据) ----
        if !cfg.connectors.okx.subscribe_klines.is_empty() {
            info!("starting OKX Business connector (candles)");
            let adapter = OkxAdapter::with_mode(
                okx_cfg_base.clone(),
                md_connector::okx::OkxStreamMode::Business,
            );
            let okx_kline_metrics = ConnectorMetrics::new();
            connector_metrics_map.insert("okx-kline".into(), okx_kline_metrics.clone());
            let connector = Arc::new(
                BaseConnector::new(
                    adapter,
                    processor.tick_tx(),
                    processor.kline_tx(),
                    cfg.connectors.okx.reconnect_delay,
                )
                .with_full_metrics(
                    processor.metrics.ticks_dropped.clone(),
                    processor.metrics.klines_dropped.clone(),
                    okx_kline_metrics.connected.clone(),
                    okx_kline_metrics.reconnect_total.clone(),
                    okx_kline_metrics.subscribe_failed_total.clone(),
                ),
            );

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

            connectors
                .write()
                .await
                .insert("okx-kline".into(), connector.clone());

            tokio::spawn(async move {
                connector.run_connection_loop().await;
            });
        }
    }

    // ---- Quote 专用连接器（1.1.0 新增；开关默认关闭，关闭时不建任何新连接）----
    start_quote_connectors(cfg, processor, &connectors, &mut connector_metrics_map).await;

    (connectors, connector_metrics_map)
}

/// 启动 quote 专用连接器（`binance-quote` / `okx-quote`）。
///
/// 设计要点：
/// - 与既有 tick/kline 连接器完全隔离：独立 WebSocket 连接、独立重连/心跳状态，
///   quote 侧故障（断线、订阅被拒）不会影响 tick/kline；
/// - 币安 bookTicker 属 Public 类流，连 `connectors.binance.stream_base_url_public`；
///   OKX tickers 走 Public WebSocket；
/// - 仅当 `quotes_enabled=true` 且对应交易所 `enabled=true` 且初始列表非空时才启动
///   （与 OKX tick 连接器一致：OKX 对无订阅连接约 30 秒后会断开，空连接只会造成重连噪声）。
async fn start_quote_connectors(
    cfg: &md_config::Config,
    processor: &Arc<Processor>,
    connectors: &Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
    connector_metrics_map: &mut HashMap<String, Arc<ConnectorMetrics>>,
) {
    // ---- Binance quote（bookTicker，Public 流）----
    let b = &cfg.connectors.binance;
    if b.quotes_enabled && !b.enabled {
        warn!("binance.quotes_enabled=true 但 binance.enabled=false，quote 连接器不会启动");
    } else if b.quotes_enabled && b.subscribe_quotes.is_empty() {
        warn!("binance.quotes_enabled=true 但 subscribe_quotes 为空，quote 连接器不会启动（补充列表后需重启）");
    } else if b.quotes_enabled {
        info!(
            "starting Binance quote connector (bookTicker) -> {}",
            b.stream_base_url_public
        );
        let adapter = BinanceAdapter::new_quote(BinanceConnectorConfig {
            // quote 连接器连 Public 流地址
            stream_base_url: b.stream_base_url_public.clone(),
            subscribe_ticks: Vec::new(),
            subscribe_klines: HashMap::new(),
            reconnect_delay: b.reconnect_delay,
            ping_interval: b.ping_interval,
        });
        let m = ConnectorMetrics::new();
        connector_metrics_map.insert("binance-quote".into(), m.clone());
        let connector = Arc::new(
            BaseConnector::new(
                adapter,
                processor.tick_tx(),
                processor.kline_tx(),
                b.reconnect_delay,
            )
            .with_quote(
                processor.quote_tx(),
                processor.metrics.quotes_dropped.clone(),
            )
            .with_full_metrics(
                processor.metrics.ticks_dropped.clone(),
                processor.metrics.klines_dropped.clone(),
                m.connected.clone(),
                m.reconnect_total.clone(),
                m.subscribe_failed_total.clone(),
            ),
        );
        let targets: Vec<SubscriptionTarget> = b
            .subscribe_quotes
            .iter()
            .map(|symbol| SubscriptionTarget {
                exchange: "binance".into(),
                data_type: DataType::Quote,
                symbol: symbol.clone(),
                kline_interval: None,
            })
            .collect();
        if let Err(e) = connector.add_subscriptions(targets).await {
            warn!("failed to add Binance quote subscriptions: {}", e);
        }
        connectors
            .write()
            .await
            .insert("binance-quote".into(), connector.clone());
        tokio::spawn(async move {
            connector.run_connection_loop().await;
        });
    }

    // ---- OKX quote（tickers，Public WebSocket）----
    let o = &cfg.connectors.okx;
    if o.quotes_enabled && !o.enabled {
        warn!("okx.quotes_enabled=true 但 okx.enabled=false，quote 连接器不会启动");
    } else if o.quotes_enabled && o.subscribe_quotes.is_empty() {
        warn!("okx.quotes_enabled=true 但 subscribe_quotes 为空，quote 连接器不会启动（补充列表后需重启）");
    } else if o.quotes_enabled {
        info!(
            "starting OKX quote connector (tickers) -> {}",
            o.stream_base_url_public
        );
        let adapter = OkxAdapter::new_quote(OkxConnectorConfig {
            stream_base_url_public: o.stream_base_url_public.clone(),
            stream_base_url_business: o.stream_base_url_business.clone(),
            subscribe_ticks: Vec::new(),
            subscribe_klines: HashMap::new(),
            reconnect_delay: o.reconnect_delay,
            ping_interval: o.ping_interval,
        });
        let m = ConnectorMetrics::new();
        connector_metrics_map.insert("okx-quote".into(), m.clone());
        let connector = Arc::new(
            BaseConnector::new(
                adapter,
                processor.tick_tx(),
                processor.kline_tx(),
                o.reconnect_delay,
            )
            .with_quote(
                processor.quote_tx(),
                processor.metrics.quotes_dropped.clone(),
            )
            .with_full_metrics(
                processor.metrics.ticks_dropped.clone(),
                processor.metrics.klines_dropped.clone(),
                m.connected.clone(),
                m.reconnect_total.clone(),
                m.subscribe_failed_total.clone(),
            ),
        );
        let targets: Vec<SubscriptionTarget> = o
            .subscribe_quotes
            .iter()
            .map(|symbol| SubscriptionTarget {
                exchange: "okx".into(),
                data_type: DataType::Quote,
                symbol: symbol.clone(),
                kline_interval: None,
            })
            .collect();
        if let Err(e) = connector.add_subscriptions(targets).await {
            warn!("failed to add OKX quote subscriptions: {}", e);
        }
        connectors
            .write()
            .await
            .insert("okx-quote".into(), connector.clone());
        tokio::spawn(async move {
            connector.run_connection_loop().await;
        });
    }
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

    // 加载"生效配置"（YAML + 环境变量覆盖 + 端口偏移）。
    // SIGHUP 重载走同一个函数，保证启动与重载的配置推导一致（见 reload.rs）。
    info!("loading config from {}", cli.config);
    let cfg = match reload::load_effective_config(&cli.config, cli.port_offset) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("failed to load config: {}", e);
            std::process::exit(1);
        }
    };
    if cli.port_offset > 0 {
        info!("port offset: +{}", cli.port_offset);
    }

    info!("log_level: {}", cfg.log_level);
    info!("gRPC: {}", cfg.grpc_server.listen_address);
    info!("API Gateway: {}", cfg.api_gateway.listen_address);

    // 创建 Processor
    let processor = Arc::new(Processor::new_full(
        cfg.processor.tick_channel_buffer,
        cfg.processor.kline_channel_buffer,
        cfg.processor.broadcast_capacity,
        cfg.processor.quote_channel_buffer,
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

        let market_data_server = md_proto::MarketDataServiceServer::new(market_data_svc);
        let admin_server = md_proto::AdminServiceServer::new(admin_svc);

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
    // 1.1.1：Gateway WS 写超时取配置项 api_gateway.ws_write_wait（默认 10s，"0s" 关闭）；Legacy /ws 不受影响
    let gateway = md_gateway::Gateway::new(processor.clone(), connectors.clone())
        .with_ws_write_wait(cfg.api_gateway.ws_write_wait);
    let router = gateway
        .router()
        .route("/health", axum::routing::get(health_handler))
        .route(
            "/metrics",
            axum::routing::get(move || {
                metrics_handler(
                    processor_metrics.clone(),
                    connector_metrics_for_handler.clone(),
                )
            }),
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
    wait_for_signal(
        &cli.config,
        cli.port_offset,
        &cfg,
        &processor,
        &connectors,
        &shutdown_tx,
    )
    .await;

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

/// 等待信号：SIGTERM/SIGINT 触发关停，SIGHUP 触发配置重载（逻辑见 `reload.rs`）
///
/// `cfg` 必须是启动时实际使用的"生效配置"（已含环境变量覆盖和端口偏移）；
/// `port_offset` 用于让重载时用同一套规则推导文件里的新配置。
async fn wait_for_signal(
    config_path: &str,
    port_offset: u16,
    cfg: &md_config::Config,
    _processor: &Arc<Processor>,
    connectors: &Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
    shutdown_tx: &watch::Sender<bool>,
) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
        let mut sighup = signal(SignalKind::hangup()).expect("failed to register SIGHUP handler");

        // "已应用配置"：只记录进程真正在用的配置；重启才能生效的变更不会写进来
        let mut reload_state = reload::ReloadState::new(cfg.clone());

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
                    reload::handle_sighup(&mut reload_state, config_path, port_offset, connectors).await;
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (config_path, port_offset, cfg, connectors);
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C");
        info!("received Ctrl+C");
    }

    let _ = shutdown_tx.send(true);
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

/// 渲染单条 histogram 序列的样本行（累积桶 + sum + count），不含 HELP/TYPE
///
/// 调用方负责在循环外输出一次 `# HELP` / `# TYPE`。
/// `base_labels`：该序列的标签集（不含 le），如 `exchange="binance",type="tick",symbol="BTCUSDT",interval=""`，不可为空。
fn render_histogram_series(
    out: &mut String,
    metric_name: &str,
    snap: &md_processor::HistogramSnapshot,
    base_labels: &str,
) {
    let mut cumulative = 0u64;
    for (i, &boundary) in snap.boundaries.iter().enumerate() {
        cumulative += snap.buckets[i];
        out.push_str(&format!(
            "{}_bucket{{{},le=\"{}\"}} {}\n",
            metric_name, base_labels, boundary, cumulative
        ));
    }
    cumulative += snap.buckets[snap.boundaries.len()];
    out.push_str(&format!(
        "{}_bucket{{{},le=\"+Inf\"}} {}\n",
        metric_name, base_labels, cumulative
    ));
    out.push_str(&format!(
        "{}_sum{{{}}} {}\n",
        metric_name, base_labels, snap.sum
    ));
    out.push_str(&format!(
        "{}_count{{{}}} {}\n",
        metric_name, base_labels, snap.count
    ));
}

/// 转义 Prometheus 标签值（symbol 可能含特殊字符，如 OKX 的 "BTC-USDT-SWAP"）
fn esc(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Prometheus metrics 端点
///
/// 暴露指标矩阵（与 Go 版 dashboard 完全对齐，含交易所 + 标的 + 周期多维度）：
///
/// **吞吐 / 处理**
/// - `md_ticks_processed` / `md_klines_processed` (counter，全局顶部 stat)
/// - `md_ticks_dropped` / `md_klines_dropped` (counter，channel 满丢弃)
/// - `md_quotes_processed` / `md_quotes_dropped` (counter，1.1.0 新增)
///
/// **入库延迟 / 吞吐（核心多维指标）**
/// - `md_ingestion_latency_ms{exchange,type,symbol,interval}` (histogram)
///   对标 Go 版 marketdata_processor_ingestion_latency_ms。dashboard 由此聚合出：
///   按交易所 `by (exchange)`、按标的 `by (exchange,symbol)`、按标的+周期 `by (exchange,symbol,interval)`；
///   吞吐量用其 `_count`。
///
/// **连接器**
/// - `md_connector_connected{exchange}` (gauge 0/1)
/// - `md_connector_reconnect_total{exchange}` (counter)
/// - `md_connector_subscribe_failed_total{exchange}` (counter)
///
/// **网关 / WebSocket**
/// - `md_ws_active_clients` (gauge)
/// - `md_ws_subscriptions_active` (gauge，1.1.1 新增：全部 WS 连接持有的 broadcast 接收端总数)
/// - `md_ws_kicked_lagged_total` (counter)
/// - `md_ws_kicked_write_timeout_total` (counter，1.1.1 新增：因写超时被踢出的 Gateway 客户端)
/// - `md_ws_messages_sent_total{kind}` (counter)
/// - `md_broadcast_lagged_total{topic_kind}` (counter)
/// - `md_gateway_internal_latency_ms{topic}` (histogram)
///   对标 Go 版 marketdata_gateway_internal_latency_ms。总体延迟 `by (le)`，推送吞吐 TOP10 用 `_count by (topic)`。
///
/// **运行时 / 分配器诊断（1.1.1 新增）**
/// - `md_tokio_alive_tasks` / `md_tokio_workers` / `md_tokio_global_queue_depth` (gauge)
/// - `md_jemalloc_{allocated,active,resident,retained,mapped}_bytes` (gauge，仅 `jemalloc-stats` 构建)
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
    // quote（1.1.0 新增；开关关闭时恒为 0）
    out.push_str("# HELP md_quotes_processed Total quotes processed (global)\n");
    out.push_str("# TYPE md_quotes_processed counter\n");
    out.push_str(&format!("md_quotes_processed {}\n", snap.quotes_processed));
    out.push_str("# HELP md_quotes_dropped Total quotes dropped (mpsc channel full)\n");
    out.push_str("# TYPE md_quotes_dropped counter\n");
    out.push_str(&format!("md_quotes_dropped {}\n", snap.quotes_dropped));

    // ---- 入库延迟/吞吐多维 histogram（exchange + type + symbol + interval）----
    // 到达延迟 md_ingestion_latency_ms = recv − E（交易所推送 → 本机接收）：我们这条链路的真实延迟。
    // 对标 Go 版 ingestion_latency_ms；dashboard 自行聚合到交易所/标的/周期维度。
    out.push_str("# HELP md_ingestion_latency_ms Arrival latency: exchange push event(E)->local receive (ms), by exchange/type/symbol/interval\n");
    out.push_str("# TYPE md_ingestion_latency_ms histogram\n");
    for s in &snap.ingestion_series {
        let base = format!(
            "exchange=\"{}\",type=\"{}\",symbol=\"{}\",interval=\"{}\"",
            esc(&s.exchange),
            s.kind,
            esc(&s.symbol),
            esc(&s.interval)
        );
        render_histogram_series(&mut out, "md_ingestion_latency_ms", &s.latency, &base);
    }

    // 端到端延迟 md_ingestion_latency_e2e_ms = recv − T（交易所成交 → 本机接收）：含交易所内部聚合延迟。
    // 与到达延迟对照：两者之差 ≈ 交易所内部延迟（币安 aggTrade E−T≈29ms；OKX 二者相等）。
    out.push_str("# HELP md_ingestion_latency_e2e_ms End-to-end latency: exchange trade time(T)->local receive (ms), includes exchange-side aggregation delay\n");
    out.push_str("# TYPE md_ingestion_latency_e2e_ms histogram\n");
    for s in &snap.ingestion_e2e_series {
        let base = format!(
            "exchange=\"{}\",type=\"{}\",symbol=\"{}\",interval=\"{}\"",
            esc(&s.exchange),
            s.kind,
            esc(&s.symbol),
            esc(&s.interval)
        );
        render_histogram_series(&mut out, "md_ingestion_latency_e2e_ms", &s.latency, &base);
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

    // ---- 网关 / WebSocket 指标 ----
    out.push_str(
        "# HELP md_ws_active_clients Currently active WebSocket clients (gateway + legacy)\n",
    );
    out.push_str("# TYPE md_ws_active_clients gauge\n");
    out.push_str(&format!(
        "md_ws_active_clients {}\n",
        snap.ws_active_clients
    ));

    out.push_str(
        "# HELP md_ws_subscriptions_active Broadcast receivers currently held by all WebSocket connections (gateway + legacy)\n",
    );
    out.push_str("# TYPE md_ws_subscriptions_active gauge\n");
    out.push_str(&format!(
        "md_ws_subscriptions_active {}\n",
        snap.ws_subscriptions_active
    ));

    out.push_str("# HELP md_ws_kicked_lagged_total Total WebSocket clients kicked due to consecutive broadcast lagged\n");
    out.push_str("# TYPE md_ws_kicked_lagged_total counter\n");
    out.push_str(&format!(
        "md_ws_kicked_lagged_total {}\n",
        snap.ws_kicked_lagged_total
    ));

    out.push_str("# HELP md_ws_kicked_write_timeout_total Total gateway WebSocket clients kicked because a write blocked longer than api_gateway.ws_write_wait\n");
    out.push_str("# TYPE md_ws_kicked_write_timeout_total counter\n");
    out.push_str(&format!(
        "md_ws_kicked_write_timeout_total {}\n",
        snap.ws_kicked_write_timeout_total
    ));

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
    out.push_str(&format!(
        "md_ws_messages_sent_total{{kind=\"quote\"}} {}\n",
        snap.ws_messages_sent_quote
    ));

    out.push_str(
        "# HELP md_broadcast_lagged_total Total broadcast lagged events (subscriber fell behind)\n",
    );
    out.push_str("# TYPE md_broadcast_lagged_total counter\n");
    out.push_str(&format!(
        "md_broadcast_lagged_total{{topic_kind=\"tick\"}} {}\n",
        snap.broadcast_lagged_tick
    ));
    out.push_str(&format!(
        "md_broadcast_lagged_total{{topic_kind=\"kline\"}} {}\n",
        snap.broadcast_lagged_kline
    ));
    out.push_str(&format!(
        "md_broadcast_lagged_total{{topic_kind=\"quote\"}} {}\n",
        snap.broadcast_lagged_quote
    ));

    // ---- 网关内部转发延迟（按 topic）：总体延迟 + 按 topic 推送吞吐 ----
    out.push_str("# HELP md_gateway_internal_latency_ms Gateway forward latency publish->client send (ms), by topic\n");
    out.push_str("# TYPE md_gateway_internal_latency_ms histogram\n");
    for g in &snap.gateway_series {
        let base = format!("topic=\"{}\"", esc(&g.topic));
        render_histogram_series(
            &mut out,
            "md_gateway_internal_latency_ms",
            &g.latency,
            &base,
        );
    }

    // ---- tokio 运行时指标（1.1.1：重连前后存活 task 数应回到同一水平，用于发现孤儿 task）----
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let rt = handle.metrics();
        out.push_str("# HELP md_tokio_alive_tasks Tasks currently alive in the tokio runtime\n");
        out.push_str("# TYPE md_tokio_alive_tasks gauge\n");
        out.push_str(&format!("md_tokio_alive_tasks {}\n", rt.num_alive_tasks()));
        out.push_str("# HELP md_tokio_workers Tokio runtime worker threads\n");
        out.push_str("# TYPE md_tokio_workers gauge\n");
        out.push_str(&format!("md_tokio_workers {}\n", rt.num_workers()));
        out.push_str(
            "# HELP md_tokio_global_queue_depth Tasks waiting in the tokio global queue\n",
        );
        out.push_str("# TYPE md_tokio_global_queue_depth gauge\n");
        out.push_str(&format!(
            "md_tokio_global_queue_depth {}\n",
            rt.global_queue_depth()
        ));
    }

    // ---- 分配器统计（仅 jemalloc-stats 构建；默认构建无输出）----
    alloc_stats::render(&mut out);

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
        let mut sub_guard = md_processor::WsSubscriptionGuard::new(processor.metrics.clone());
        sub_guard.sync(3);
        processor.metrics.ws_message_sent("tick");
        processor.metrics.ws_message_sent("kline");
        processor.metrics.ws_client_kicked_lagged();
        processor.metrics.ws_client_kicked_write_timeout();
        processor.metrics.record_broadcast_lagged("tick");
        processor
            .metrics
            .record_gateway_forward_latency_ms("tick.binance.BTCUSDT", 2);

        let mut conn_metrics: HashMap<String, Arc<ConnectorMetrics>> = HashMap::new();
        let m = ConnectorMetrics::new();
        m.connected.store(1, Ordering::Relaxed);
        m.reconnect_total.store(3, Ordering::Relaxed);
        m.subscribe_failed_total.store(1, Ordering::Relaxed);
        conn_metrics.insert("binance".into(), m);

        process_metrics::init();
        let out = metrics_handler(processor, conn_metrics).await;

        // ---- 全局吞吐 ----
        assert!(
            out.contains("md_ticks_processed 1"),
            "missing md_ticks_processed"
        );
        assert!(
            out.contains("md_klines_processed 1"),
            "missing md_klines_processed"
        );
        // ---- 多维入库延迟/吞吐 histogram：按 exchange + type + symbol + interval ----
        // 按标的（symbol）维度 -- 这是 Go 版有而旧 Rust 版缺失的关键维度
        assert!(
            out.contains("md_ingestion_latency_ms_bucket{exchange=\"binance\",type=\"tick\",symbol=\"BTCUSDT\",interval=\"\""),
            "missing per-symbol tick latency (binance/BTCUSDT)"
        );
        assert!(
            out.contains("md_ingestion_latency_ms_bucket{exchange=\"okx\",type=\"kline\",symbol=\"BTC-USDT\",interval=\"1m\""),
            "missing per-symbol+interval kline latency (okx/BTC-USDT/1m)"
        );
        // _count 用于吞吐量聚合
        assert!(
            out.contains("md_ingestion_latency_ms_count{exchange=\"binance\",type=\"tick\",symbol=\"BTCUSDT\",interval=\"\"}"),
            "missing ingestion _count for throughput"
        );
        // 端到端延迟（recv-T）：与到达延迟（recv-E）对照，判断高延迟来自我们还是交易所
        assert!(
            out.contains("md_ingestion_latency_e2e_ms_bucket{exchange=\"binance\",type=\"tick\",symbol=\"BTCUSDT\",interval=\"\""),
            "missing end-to-end (recv-T) tick latency series"
        );
        // ---- 网关内部转发延迟（按 topic，Go dashboard"网关内部延迟"+"推送 TOP10"）----
        assert!(
            out.contains("md_gateway_internal_latency_ms_bucket{topic=\"tick.binance.BTCUSDT\""),
            "missing gateway_internal_latency histogram by topic"
        );
        assert!(
            out.contains("md_gateway_internal_latency_ms_count{topic=\"tick.binance.BTCUSDT\"}"),
            "missing gateway_internal_latency _count for push throughput"
        );
        // ---- WebSocket 健康（Go dashboard"活跃连接 / 慢客户端踢出 / 推送吞吐"）----
        assert!(
            out.contains("md_ws_active_clients 1"),
            "missing ws_active_clients"
        );
        assert!(
            out.contains("md_ws_subscriptions_active 3"),
            "missing md_ws_subscriptions_active"
        );
        // ---- tokio 运行时指标（1.1.1）----
        assert!(
            out.contains("md_tokio_alive_tasks "),
            "missing md_tokio_alive_tasks"
        );
        assert!(
            out.contains("md_tokio_workers "),
            "missing md_tokio_workers"
        );
        assert!(
            out.contains("md_tokio_global_queue_depth "),
            "missing md_tokio_global_queue_depth"
        );
        // 默认构建不含 jemalloc 统计
        #[cfg(not(feature = "jemalloc-stats"))]
        assert!(!out.contains("md_jemalloc_"));
        #[cfg(feature = "jemalloc-stats")]
        assert!(out.contains("md_jemalloc_allocated_bytes "));
        assert!(
            out.contains("md_ws_kicked_lagged_total 1"),
            "missing ws_kicked_lagged_total"
        );
        assert!(
            out.contains("md_ws_kicked_write_timeout_total 1"),
            "missing ws_kicked_write_timeout_total"
        );
        assert!(
            out.contains("md_ws_messages_sent_total{kind=\"tick\"}"),
            "missing ws_messages_sent_total tick"
        );
        assert!(
            out.contains("md_ws_messages_sent_total{kind=\"kline\"}"),
            "missing ws_messages_sent_total kline"
        );
        // ---- broadcast lagged ----
        assert!(
            out.contains("md_broadcast_lagged_total{topic_kind=\"tick\"} 1"),
            "missing broadcast_lagged_total"
        );
        // ---- 连接器指标 ----
        assert!(out.contains("md_connector_connected{exchange=\"binance\"} 1"));
        assert!(out.contains("md_connector_reconnect_total{exchange=\"binance\"} 3"));
        // ---- 进程级标准指标（Go process_exporter 对应）----
        assert!(
            out.contains("process_resident_memory_bytes"),
            "missing process_resident_memory_bytes"
        );
        assert!(
            out.contains("process_virtual_memory_bytes"),
            "missing process_virtual_memory_bytes"
        );
        assert!(
            out.contains("process_cpu_seconds_total"),
            "missing process_cpu_seconds_total"
        );
        assert!(out.contains("process_open_fds"), "missing process_open_fds");
        assert!(out.contains("process_max_fds"), "missing process_max_fds");
        assert!(
            out.contains("process_start_time_seconds"),
            "missing process_start_time_seconds"
        );
        assert!(
            out.contains("process_uptime_seconds"),
            "missing process_uptime_seconds"
        );
    }

    // ---- quote（1.1.0 新增）----

    /// quote 指标只新增、不改旧指标；开关关闭（无 quote 流量）时也有占位 0 值，便于面板不断线
    #[tokio::test]
    async fn metrics_handler_exposes_quote_metrics() {
        let processor = Arc::new(Processor::new_with_broadcast_capacity(100, 100, 256));
        process_metrics::init();

        // 无任何 quote 流量
        let out = metrics_handler(processor.clone(), HashMap::new()).await;
        assert!(out.contains("md_quotes_processed 0"));
        assert!(out.contains("md_quotes_dropped 0"));
        assert!(out.contains("md_ws_messages_sent_total{kind=\"quote\"} 0"));
        assert!(out.contains("md_broadcast_lagged_total{topic_kind=\"quote\"} 0"));
        // 无 quote 流量时不应有 type="quote" 的入库延迟序列
        assert!(!out.contains("type=\"quote\""));

        // 有流量后：processed 递增，且带 exchange/type/symbol 维度的入库延迟出现
        processor.handle_quote(md_domain::types::Quote {
            exchange: "okx".into(),
            symbol: "BNB-USDT-SWAP".into(),
            bid_price: "744".into(),
            ask_price: "744.1".into(),
            last_price: "744".into(),
            timestamp: 1711929600000,
            connector_receive_ts: 1711929600050,
            ..Default::default()
        });
        processor.metrics.ws_message_sent("quote");
        processor.metrics.record_broadcast_lagged("quote");
        let out = metrics_handler(processor, HashMap::new()).await;
        assert!(out.contains("md_quotes_processed 1"));
        assert!(out.contains("md_ws_messages_sent_total{kind=\"quote\"} 1"));
        assert!(out.contains("md_broadcast_lagged_total{topic_kind=\"quote\"} 1"));
        assert!(
            out.contains("md_ingestion_latency_ms_count{exchange=\"okx\",type=\"quote\",symbol=\"BNB-USDT-SWAP\",interval=\"\"}"),
            "missing quote ingestion latency series"
        );
    }

    /// 起一个假上游 WS 服务：记录每个连接收到的文本帧（`tag` 标识这是哪一个配置地址）
    async fn fake_upstream(
        tag: &'static str,
        tx: tokio::sync::mpsc::UnboundedSender<(&'static str, String)>,
    ) -> String {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    let _ = tx.send((tag, "<connected>".to_string()));
                    while let Some(Ok(msg)) = ws.next().await {
                        if let Message::Text(text) = msg {
                            let _ = tx.send((tag, text.to_string()));
                        }
                    }
                });
            }
        });
        format!("ws://127.0.0.1:{}/ws", port)
    }

    /// 验收 (c)：重启（= 用配置重新 `start_connectors`）后，每个连接器实际连到**配置里对应字段**的地址，
    /// 并且字段和连接器的对应关系正确：
    /// OKX Public → trades / tickers，OKX Business → K 线，Binance market → aggTrade / kline，Binance public → bookTicker。
    #[tokio::test]
    async fn start_connectors_connects_each_connector_to_its_configured_url() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let binance_market = fake_upstream("binance-market", tx.clone()).await;
        let binance_public = fake_upstream("binance-public", tx.clone()).await;
        let okx_public = fake_upstream("okx-public", tx.clone()).await;
        let okx_business = fake_upstream("okx-business", tx.clone()).await;

        let yaml = r#"
connectors:
  binance:
    enabled: true
    stream_base_url: "@BINANCE_MARKET@"
    stream_base_url_public: "@BINANCE_PUBLIC@"
    rest_base_url: "http://127.0.0.1:1"
    subscribe_ticks: ["BTCUSDT"]
    subscribe_klines: { "1m": ["BTCUSDT"] }
    reconnect_delay: "1s"
    ping_interval: "30s"
    quotes_enabled: true
    subscribe_quotes: ["BNBUSDT"]
  okx:
    enabled: true
    stream_base_url_public: "@OKX_PUBLIC@"
    stream_base_url_business: "@OKX_BUSINESS@"
    rest_base_url: "http://127.0.0.1:1"
    subscribe_ticks: ["BTC-USDT-SWAP"]
    subscribe_klines: { "1m": ["BTC-USDT-SWAP"] }
    reconnect_delay: "1s"
    ping_interval: "25s"
    quotes_enabled: true
    subscribe_quotes: ["BNB-USDT-SWAP"]
"#
        .replace("@BINANCE_MARKET@", &binance_market)
        .replace("@BINANCE_PUBLIC@", &binance_public)
        .replace("@OKX_PUBLIC@", &okx_public)
        .replace("@OKX_BUSINESS@", &okx_business);
        let cfg = md_config::load_config_from_str(&yaml).unwrap();

        let processor = Arc::new(Processor::new_full(1000, 1000, 4096, 1000));
        let (connectors, _metrics) = start_connectors(&cfg, &processor).await;

        // 期望：每个地址收到的订阅帧
        let expect: [(&str, &[&str]); 4] = [
            ("binance-market", &["btcusdt@aggTrade", "btcusdt@kline_1m"]),
            ("binance-public", &["bnbusdt@bookTicker"]),
            (
                "okx-public",
                &["\"channel\":\"trades\"", "\"channel\":\"tickers\""],
            ),
            ("okx-business", &["\"channel\":\"candle1m\""]),
        ];
        let mut seen: HashMap<&'static str, Vec<String>> = HashMap::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let satisfied = |seen: &HashMap<&'static str, Vec<String>>| {
            expect.iter().all(|(tag, needles)| {
                let frames = seen.get(tag).map(|v| v.join("\n")).unwrap_or_default();
                needles.iter().all(|n| frames.contains(n))
            })
        };
        while !satisfied(&seen) {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some((tag, frame))) => seen.entry(tag).or_default().push(frame),
                _ => break,
            }
        }
        // 再留一小段时间，让可能的"串线"帧也到达
        while let Ok(Some((tag, frame))) =
            tokio::time::timeout(Duration::from_millis(300), rx.recv()).await
        {
            seen.entry(tag).or_default().push(frame);
        }

        for (tag, needles) in expect {
            let frames = seen.get(tag).map(|v| v.join("\n")).unwrap_or_default();
            for n in needles {
                assert!(
                    frames.contains(n),
                    "{tag} 应收到含 {n} 的订阅帧，实际: {frames}"
                );
            }
        }
        // 不串线
        let frames_of = |tag: &str| seen.get(tag).map(|v| v.join("\n")).unwrap_or_default();
        assert!(
            !frames_of("okx-business").contains("trades")
                && !frames_of("okx-business").contains("tickers")
        );
        assert!(!frames_of("okx-public").contains("candle"));
        assert!(!frames_of("binance-market").contains("bookTicker"));
        assert!(!frames_of("binance-public").contains("aggTrade"));
        // OKX Public 地址上有两条连接：trades 连接器 + okx-quote 连接器
        let okx_public_connections = seen
            .get("okx-public")
            .map(|v| v.iter().filter(|f| *f == "<connected>").count())
            .unwrap_or(0);
        assert_eq!(
            okx_public_connections, 2,
            "okx / okx-quote 都应连 stream_base_url_public"
        );

        for connector in connectors.read().await.values() {
            let _ = connector.stop().await;
        }
    }
}
