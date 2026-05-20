// 生成的 protobuf 类型和 gRPC 服务
// tonic_build 在 build.rs 中编译 proto 文件，生成代码到 OUT_DIR

pub mod marketdata {
    tonic::include_proto!("marketdata");
}

pub mod admin {
    tonic::include_proto!("admin");
}

// 便捷 re-export
pub use marketdata::market_data_service_client::MarketDataServiceClient;
pub use marketdata::market_data_service_server::{MarketDataService, MarketDataServiceServer};
pub use marketdata::{
    HistoricalKlinesRequest, HistoricalKlinesResponse, Kline, SingleKlineDataRequest,
    SingleTickDataRequest, SubscriptionRequest, Tick,
};

pub use admin::admin_service_client::AdminServiceClient;
pub use admin::admin_service_server::{AdminService, AdminServiceServer};
pub use admin::{
    DataType, GetSubscriptionsRequest, GetSubscriptionsResponse, SubscriptionChangeRequest,
    SubscriptionChangeResponse, SubscriptionInfo,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_compiles_successfully() {
        // 验证生成的类型可以实例化
        let tick = Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            timestamp: 1711929600000,
            price: "67000.50".into(),
            quantity: "0.1".into(),
            trade_id: 12345,
            is_buyer_maker: true,
            best_bid_price: "67000.00".into(),
            best_bid_quantity: "1.5".into(),
            best_ask_price: "67001.00".into(),
            best_ask_quantity: "2.0".into(),
            exchange_event_ts: 1711929600001,
            connector_receive_ts: 1711929600002,
        };
        assert_eq!(tick.exchange, "binance");
        assert_eq!(tick.symbol, "BTCUSDT");
        assert_eq!(tick.price, "67000.50");
        assert!(tick.is_buyer_maker);
    }

    #[test]
    fn kline_type_works() {
        let kline = Kline {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            open_time: 1711929600000,
            open: "67000.00".into(),
            high: "67100.00".into(),
            low: "66900.00".into(),
            close: "67050.00".into(),
            volume: "100.5".into(),
            close_time: 1711929659999,
            quote_asset_volume: "6730000.0".into(),
            number_of_trades: 500,
            is_closed: true,
            exchange_event_ts: 1711929600000,
            connector_receive_ts: 1711929600001,
        };
        assert_eq!(kline.exchange, "binance");
        assert!(kline.is_closed);
    }

    #[test]
    fn market_data_service_trait_exists() {
        // 验证 MarketDataServiceServer trait 可以被引用
        // 这确保 tonic 生成了正确的服务定义
        fn _assert_server<T: MarketDataService>() {}
        fn _assert_client() {
            let _ = MarketDataServiceClient::<tonic::transport::Channel>::new;
        }
    }

    #[test]
    fn admin_service_trait_exists() {
        fn _assert_server<T: AdminService>() {}
        fn _assert_client() {
            let _ = AdminServiceClient::<tonic::transport::Channel>::new;
        }
    }

    #[test]
    fn data_type_enum_variants() {
        assert_eq!(DataType::Unknown as i32, 0);
        assert_eq!(DataType::Tick as i32, 1);
        assert_eq!(DataType::Kline as i32, 2);
    }

    #[test]
    fn subscription_request_fields() {
        let req = SubscriptionRequest {
            exchange: "binance".into(),
            symbols: vec!["BTCUSDT".into(), "ETHUSDT".into()],
            kline_interval: "1m".into(),
        };
        assert_eq!(req.exchange, "binance");
        assert_eq!(req.symbols.len(), 2);
    }

    #[test]
    fn tick_json_serialization_matches_go_format() {
        let tick = Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            timestamp: 1234567890,
            price: "100.0".into(),
            quantity: "0.5".into(),
            trade_id: 99,
            is_buyer_maker: true,
            best_bid_price: "99.0".into(),
            best_bid_quantity: "1.0".into(),
            best_ask_price: "101.0".into(),
            best_ask_quantity: "2.0".into(),
            exchange_event_ts: 1234567891,
            connector_receive_ts: 1234567892,
        };
        let json = serde_json::to_string(&tick).unwrap();
        // prost 生成的 JSON 字段名是 snake_case（与 Go protobuf struct tags 一致）
        // Go: json:"exchange,omitempty" -> "exchange"
        // Go: json:"trade_id,omitempty" -> "trade_id"
        // Go: json:"is_buyer_maker,omitempty" -> "is_buyer_maker"
        assert!(json.contains("\"exchange\":\"binance\""));
        assert!(json.contains("\"symbol\":\"BTCUSDT\""));
        assert!(json.contains("\"trade_id\":99"));
        assert!(json.contains("\"is_buyer_maker\":true"));
        assert!(json.contains("\"best_bid_price\":\"99.0\""));
        assert!(json.contains("\"connector_receive_ts\":1234567892"));
    }

    #[test]
    fn tick_prost_default_is_zero_values() {
        let tick = Tick::default();
        assert_eq!(tick.exchange, "");
        assert_eq!(tick.timestamp, 0);
        assert!(!tick.is_buyer_maker);
        assert_eq!(tick.trade_id, 0);
    }

    #[test]
    fn admin_data_type_serde_roundtrip() {
        let dt = DataType::Tick;
        let json = serde_json::to_string(&dt).unwrap();
        // prost enum serde 序列化为字符串名
        assert_eq!(json, "\"Tick\"");
        let back: DataType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, DataType::Tick);

        let dt2 = DataType::Kline;
        let json2 = serde_json::to_string(&dt2).unwrap();
        assert_eq!(json2, "\"Kline\"");
    }
}
