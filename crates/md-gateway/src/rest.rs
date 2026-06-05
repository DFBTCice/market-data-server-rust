use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use md_connector::Connector;
use md_processor::Processor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 共享状态
#[derive(Clone)]
pub struct AppState {
    pub processor: Arc<Processor>,
    pub connectors: Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
}

/// REST 错误响应 -- 对标 Go 版 {"error": "message"}
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// 订阅请求体 -- 对标 Go 版
#[derive(Deserialize)]
pub struct SubscriptionRequest {
    pub exchange: String,
    #[serde(rename = "type")]
    pub data_type: String,
    pub symbols: Vec<String>,
    #[serde(default)]
    pub interval: String,
}

/// 订阅信息响应 -- 对标 Go 版 protobuf SubscriptionInfo
/// Go 版用 data_type (int32): 1=TICK, 2=KLINE
#[derive(Serialize)]
struct SubscriptionInfo {
    exchange: String,
    data_type: i32,
    symbol: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    kline_interval: String,
}

/// 构建 REST 路由
pub fn routes(
    processor: Arc<Processor>,
    connectors: Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
) -> Router {
    let state = AppState {
        processor,
        connectors,
    };

    Router::new()
        .route(
            "/api/v1/subscriptions",
            get(get_subscriptions)
                .post(add_subscription)
                .delete(remove_subscription),
        )
        .route(
            "/api/v1/data/latest/tick/:exchange/:symbol",
            get(get_latest_tick),
        )
        .route(
            "/api/v1/data/latest/kline/:exchange/:symbol/:interval",
            get(get_latest_kline),
        )
        .with_state(state)
}

/// GET /api/v1/subscriptions
async fn get_subscriptions(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let connectors = state.connectors.read().await;
    let mut subs = Vec::new();

    for (_name, connector) in connectors.iter() {
        for target in connector.current_subscriptions() {
            subs.push(SubscriptionInfo {
                exchange: target.exchange,
                data_type: match target.data_type {
                    md_connector::DataType::Tick => 1,
                    md_connector::DataType::Kline => 2,
                },
                symbol: target.symbol,
                kline_interval: target.kline_interval.unwrap_or_default(),
            });
        }
    }

    Json(subs)
}

/// POST /api/v1/subscriptions
async fn add_subscription(
    State(state): State<AppState>,
    Json(req): Json<SubscriptionRequest>,
) -> impl IntoResponse {
    if req.exchange.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "exchange is required".into() })).into_response();
    }
    if req.symbols.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "at least one symbol is required".into() })).into_response();
    }

    let data_type = match req.data_type.to_uppercase().as_str() {
        "TICK" => md_connector::DataType::Tick,
        "KLINE" => md_connector::DataType::Kline,
        _ => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "type must be TICK or KLINE".into() })).into_response(),
    };

    if data_type == md_connector::DataType::Kline && req.interval.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "interval is required for KLINE".into() })).into_response();
    }

    let connectors = state.connectors.read().await;
    let exchange_key = req.exchange.to_lowercase();
    // OKX 双连接路由：KLINE → "{exchange}-kline"，TICK → "{exchange}"
    let connector_key = if data_type == md_connector::DataType::Kline {
        format!("{}-kline", exchange_key)
    } else {
        exchange_key.clone()
    };
    let connector = match connectors.get(&connector_key).or_else(|| connectors.get(&exchange_key)) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("connector not found: {}", req.exchange) })).into_response(),
    };

    let targets: Vec<md_connector::SubscriptionTarget> = req.symbols.iter().map(|sym| {
        md_connector::SubscriptionTarget {
            exchange: exchange_key.clone(),
            data_type,
            symbol: sym.clone(),
            kline_interval: if data_type == md_connector::DataType::Kline {
                Some(req.interval.clone())
            } else {
                None
            },
        }
    }).collect();

    match connector.add_subscriptions(targets).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response(),
    }
}

/// DELETE /api/v1/subscriptions
async fn remove_subscription(
    State(state): State<AppState>,
    Json(req): Json<SubscriptionRequest>,
) -> impl IntoResponse {
    if req.exchange.is_empty() || req.symbols.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "exchange and symbols are required".into() })).into_response();
    }

    let data_type = match req.data_type.to_uppercase().as_str() {
        "TICK" => md_connector::DataType::Tick,
        "KLINE" => md_connector::DataType::Kline,
        _ => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "type must be TICK or KLINE".into() })).into_response(),
    };

    let connectors = state.connectors.read().await;
    let exchange_key = req.exchange.to_lowercase();
    let connector_key = if data_type == md_connector::DataType::Kline {
        format!("{}-kline", exchange_key)
    } else {
        exchange_key.clone()
    };
    let connector = match connectors.get(&connector_key).or_else(|| connectors.get(&exchange_key)) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("connector not found: {}", req.exchange) })).into_response(),
    };

    let targets: Vec<md_connector::SubscriptionTarget> = req.symbols.iter().map(|sym| {
        md_connector::SubscriptionTarget {
            exchange: exchange_key.clone(),
            data_type,
            symbol: sym.clone(),
            kline_interval: if data_type == md_connector::DataType::Kline {
                Some(req.interval.clone())
            } else {
                None
            },
        }
    }).collect();

    match connector.remove_subscriptions(targets).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response(),
    }
}

/// GET /api/v1/data/latest/tick/{exchange}/{symbol}
async fn get_latest_tick(
    State(state): State<AppState>,
    Path((exchange, symbol)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.processor.get_latest_tick(&exchange, &symbol) {
        Some(tick) => (StatusCode::OK, Json(tick.as_ref().clone())).into_response(),
        None => (StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("no tick data for {}:{}", exchange, symbol) })).into_response(),
    }
}

/// GET /api/v1/data/latest/kline/{exchange}/{symbol}/{interval}
async fn get_latest_kline(
    State(state): State<AppState>,
    Path((exchange, symbol, interval)): Path<(String, String, String)>,
) -> impl IntoResponse {
    match state.processor.get_latest_kline(&exchange, &symbol, &interval) {
        Some(kline) => (StatusCode::OK, Json(kline.as_ref().clone())).into_response(),
        None => (StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("no kline data for {}:{}:{}", exchange, symbol, interval) })).into_response(),
    }
}
