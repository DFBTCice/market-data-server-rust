/// 兼容性测试：验证 Rust 序列化输出与 Go 版 JSON 快照一致。
///
/// 运行方式：
///   cargo test --test json_compat
///
/// 前置条件：
///   1. 运行 scripts/capture-snapshots.sh 抓取 Go 版快照
///   2. 快照文件在 tests/compatibility/fixtures/ 下
use md_domain::types::{Kline, Tick};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn fixtures_dir() -> &'static Path {
    Path::new("tests/fixtures")
}

/// 加载快照文件，如果不存在则跳过测试
fn load_fixture(name: &str) -> Option<String> {
    let path = fixtures_dir().join(name);
    if path.exists() {
        let content = fs::read_to_string(&path).unwrap();
        if content.trim().is_empty() || content.contains("跳过") {
            None
        } else {
            Some(content)
        }
    } else {
        None
    }
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
