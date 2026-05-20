use crate::{data_type_from_proto, data_type_to_proto};
use md_connector::Connector;
use md_proto::AdminService;
use md_proto::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::info;

/// AdminService 实现 -- 对标 Go 版 internal/server/admin/server.go
pub struct AdminServiceImpl {
    connectors: Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>,
}

impl AdminServiceImpl {
    pub fn new(connectors: Arc<RwLock<HashMap<String, Arc<dyn Connector>>>>) -> Self {
        Self { connectors }
    }
}

#[tonic::async_trait]
impl AdminService for AdminServiceImpl {
    /// 动态添加订阅
    async fn add_subscription(
        &self,
        request: Request<SubscriptionChangeRequest>,
    ) -> Result<Response<SubscriptionChangeResponse>, Status> {
        let req = request.into_inner();

        if req.exchange.is_empty() {
            return Err(Status::invalid_argument("exchange is required"));
        }
        if req.symbols.is_empty() {
            return Err(Status::invalid_argument("at least one symbol is required"));
        }

        let data_type =
            data_type_from_proto(DataType::try_from(req.data_type).unwrap_or(DataType::Unknown))
                .ok_or_else(|| Status::invalid_argument("data_type must be TICK or KLINE"))?;

        let connectors = self.connectors.read().await;
        let exchange_key = req.exchange.to_lowercase();
        // OKX 双连接路由：KLINE → "{exchange}-kline"，TICK → "{exchange}"
        let connector_key = if data_type == md_connector::DataType::Kline {
            format!("{}-kline", exchange_key)
        } else {
            exchange_key.clone()
        };
        let connector = connectors
            .get(&connector_key)
            .or_else(|| connectors.get(&exchange_key))
            .ok_or_else(|| Status::not_found(format!("connector not found: {}", req.exchange)))?;

        let targets: Vec<md_connector::SubscriptionTarget> = req
            .symbols
            .iter()
            .map(|sym| md_connector::SubscriptionTarget {
                exchange: exchange_key.clone(),
                data_type,
                symbol: sym.clone(),
                kline_interval: if data_type == md_connector::DataType::Kline {
                    Some(req.kline_interval.clone())
                } else {
                    None
                },
            })
            .collect();

        match connector.add_subscriptions(targets).await {
            Ok(()) => {
                info!(
                    "Added {} subscriptions for {}",
                    req.symbols.len(),
                    exchange_key
                );
                Ok(Response::new(SubscriptionChangeResponse {
                    success: true,
                    message: format!(
                        "added {} {} subscriptions for {}",
                        req.symbols.len(),
                        if data_type == md_connector::DataType::Tick {
                            "tick"
                        } else {
                            "kline"
                        },
                        req.exchange
                    ),
                }))
            }
            Err(e) => Ok(Response::new(SubscriptionChangeResponse {
                success: false,
                message: format!("failed: {}", e),
            })),
        }
    }

    /// 动态移除订阅
    async fn remove_subscription(
        &self,
        request: Request<SubscriptionChangeRequest>,
    ) -> Result<Response<SubscriptionChangeResponse>, Status> {
        let req = request.into_inner();

        if req.exchange.is_empty() {
            return Err(Status::invalid_argument("exchange is required"));
        }
        if req.symbols.is_empty() {
            return Err(Status::invalid_argument("at least one symbol is required"));
        }

        let data_type =
            data_type_from_proto(DataType::try_from(req.data_type).unwrap_or(DataType::Unknown))
                .ok_or_else(|| Status::invalid_argument("data_type must be TICK or KLINE"))?;

        let connectors = self.connectors.read().await;
        let exchange_key = req.exchange.to_lowercase();
        let connector_key = if data_type == md_connector::DataType::Kline {
            format!("{}-kline", exchange_key)
        } else {
            exchange_key.clone()
        };
        let connector = connectors
            .get(&connector_key)
            .or_else(|| connectors.get(&exchange_key))
            .ok_or_else(|| Status::not_found(format!("connector not found: {}", req.exchange)))?;

        let targets: Vec<md_connector::SubscriptionTarget> = req
            .symbols
            .iter()
            .map(|sym| md_connector::SubscriptionTarget {
                exchange: exchange_key.clone(),
                data_type,
                symbol: sym.clone(),
                kline_interval: if data_type == md_connector::DataType::Kline {
                    Some(req.kline_interval.clone())
                } else {
                    None
                },
            })
            .collect();

        match connector.remove_subscriptions(targets).await {
            Ok(()) => {
                info!(
                    "Removed {} subscriptions for {}",
                    req.symbols.len(),
                    req.exchange
                );
                Ok(Response::new(SubscriptionChangeResponse {
                    success: true,
                    message: format!(
                        "removed {} {} subscriptions for {}",
                        req.symbols.len(),
                        if data_type == md_connector::DataType::Tick {
                            "tick"
                        } else {
                            "kline"
                        },
                        req.exchange
                    ),
                }))
            }
            Err(e) => Ok(Response::new(SubscriptionChangeResponse {
                success: false,
                message: format!("failed: {}", e),
            })),
        }
    }

    /// 获取当前订阅列表
    async fn get_subscriptions(
        &self,
        request: Request<GetSubscriptionsRequest>,
    ) -> Result<Response<GetSubscriptionsResponse>, Status> {
        let req = request.into_inner();
        let connectors = self.connectors.read().await;

        let mut subscriptions = Vec::new();

        for (name, connector) in connectors.iter() {
            // 如果指定了 exchange filter，跳过不匹配的（兼容 "okx-kline" 后缀）
            if !req.exchange.is_empty() {
                let filter = req.exchange.to_lowercase();
                let name_base = name.strip_suffix("-kline").unwrap_or(name);
                if name_base != filter {
                    continue;
                }
            }

            for target in connector.current_subscriptions() {
                subscriptions.push(SubscriptionInfo {
                    exchange: target.exchange,
                    data_type: data_type_to_proto(target.data_type) as i32,
                    symbol: target.symbol,
                    kline_interval: target.kline_interval.unwrap_or_default(),
                });
            }
        }

        Ok(Response::new(GetSubscriptionsResponse { subscriptions }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use md_connector::{ConnectorError, SubscriptionTarget};
    use tonic::Request;

    /// Mock Connector 用于测试
    struct MockConnector {
        name: String,
        subs: RwLock<Vec<SubscriptionTarget>>,
    }

    impl MockConnector {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                subs: RwLock::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl Connector for MockConnector {
        fn name(&self) -> &str {
            &self.name
        }

        async fn start(&self) -> Result<(), ConnectorError> {
            Ok(())
        }
        async fn stop(&self) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn add_subscriptions(
            &self,
            targets: Vec<SubscriptionTarget>,
        ) -> Result<(), ConnectorError> {
            let mut subs = self.subs.write().await;
            subs.extend(targets);
            Ok(())
        }

        async fn remove_subscriptions(
            &self,
            targets: Vec<SubscriptionTarget>,
        ) -> Result<(), ConnectorError> {
            let mut subs = self.subs.write().await;
            for t in targets {
                subs.retain(|s| s != &t);
            }
            Ok(())
        }

        fn current_subscriptions(&self) -> Vec<SubscriptionTarget> {
            // 使用 try_read 以匹配 trait 签名（同步方法）
            match self.subs.try_read() {
                Ok(subs) => subs.clone(),
                Err(_) => Vec::new(),
            }
        }
    }

    fn make_admin() -> AdminServiceImpl {
        let mut connectors: HashMap<String, Arc<dyn Connector>> = HashMap::new();
        connectors.insert("binance".into(), Arc::new(MockConnector::new("binance")));
        AdminServiceImpl::new(Arc::new(RwLock::new(connectors)))
    }

    #[tokio::test]
    async fn add_subscription_success() {
        let svc = make_admin();
        let req = Request::new(SubscriptionChangeRequest {
            exchange: "binance".into(),
            data_type: DataType::Tick as i32,
            symbols: vec!["BTCUSDT".into(), "ETHUSDT".into()],
            kline_interval: "".into(),
        });
        let resp = svc.add_subscription(req).await.unwrap().into_inner();
        assert!(resp.success);
        assert!(resp.message.contains("2"));
    }

    #[tokio::test]
    async fn add_subscription_missing_exchange() {
        let svc = make_admin();
        let req = Request::new(SubscriptionChangeRequest {
            exchange: "".into(),
            data_type: DataType::Tick as i32,
            symbols: vec!["BTCUSDT".into()],
            kline_interval: "".into(),
        });
        let result = svc.add_subscription(req).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn add_subscription_unknown_connector() {
        let svc = make_admin();
        let req = Request::new(SubscriptionChangeRequest {
            exchange: "okx".into(),
            data_type: DataType::Tick as i32,
            symbols: vec!["BTCUSDT".into()],
            kline_interval: "".into(),
        });
        let result = svc.add_subscription(req).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn remove_subscription_success() {
        let svc = make_admin();

        // 先添加
        let req = Request::new(SubscriptionChangeRequest {
            exchange: "binance".into(),
            data_type: DataType::Tick as i32,
            symbols: vec!["BTCUSDT".into()],
            kline_interval: "".into(),
        });
        svc.add_subscription(req).await.unwrap();

        // 再移除
        let req = Request::new(SubscriptionChangeRequest {
            exchange: "binance".into(),
            data_type: DataType::Tick as i32,
            symbols: vec!["BTCUSDT".into()],
            kline_interval: "".into(),
        });
        let resp = svc.remove_subscription(req).await.unwrap().into_inner();
        assert!(resp.success);
    }

    #[tokio::test]
    async fn get_subscriptions_empty() {
        let svc = make_admin();
        let req = Request::new(GetSubscriptionsRequest {
            exchange: "".into(),
        });
        let resp = svc.get_subscriptions(req).await.unwrap().into_inner();
        assert!(resp.subscriptions.is_empty());
    }

    #[tokio::test]
    async fn get_subscriptions_after_add() {
        let svc = make_admin();

        let req = Request::new(SubscriptionChangeRequest {
            exchange: "binance".into(),
            data_type: DataType::Tick as i32,
            symbols: vec!["BTCUSDT".into()],
            kline_interval: "".into(),
        });
        svc.add_subscription(req).await.unwrap();

        let req = Request::new(GetSubscriptionsRequest {
            exchange: "".into(),
        });
        let resp = svc.get_subscriptions(req).await.unwrap().into_inner();
        assert_eq!(resp.subscriptions.len(), 1);
        assert_eq!(resp.subscriptions[0].exchange, "binance");
        assert_eq!(resp.subscriptions[0].symbol, "BTCUSDT");
    }
}
