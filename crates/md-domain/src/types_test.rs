#[cfg(test)]
mod tick_json_compat {
    use crate::types::Tick;
    use serde_json::Value;

    /// 比较两个 JSON 字符串，忽略字段顺序
    #[allow(dead_code)]
    fn assert_json_eq(rust_json: &str, expected_json: &str) {
        let rust_val: Value = serde_json::from_str(rust_json).unwrap();
        let expected_val: Value = serde_json::from_str(expected_json).unwrap();
        assert_eq!(
            rust_val, expected_val,
            "\nRust:     {}\nExpected: {}",
            rust_json, expected_json
        );
    }

    #[test]
    fn full_tick_all_fields_present() {
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
        let json = serde_json::to_string(&tick).unwrap();
        // 所有字段都有值，全部输出
        assert!(json.contains("\"exchange\":\"binance\""));
        assert!(json.contains("\"is_buyer_maker\":true"));
        assert!(json.contains("\"trade_id\":12345"));
    }

    #[test]
    fn zero_fields_omitted_like_go() {
        // Go 版 omitempty 行为：
        // - bool false -> omitted
        // - int64 0 -> omitted
        // - string "" -> omitted
        let tick = Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            timestamp: 1234567890,
            price: "100.0".into(),
            // quantity 为空 -> omitted
            quantity: String::new(),
            // trade_id 为 0 -> omitted
            trade_id: 0,
            // is_buyer_maker 为 false -> omitted
            is_buyer_maker: false,
            // 其他字段全部为零值 -> omitted
            best_bid_price: String::new(),
            best_bid_quantity: String::new(),
            best_ask_price: String::new(),
            best_ask_quantity: String::new(),
            exchange_event_ts: 0,
            connector_receive_ts: 0,
        };
        let json = serde_json::to_string(&tick).unwrap();

        // 应该只包含非零值字段
        let expected =
            r#"{"exchange":"binance","symbol":"BTCUSDT","timestamp":1234567890,"price":"100.0"}"#;
        assert_json_eq(&json, expected);
    }

    #[test]
    fn only_exchange_and_symbol() {
        // 最小情况：只有 exchange 和 symbol 有值
        let tick = Tick {
            exchange: "okx".into(),
            symbol: "ETH-USDT-SWAP".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&tick).unwrap();
        let expected = r#"{"exchange":"okx","symbol":"ETH-USDT-SWAP"}"#;
        assert_json_eq(&json, expected);
    }

    #[test]
    fn field_order_matches_go() {
        // Go encoding/json 按 struct 字段定义顺序输出
        // serde_json 也按 struct 字段定义顺序输出（BTreeMap 或 struct serializer）
        let tick = Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            timestamp: 100,
            price: "1.0".into(),
            quantity: "2.0".into(),
            trade_id: 1,
            is_buyer_maker: true,
            best_bid_price: "0.9".into(),
            best_bid_quantity: "10".into(),
            best_ask_price: "1.1".into(),
            best_ask_quantity: "20".into(),
            exchange_event_ts: 101,
            connector_receive_ts: 102,
        };
        let json = serde_json::to_string(&tick).unwrap();

        // 验证字段顺序：exchange, symbol, timestamp, price, quantity, trade_id, ...
        let exchange_pos = json.find("\"exchange\":").unwrap();
        let symbol_pos = json.find("\"symbol\":").unwrap();
        let timestamp_pos = json.find("\"timestamp\":").unwrap();
        let price_pos = json.find("\"price\":").unwrap();
        let quantity_pos = json.find("\"quantity\":").unwrap();
        let trade_id_pos = json.find("\"trade_id\":").unwrap();

        assert!(exchange_pos < symbol_pos);
        assert!(symbol_pos < timestamp_pos);
        assert!(timestamp_pos < price_pos);
        assert!(price_pos < quantity_pos);
        assert!(quantity_pos < trade_id_pos);
    }
}

#[cfg(test)]
mod kline_json_compat {
    use crate::types::Kline;
    use serde_json::Value;

    #[allow(dead_code)]
    fn assert_json_eq(rust_json: &str, expected_json: &str) {
        let rust_val: Value = serde_json::from_str(rust_json).unwrap();
        let expected_val: Value = serde_json::from_str(expected_json).unwrap();
        assert_eq!(rust_val, expected_val);
    }

    #[test]
    fn full_kline_all_fields() {
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
        let json = serde_json::to_string(&kline).unwrap();
        assert!(json.contains("\"is_closed\":true"));
        assert!(json.contains("\"number_of_trades\":500"));
    }

    #[test]
    fn zero_fields_omitted() {
        let kline = Kline {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            open_time: 1000,
            open: "100.0".into(),
            // 其他为零值
            ..Default::default()
        };
        let json = serde_json::to_string(&kline).unwrap();
        assert!(!json.contains("\"high\""));
        assert!(!json.contains("\"is_closed\""));
        assert!(!json.contains("\"close_time\""));
    }
}
