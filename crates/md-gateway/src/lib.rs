pub mod rest;
pub mod ws;

use axum::Router;
use md_connector::Connector;
use md_processor::Processor;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

/// API Gateway 配置
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub listen_address: String,
}

/// API Gateway -- 对标 Go 版 internal/apigateway/gateway.go
///
/// 直接访问 Processor 和 Connector，不走 gRPC（进程内访问）
pub struct Gateway {
    pub processor: Arc<Processor>,
    pub connectors: Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
    /// Gateway WS（`/ws/v1/data`）单次写超时；`None` = 不限（1.0.0/1.1.0 的行为）。
    /// 由 `api_gateway.ws_write_wait` 配置；Legacy `/ws` 不使用。
    ws_write_wait: Option<Duration>,
}

impl Gateway {
    pub fn new(
        processor: Arc<Processor>,
        connectors: Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
    ) -> Self {
        Self {
            processor,
            connectors,
            ws_write_wait: None,
        }
    }

    /// 设置 Gateway WS 的单次写超时（对应配置 `api_gateway.ws_write_wait`）。
    /// 客户端停读/半死导致一次写入阻塞超过该时长，连接会被主动断开并释放其全部 broadcast 接收端。
    /// `Duration::ZERO` 表示关闭（不限时）。
    pub fn with_ws_write_wait(mut self, write_wait: Duration) -> Self {
        self.ws_write_wait = if write_wait.is_zero() {
            None
        } else {
            Some(write_wait)
        };
        self
    }

    /// 构建 axum Router
    pub fn router(&self) -> Router {
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        Router::new()
            .merge(rest::routes(
                self.processor.clone(),
                self.connectors.clone(),
            ))
            .merge(ws::routes_with_write_wait(
                self.processor.clone(),
                self.ws_write_wait,
            ))
            .layer(cors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use md_domain::types::Tick;
    use tower::util::ServiceExt;

    fn make_gateway() -> Gateway {
        let proc = Arc::new(Processor::new(100, 100));
        let connectors: Arc<RwLock<HashMap<String, Arc<dyn Connector>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        Gateway::new(proc, connectors)
    }

    #[tokio::test]
    async fn get_latest_tick_not_found() {
        let gw = make_gateway();
        let app = gw.router();

        let req = Request::builder()
            .uri("/api/v1/data/latest/tick/binance/BTCUSDT")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_latest_tick_found() {
        let gw = make_gateway();

        // 插入数据
        gw.processor.handle_tick(Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            price: "67000.50".into(),
            timestamp: 1711929600000,
            ..Default::default()
        });

        let app = gw.router();
        let req = Request::builder()
            .uri("/api/v1/data/latest/tick/binance/BTCUSDT")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let tick: Tick = serde_json::from_slice(&body).unwrap();
        assert_eq!(tick.exchange, "binance");
        assert_eq!(tick.price, "67000.50");
    }

    #[tokio::test]
    async fn get_latest_kline_not_found() {
        let gw = make_gateway();
        let app = gw.router();

        let req = Request::builder()
            .uri("/api/v1/data/latest/kline/binance/BTCUSDT/1m")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_latest_kline_found() {
        let gw = make_gateway();

        gw.processor.handle_kline(md_domain::types::Kline {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            close: "67050.00".into(),
            ..Default::default()
        });

        let app = gw.router();
        let req = Request::builder()
            .uri("/api/v1/data/latest/kline/binance/BTCUSDT/1m")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let kline: md_domain::types::Kline = serde_json::from_slice(&body).unwrap();
        assert_eq!(kline.close, "67050.00");
    }

    #[tokio::test]
    async fn get_subscriptions_empty() {
        let gw = make_gateway();
        let app = gw.router();

        let req = Request::builder()
            .uri("/api/v1/subscriptions")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let subs: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(subs, serde_json::json!([]));
    }
}
