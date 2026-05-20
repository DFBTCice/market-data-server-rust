pub mod admin_service;
pub mod market_data;

pub use admin_service::AdminServiceImpl;
pub use market_data::MarketDataServiceImpl;

// ---- 类型转换：md-domain <-> md-proto ----

/// md-domain Tick -> md-proto Tick
pub fn tick_to_proto(tick: &md_domain::types::Tick) -> md_proto::Tick {
    md_proto::Tick {
        exchange: tick.exchange.clone(),
        symbol: tick.symbol.clone(),
        timestamp: tick.timestamp,
        price: tick.price.clone(),
        quantity: tick.quantity.clone(),
        trade_id: tick.trade_id,
        is_buyer_maker: tick.is_buyer_maker,
        best_bid_price: tick.best_bid_price.clone(),
        best_bid_quantity: tick.best_bid_quantity.clone(),
        best_ask_price: tick.best_ask_price.clone(),
        best_ask_quantity: tick.best_ask_quantity.clone(),
        exchange_event_ts: tick.exchange_event_ts,
        connector_receive_ts: tick.connector_receive_ts,
    }
}

/// md-proto Tick -> md-domain Tick
pub fn tick_from_proto(tick: &md_proto::Tick) -> md_domain::types::Tick {
    md_domain::types::Tick {
        exchange: tick.exchange.clone(),
        symbol: tick.symbol.clone(),
        timestamp: tick.timestamp,
        price: tick.price.clone(),
        quantity: tick.quantity.clone(),
        trade_id: tick.trade_id,
        is_buyer_maker: tick.is_buyer_maker,
        best_bid_price: tick.best_bid_price.clone(),
        best_bid_quantity: tick.best_bid_quantity.clone(),
        best_ask_price: tick.best_ask_price.clone(),
        best_ask_quantity: tick.best_ask_quantity.clone(),
        exchange_event_ts: tick.exchange_event_ts,
        connector_receive_ts: tick.connector_receive_ts,
    }
}

/// md-domain Kline -> md-proto Kline
pub fn kline_to_proto(kline: &md_domain::types::Kline) -> md_proto::Kline {
    md_proto::Kline {
        exchange: kline.exchange.clone(),
        symbol: kline.symbol.clone(),
        interval: kline.interval.clone(),
        open_time: kline.open_time,
        open: kline.open.clone(),
        high: kline.high.clone(),
        low: kline.low.clone(),
        close: kline.close.clone(),
        volume: kline.volume.clone(),
        close_time: kline.close_time,
        quote_asset_volume: kline.quote_asset_volume.clone(),
        number_of_trades: kline.number_of_trades,
        is_closed: kline.is_closed,
        exchange_event_ts: kline.exchange_event_ts,
        connector_receive_ts: kline.connector_receive_ts,
    }
}

/// md-proto Kline -> md-domain Kline
pub fn kline_from_proto(kline: &md_proto::Kline) -> md_domain::types::Kline {
    md_domain::types::Kline {
        exchange: kline.exchange.clone(),
        symbol: kline.symbol.clone(),
        interval: kline.interval.clone(),
        open_time: kline.open_time,
        open: kline.open.clone(),
        high: kline.high.clone(),
        low: kline.low.clone(),
        close: kline.close.clone(),
        volume: kline.volume.clone(),
        close_time: kline.close_time,
        quote_asset_volume: kline.quote_asset_volume.clone(),
        number_of_trades: kline.number_of_trades,
        is_closed: kline.is_closed,
        exchange_event_ts: kline.exchange_event_ts,
        connector_receive_ts: kline.connector_receive_ts,
    }
}

/// md-connector DataType -> md-proto DataType
pub fn data_type_to_proto(dt: md_connector::DataType) -> md_proto::DataType {
    match dt {
        md_connector::DataType::Tick => md_proto::DataType::Tick,
        md_connector::DataType::Kline => md_proto::DataType::Kline,
    }
}

/// md-proto DataType -> md-connector DataType
pub fn data_type_from_proto(dt: md_proto::DataType) -> Option<md_connector::DataType> {
    match dt {
        md_proto::DataType::Tick => Some(md_connector::DataType::Tick),
        md_proto::DataType::Kline => Some(md_connector::DataType::Kline),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_roundtrip() {
        let domain_tick = md_domain::types::Tick {
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

        let proto_tick = tick_to_proto(&domain_tick);
        assert_eq!(proto_tick.exchange, "binance");
        assert_eq!(proto_tick.price, "67000.50");
        assert_eq!(proto_tick.trade_id, 12345);

        let back = tick_from_proto(&proto_tick);
        assert_eq!(back, domain_tick);
    }

    #[test]
    fn kline_roundtrip() {
        let domain_kline = md_domain::types::Kline {
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

        let proto_kline = kline_to_proto(&domain_kline);
        assert_eq!(proto_kline.exchange, "binance");
        assert_eq!(proto_kline.close, "67050.00");

        let back = kline_from_proto(&proto_kline);
        assert_eq!(back, domain_kline);
    }

    #[test]
    fn data_type_conversion() {
        assert_eq!(
            data_type_to_proto(md_connector::DataType::Tick),
            md_proto::DataType::Tick
        );
        assert_eq!(
            data_type_to_proto(md_connector::DataType::Kline),
            md_proto::DataType::Kline
        );
        assert_eq!(
            data_type_from_proto(md_proto::DataType::Tick),
            Some(md_connector::DataType::Tick)
        );
        assert_eq!(data_type_from_proto(md_proto::DataType::Unknown), None);
    }
}
