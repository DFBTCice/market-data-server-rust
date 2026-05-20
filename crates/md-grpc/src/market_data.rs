use crate::{kline_to_proto, tick_to_proto};
use md_processor::{BroadcastEvent, Processor};
use md_proto::MarketDataService;
use md_proto::*;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

/// MarketDataService 实现 -- 对标 Go 版 internal/server/grpc/server.go
pub struct MarketDataServiceImpl {
    processor: Arc<Processor>,
}

impl MarketDataServiceImpl {
    pub fn new(processor: Arc<Processor>) -> Self {
        Self { processor }
    }
}

#[tonic::async_trait]
impl MarketDataService for MarketDataServiceImpl {
    /// 订阅实时 Tick 数据
    async fn subscribe_ticks(
        &self,
        request: Request<SubscriptionRequest>,
    ) -> Result<Response<Self::SubscribeTicksStream>, Status> {
        let req = request.into_inner();

        if req.exchange.is_empty() {
            return Err(Status::invalid_argument("exchange is required"));
        }
        if req.symbols.is_empty() {
            return Err(Status::invalid_argument("at least one symbol is required"));
        }

        let processor = self.processor.clone();
        let exchange = req.exchange.clone();
        let symbols = req.symbols.clone();

        let (tx, rx) = mpsc::channel(256);

        // 为每个 symbol 创建订阅
        for symbol in &symbols {
            let topic = md_domain::topic::tick_topic(&exchange, symbol);
            let mut sub = processor.subscribe(&topic);
            let tx = tx.clone();

            let metrics = processor.metrics.clone();
            tokio::spawn(async move {
                loop {
                    match sub.rx.recv().await {
                        Ok(BroadcastEvent::Tick(tick, emit_at)) => {
                            let proto_tick = tick_to_proto(&tick);
                            if tx.send(Ok(proto_tick)).await.is_err() {
                                break; // 客户端断开
                            }
                            metrics.ws_message_sent("tick");
                            metrics.record_gateway_forward_latency_ms(
                                emit_at.elapsed().as_millis() as u64,
                            );
                        }
                        Ok(_) => {} // 忽略非 Tick 事件
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            metrics.record_broadcast_lagged("tick");
                            warn!("tick subscriber lagged {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            });
        }

        // 丢弃最后一个 tx，这样当所有 spawn 结束时 rx 会关闭
        drop(tx);

        info!(
            "SubscribeTicks: exchange={}, symbols={:?}",
            exchange, symbols
        );
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type SubscribeTicksStream = ReceiverStream<Result<Tick, Status>>;

    /// 订阅实时 Kline 数据
    async fn subscribe_klines(
        &self,
        request: Request<SubscriptionRequest>,
    ) -> Result<Response<Self::SubscribeKlinesStream>, Status> {
        let req = request.into_inner();

        if req.exchange.is_empty() {
            return Err(Status::invalid_argument("exchange is required"));
        }
        if req.symbols.is_empty() {
            return Err(Status::invalid_argument("at least one symbol is required"));
        }
        if req.kline_interval.is_empty() {
            return Err(Status::invalid_argument(
                "kline_interval is required for kline subscription",
            ));
        }

        let processor = self.processor.clone();
        let exchange = req.exchange.clone();
        let symbols = req.symbols.clone();
        let interval = req.kline_interval.clone();

        let (tx, rx) = mpsc::channel(256);

        for symbol in &symbols {
            let topic = md_domain::topic::kline_topic(&exchange, symbol, &interval);
            let mut sub = processor.subscribe(&topic);
            let tx = tx.clone();

            let metrics = processor.metrics.clone();
            tokio::spawn(async move {
                loop {
                    match sub.rx.recv().await {
                        Ok(BroadcastEvent::Kline(kline, emit_at)) => {
                            let proto_kline = kline_to_proto(&kline);
                            if tx.send(Ok(proto_kline)).await.is_err() {
                                break;
                            }
                            metrics.ws_message_sent("kline");
                            metrics.record_gateway_forward_latency_ms(
                                emit_at.elapsed().as_millis() as u64,
                            );
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            metrics.record_broadcast_lagged("kline");
                            warn!("kline subscriber lagged {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            });
        }

        drop(tx);

        info!(
            "SubscribeKlines: exchange={}, symbols={:?}, interval={}",
            exchange, symbols, interval
        );
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type SubscribeKlinesStream = ReceiverStream<Result<Kline, Status>>;

    /// 获取最新 Tick
    async fn get_latest_tick(
        &self,
        request: Request<SingleTickDataRequest>,
    ) -> Result<Response<Tick>, Status> {
        let req = request.into_inner();

        if req.exchange.is_empty() {
            return Err(Status::invalid_argument("exchange is required"));
        }
        if req.symbol.is_empty() {
            return Err(Status::invalid_argument("symbol is required"));
        }

        match self.processor.get_latest_tick(&req.exchange, &req.symbol) {
            Some(tick) => Ok(Response::new(tick_to_proto(&tick))),
            None => Err(Status::not_found(format!(
                "no tick data for {}:{}",
                req.exchange, req.symbol
            ))),
        }
    }

    /// 获取最新 Kline
    async fn get_latest_kline(
        &self,
        request: Request<SingleKlineDataRequest>,
    ) -> Result<Response<Kline>, Status> {
        let req = request.into_inner();

        if req.exchange.is_empty() {
            return Err(Status::invalid_argument("exchange is required"));
        }
        if req.symbol.is_empty() {
            return Err(Status::invalid_argument("symbol is required"));
        }
        if req.interval.is_empty() {
            return Err(Status::invalid_argument("interval is required"));
        }

        match self
            .processor
            .get_latest_kline(&req.exchange, &req.symbol, &req.interval)
        {
            Some(kline) => Ok(Response::new(kline_to_proto(&kline))),
            None => Err(Status::not_found(format!(
                "no kline data for {}:{}:{}",
                req.exchange, req.symbol, req.interval
            ))),
        }
    }

    /// 获取历史 Kline（未实现）
    async fn get_historical_klines(
        &self,
        _request: Request<HistoricalKlinesRequest>,
    ) -> Result<Response<HistoricalKlinesResponse>, Status> {
        Err(Status::unimplemented(
            "GetHistoricalKlines is not yet implemented",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Request;

    fn make_processor() -> Arc<Processor> {
        Arc::new(Processor::new(100, 100))
    }

    #[tokio::test]
    async fn get_latest_tick_not_found() {
        let svc = MarketDataServiceImpl::new(make_processor());
        let req = Request::new(SingleTickDataRequest {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
        });
        let result = svc.get_latest_tick(req).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn get_latest_tick_found() {
        let proc = make_processor();
        let svc = MarketDataServiceImpl::new(proc.clone());

        // 手动插入数据
        proc.handle_tick(md_domain::types::Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            price: "67000.50".into(),
            timestamp: 1711929600000,
            ..Default::default()
        });

        let req = Request::new(SingleTickDataRequest {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
        });
        let result = svc.get_latest_tick(req).await.unwrap();
        let tick = result.into_inner();
        assert_eq!(tick.exchange, "binance");
        assert_eq!(tick.price, "67000.50");
    }

    #[tokio::test]
    async fn get_latest_tick_missing_exchange() {
        let svc = MarketDataServiceImpl::new(make_processor());
        let req = Request::new(SingleTickDataRequest {
            exchange: "".into(),
            symbol: "BTCUSDT".into(),
        });
        let result = svc.get_latest_tick(req).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn get_latest_kline_not_found() {
        let svc = MarketDataServiceImpl::new(make_processor());
        let req = Request::new(SingleKlineDataRequest {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
        });
        let result = svc.get_latest_kline(req).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn get_latest_kline_found() {
        let proc = make_processor();
        let svc = MarketDataServiceImpl::new(proc.clone());

        proc.handle_kline(md_domain::types::Kline {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            close: "67050.00".into(),
            ..Default::default()
        });

        let req = Request::new(SingleKlineDataRequest {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
        });
        let result = svc.get_latest_kline(req).await.unwrap();
        let kline = result.into_inner();
        assert_eq!(kline.close, "67050.00");
    }

    #[tokio::test]
    async fn get_latest_kline_missing_interval() {
        let svc = MarketDataServiceImpl::new(make_processor());
        let req = Request::new(SingleKlineDataRequest {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "".into(),
        });
        let result = svc.get_latest_kline(req).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn subscribe_ticks_validation() {
        let svc = MarketDataServiceImpl::new(make_processor());

        // 空 exchange
        let req = Request::new(SubscriptionRequest {
            exchange: "".into(),
            symbols: vec!["BTCUSDT".into()],
            kline_interval: "".into(),
        });
        let result = svc.subscribe_ticks(req).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

        // 空 symbols
        let req = Request::new(SubscriptionRequest {
            exchange: "binance".into(),
            symbols: vec![],
            kline_interval: "".into(),
        });
        let result = svc.subscribe_ticks(req).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn subscribe_klines_validation() {
        let svc = MarketDataServiceImpl::new(make_processor());

        // 缺少 interval
        let req = Request::new(SubscriptionRequest {
            exchange: "binance".into(),
            symbols: vec!["BTCUSDT".into()],
            kline_interval: "".into(),
        });
        let result = svc.subscribe_klines(req).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn subscribe_ticks_receives_data() {
        let proc = make_processor();
        let svc = MarketDataServiceImpl::new(proc.clone());

        let req = Request::new(SubscriptionRequest {
            exchange: "binance".into(),
            symbols: vec!["BTCUSDT".into()],
            kline_interval: "".into(),
        });

        let resp = svc.subscribe_ticks(req).await.unwrap();
        let mut stream = resp.into_inner();

        // 发送数据
        proc.handle_tick(md_domain::types::Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            price: "67000.50".into(),
            timestamp: 1711929600000,
            ..Default::default()
        });

        // 接收
        use tokio_stream::StreamExt;
        let tick = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream closed")
            .expect("grpc error");

        assert_eq!(tick.exchange, "binance");
        assert_eq!(tick.price, "67000.50");
    }

    #[tokio::test]
    async fn get_historical_klines_unimplemented() {
        let svc = MarketDataServiceImpl::new(make_processor());
        let req = Request::new(HistoricalKlinesRequest {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            start_time: 0,
            end_time: 0,
            limit: 100,
        });
        let result = svc.get_historical_klines(req).await;
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unimplemented);
    }
}
