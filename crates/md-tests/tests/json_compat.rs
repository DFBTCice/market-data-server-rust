/// 兼容性测试：验证 Rust 序列化输出与既有 JSON 快照一致。
///
/// 运行方式：
///   cargo test -p md-tests -- --nocapture
///
/// 前置条件：
///   1. 运行 scripts/capture-snapshots.sh 抓取快照
///   2. 快照文件在 crates/md-tests/tests/fixtures/ 下
///
/// 快照来源（1.1.0 起，见 evidence/1.1.0/ 与 CHANGELOG）：
///   - tick_*.json / kline_*.json / subscriptions.json：最初由 Go 版抓取（2026-05-05）
///   - error_*.json / ws_gateway_*.json：2026-10-09 从 NAS 测试服务器 192.168.1.198 上的
///     Rust 1.0.0 基线抓取（任务书要求不触碰 Go 版 192.168.1.153，故未从 Go 版取）
///
/// 1.1.0 起快照缺失或为空文件会让测试**失败**，不再静默 SKIP 后报告通过：
/// 静默跳过会让 "兼容测试通过" 变成无意义的空话。
use md_domain::types::{Kline, Quote, Tick};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn fixtures_dir() -> &'static Path {
    Path::new("tests/fixtures")
}

/// 加载快照文件。缺失、为空或含 "跳过" 标记都视为失败（不再静默 SKIP）。
fn load_fixture(name: &str) -> Option<String> {
    let path = fixtures_dir().join(name);
    assert!(
        path.exists(),
        "fixture {} 不存在：请先运行 scripts/capture-snapshots.sh（缺快照 = 测试没有真正执行）",
        name
    );
    let content = fs::read_to_string(&path).unwrap();
    assert!(
        !content.trim().is_empty() && !content.contains("跳过"),
        "fixture {} 为空或为占位内容：请重新抓取（空快照会让兼容测试空转）",
        name
    );
    Some(content)
}

/// 比较两个 JSON 值，忽略字段顺序
fn assert_json_equivalent(rust_json: &str, go_snapshot: &str, context: &str) {
    let rust_val: Value = serde_json::from_str(rust_json)
        .unwrap_or_else(|e| panic!("Rust JSON parse error in {}: {}", context, e));
    let go_val: Value = serde_json::from_str(go_snapshot)
        .unwrap_or_else(|e| panic!("Go snapshot parse error in {}: {}", context, e));

    if rust_val != go_val {
        // 打印详细的 diff
        let rust_pretty = serde_json::to_string_pretty(&rust_val).unwrap();
        let go_pretty = serde_json::to_string_pretty(&go_val).unwrap();
        panic!(
            "JSON mismatch in {}:\n--- Rust ---\n{}\n--- Go ---\n{}",
            context, rust_pretty, go_pretty
        );
    }
}

#[test]
fn tick_snapshot_binance_btcusdt() {
    let go_json = match load_fixture("tick_binance_btcusdt.json") {
        Some(j) => j,
        None => {
            eprintln!(
                "SKIP: tick_binance_btcusdt.json not found. Run scripts/capture-snapshots.sh"
            );
            return;
        }
    };

    // 反序列化 Go 快照 -> 重新序列化 -> 比较
    // 这样可以验证 serde 的 roundtrip 兼容性
    let tick: Tick = serde_json::from_str(&go_json).unwrap();
    let rust_json = serde_json::to_string(&tick).unwrap();

    assert_json_equivalent(&rust_json, &go_json, "tick_binance_btcusdt");
}

#[test]
fn kline_snapshot_binance_btcusdt_1m() {
    let go_json = match load_fixture("kline_binance_btcusdt_1m.json") {
        Some(j) => j,
        None => {
            eprintln!("SKIP: kline_binance_btcusdt_1m.json not found");
            return;
        }
    };

    let kline: Kline = serde_json::from_str(&go_json).unwrap();
    let rust_json = serde_json::to_string(&kline).unwrap();

    assert_json_equivalent(&rust_json, &go_json, "kline_binance_btcusdt_1m");
}

#[test]
fn ws_gateway_message_format() {
    let go_json = match load_fixture("ws_gateway_tick_push.json") {
        Some(j) => j,
        None => {
            eprintln!("SKIP: ws_gateway_tick_push.json not found");
            return;
        }
    };

    // 验证 Gateway WS 格式：{"type": "tick", "topic": "...", "data": {...}}
    let val: Value = serde_json::from_str(&go_json).unwrap();
    assert!(
        val.get("type").is_some(),
        "Gateway WS must have 'type' field"
    );
    assert!(
        val.get("topic").is_some(),
        "Gateway WS must have 'topic' field"
    );
    assert!(
        val.get("data").is_some(),
        "Gateway WS must have 'data' field"
    );

    // 验证 data 内容可以反序列化为 Tick
    let data = val.get("data").unwrap().to_string();
    let _tick: Tick = serde_json::from_str(&data).unwrap();
}

#[test]
fn error_response_format() {
    let go_json = match load_fixture("error_not_found.json") {
        Some(j) => j,
        None => {
            eprintln!("SKIP: error_not_found.json not found");
            return;
        }
    };

    // 验证错误格式：{"error": "message"}
    let val: Value = serde_json::from_str(&go_json).unwrap();
    assert!(
        val.get("error").is_some(),
        "Error response must have 'error' field"
    );
    assert!(
        val.get("error").unwrap().is_string(),
        "'error' must be a string"
    );
}

// ======================================================================
// 以下为 1.1.0 补充：覆盖原先没有断言的快照，并锁定 quote 新增不影响既有格式
// ======================================================================

/// 所有快照必须非空（防止抓取脚本失败后留下 0 字节文件而测试仍"通过"）
#[test]
fn all_fixtures_are_non_empty() {
    let mut count = 0;
    for entry in fs::read_dir(fixtures_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            !content.trim().is_empty(),
            "{} 是空快照",
            path.display()
        );
        let _: Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("{} 不是合法 JSON: {}", path.display(), e));
        count += 1;
    }
    assert!(count >= 8, "快照数量异常：只找到 {} 个", count);
}

/// 兼容约束 2：GET /api/v1/subscriptions 的结构与 data_type 数值不变（TICK=1, KLINE=2）
#[test]
fn subscriptions_snapshot_shape_and_data_type_values() {
    let json = load_fixture("subscriptions.json").unwrap();
    let val: Value = serde_json::from_str(&json).unwrap();
    let arr = val.as_array().expect("subscriptions must be a JSON array");
    assert!(!arr.is_empty());
    let mut saw_tick = false;
    let mut saw_kline = false;
    for item in arr {
        let dt = item["data_type"].as_i64().expect("data_type must be integer");
        assert!(item["exchange"].is_string() && item["symbol"].is_string());
        match dt {
            1 => {
                saw_tick = true;
                // TICK 无 kline_interval 字段
                assert!(item.get("kline_interval").is_none(), "TICK 不应带 kline_interval: {}", item);
            }
            2 => {
                saw_kline = true;
                assert!(item["kline_interval"].is_string(), "KLINE 必须带 kline_interval: {}", item);
            }
            other => panic!("旧快照中出现未知 data_type={}（QUOTE=3 只应在开启 quote 后新增出现）", other),
        }
    }
    assert!(saw_tick && saw_kline, "快照应同时包含 TICK(1) 与 KLINE(2)");
}

#[test]
fn error_bad_request_format() {
    let json = load_fixture("error_bad_request.json").unwrap();
    let val: Value = serde_json::from_str(&json).unwrap();
    assert!(val["error"].is_string());
    // 非法 type 的错误文案与 1.0.0 完全一致（1.1.0 起 type 可为 QUOTE，但文案未改）
    assert_eq!(val["error"], "type must be TICK or KLINE");
}

#[test]
fn ws_gateway_subscribe_response_format() {
    let json = load_fixture("ws_gateway_subscribe_response.json").unwrap();
    let val: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(val["action"], "subscribe");
    assert_eq!(val["status"], "success");
    assert!(val["stream"].as_str().unwrap().starts_with("tick."));
}

/// Gateway tick 推送：快照 -> Tick -> 再序列化后，字段名、字段顺序、取值逐字节一致
/// （比 Value 比较更严格：Value 比较会忽略字段顺序）
#[test]
fn ws_gateway_tick_push_data_is_byte_stable() {
    let json = load_fixture("ws_gateway_tick_push.json").unwrap();
    let val: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(val["type"], "tick");
    let data_raw = serde_json::to_string(&val["data"]).unwrap();
    let tick: Tick = serde_json::from_str(&data_raw).unwrap();
    let reserialized = serde_json::to_string(&tick).unwrap();
    // serde_json 的 Value 默认按 key 排序，不能用来比顺序；这里直接比较 Rust 结构体的输出
    // 与快照里 data 的 key 顺序（取自 1.0.0 基线原始推送行）
    let expected_order = [
        "exchange", "symbol", "timestamp", "price", "quantity", "trade_id",
        "exchange_event_ts", "connector_receive_ts",
    ];
    let mut last = 0usize;
    for key in expected_order {
        let pos = reserialized
            .find(&format!("\"{}\"", key))
            .unwrap_or_else(|| panic!("字段 {} 丢失: {}", key, reserialized));
        assert!(pos >= last, "字段顺序变化：{} 出现在前面字段之前: {}", key, reserialized);
        last = pos;
    }
    assert_eq!(
        serde_json::from_str::<Value>(&reserialized).unwrap(),
        val["data"],
        "tick 重序列化后与快照取值不一致"
    );
}

/// quote 的 JSON 结构不含 tick/kline 专有字段，且无 omitempty：空值字段仍然输出
#[test]
fn quote_json_has_all_fields_even_when_empty() {
    let q = Quote {
        exchange: "binance".into(),
        symbol: "BNBUSDT".into(),
        bid_price: "743.90".into(),
        bid_qty: "12.3".into(),
        ask_price: "743.91".into(),
        ask_qty: "8.1".into(),
        last_price: String::new(), // 币安 bookTicker 无成交价
        timestamp: 1791531605419,
        connector_receive_ts: 1791531605962,
    };
    let json = serde_json::to_string(&q).unwrap();
    assert_eq!(
        json,
        r#"{"exchange":"binance","symbol":"BNBUSDT","bid_price":"743.90","bid_qty":"12.3","ask_price":"743.91","ask_qty":"8.1","last_price":"","timestamp":1791531605419,"connector_receive_ts":1791531605962}"#
    );
}

/// 新增 Quote 不应改变既有 Tick / Kline 的序列化（对旧快照做严格字符串比较）
#[test]
fn tick_and_kline_serialization_unchanged_by_quote_addition() {
    let tick: Tick = serde_json::from_str(&load_fixture("tick_okx_btc-usdt-swap.json").unwrap()).unwrap();
    assert_eq!(
        serde_json::to_string(&tick).unwrap(),
        r#"{"exchange":"okx","symbol":"BTC-USDT-SWAP","timestamp":1777966262338,"price":"80992","quantity":"0.01","trade_id":2573412162,"exchange_event_ts":1777966262338,"connector_receive_ts":1777966262232}"#
    );
    let kline: Kline = serde_json::from_str(&load_fixture("kline_binance_btcusdt_1m.json").unwrap()).unwrap();
    assert_eq!(
        serde_json::to_string(&kline).unwrap(),
        r#"{"exchange":"binance","symbol":"BTCUSDT","interval":"1m","open_time":1777966260000,"open":"80987.40","high":"80987.40","low":"80987.30","close":"80987.30","volume":"0.630","close_time":1777966319999,"quote_asset_volume":"51022.00760","number_of_trades":34,"exchange_event_ts":1777966260000,"connector_receive_ts":1777966262471}"#
    );
}
