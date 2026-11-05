//! D2 诊断：量化"每个 topic 的 broadcast 环形缓冲在满载时常驻多少内存"（1.1.1 内存调查）。
//!
//! 用计数分配器精确统计：
//!   1. `broadcast::channel(cap)` 创建后的槽位数组大小（空环，创建即分配）；
//!   2. 环写满后，每个被保留的事件（`Arc<Tick|Kline|Quote>` 含全部 String 堆块）的字节数与分配次数。
//!
//! 事件样本取自真实抓取的 JSON（evidence/1.1.0/local-e2e/...），字符串长度与现网一致。
//!
//! 运行：`cargo run -p md-processor --example retention_sizing --release`

use md_domain::types::{Kline, Quote, Tick};
use md_processor::BroadcastEvent;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
/// 估算 musl malloc 的实际占用：请求大小 + 8 字节头，向上取整到 16
static LIVE_MUSL_EST: AtomicUsize = AtomicUsize::new(0);

fn musl_est(size: usize) -> usize {
    (size + 8).div_ceil(16) * 16
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Ordering::Relaxed);
        LIVE_MUSL_EST.fetch_add(musl_est(l.size()), Ordering::Relaxed);
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        LIVE_MUSL_EST.fetch_sub(musl_est(l.size()), Ordering::Relaxed);
        ALLOCS.fetch_sub(1, Ordering::Relaxed);
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        LIVE.fetch_add(new, Ordering::Relaxed);
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        LIVE_MUSL_EST.fetch_add(musl_est(new), Ordering::Relaxed);
        LIVE_MUSL_EST.fetch_sub(musl_est(l.size()), Ordering::Relaxed);
        System.realloc(p, l, new)
    }
}

#[global_allocator]
static A: Counting = Counting;

const KLINE_OKX: &str = r#"{"exchange":"okx","symbol":"BTC-USDT-SWAP","interval":"5m","open_time":1791533700000,"open":"82646.7","high":"82646.7","low":"82592.0","close":"82633.2","volume":"11112.66","close_time":1791533999999,"quote_asset_volume":"111.1266","exchange_event_ts":1791533700000,"connector_receive_ts":1791533917330}"#;
const KLINE_BINANCE: &str = r#"{"exchange":"binance","symbol":"BTCUSDT","interval":"5m","open_time":1791533700000,"open":"82646.70","high":"82646.70","low":"82592.00","close":"82633.20","volume":"111.12660","close_time":1791533999999,"quote_asset_volume":"9183512.3456","number_of_trades":1234,"exchange_event_ts":1791533700000,"connector_receive_ts":1791533917330}"#;
const TICK_OKX: &str = r#"{"exchange":"okx","symbol":"BTC-USDT-SWAP","timestamp":1791533917076,"price":"82633.1","quantity":"0.5","trade_id":2984603789,"is_buyer_maker":true,"exchange_event_ts":1791533917076,"connector_receive_ts":1791533917135}"#;
const TICK_BINANCE: &str = r#"{"exchange":"binance","symbol":"BTCUSDT","timestamp":1791533917076,"price":"82633.10","quantity":"0.00500","trade_id":2984603789,"is_buyer_maker":true,"exchange_event_ts":1791533917076,"connector_receive_ts":1791533917135}"#;
const QUOTE_OKX: &str = r#"{"exchange":"okx","symbol":"BNB-USDT-SWAP","bid_price":"743.8","bid_qty":"6920","ask_price":"743.9","ask_qty":"3184","last_price":"743.8","timestamp":1791533916971,"connector_receive_ts":1791533917127}"#;

fn measure<F: Fn() -> BroadcastEvent>(name: &str, cap: usize, make: F) {
    let base = LIVE.load(Ordering::SeqCst);
    let base_musl = LIVE_MUSL_EST.load(Ordering::SeqCst);
    let (tx, mut rx) = broadcast::channel::<BroadcastEvent>(cap);
    let ring = LIVE.load(Ordering::SeqCst) - base;
    let ring_musl = LIVE_MUSL_EST.load(Ordering::SeqCst) - base_musl;

    // 写满并溢出一圈（ring 稳态：恒保留最近 cap 个事件）
    let before_events = LIVE.load(Ordering::SeqCst);
    let before_events_musl = LIVE_MUSL_EST.load(Ordering::SeqCst);
    for _ in 0..cap * 2 {
        let _ = tx.send(make());
    }
    let after_events = LIVE.load(Ordering::SeqCst);
    let after_events_musl = LIVE_MUSL_EST.load(Ordering::SeqCst);
    let events = after_events - before_events;
    let events_musl = after_events_musl - before_events_musl;
    // 单个事件的堆足迹
    let one_before = LIVE.load(Ordering::SeqCst);
    let one_allocs_before = ALLOCS.load(Ordering::SeqCst);
    let e = make();
    let per_event = LIVE.load(Ordering::SeqCst) - one_before;
    let per_event_allocs = ALLOCS.load(Ordering::SeqCst) - one_allocs_before;
    drop(e);
    println!(
        "{name:<14} cap={cap}: 空环槽位数组 {:>7.1} KiB | 每事件 {:>4} B / {} 次分配 | 满环事件总计 {:>6.2} MiB (musl 估算 {:>6.2} MiB) | 满载合计 {:>6.2} MiB (musl 估算 {:>6.2} MiB)",
        ring as f64 / 1024.0,
        per_event,
        per_event_allocs,
        events as f64 / 1048576.0,
        events_musl as f64 / 1048576.0,
        (ring + events) as f64 / 1048576.0,
        (ring_musl + events_musl) as f64 / 1048576.0,
    );
    let _ = rx.try_recv();
    drop(tx);
}

fn main() {
    let cap: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4096);
    println!(
        "BroadcastEvent 大小 = {} B",
        std::mem::size_of::<BroadcastEvent>()
    );
    let k_okx: Kline = serde_json::from_str(KLINE_OKX).unwrap();
    let k_bn: Kline = serde_json::from_str(KLINE_BINANCE).unwrap();
    let t_okx: Tick = serde_json::from_str(TICK_OKX).unwrap();
    let t_bn: Tick = serde_json::from_str(TICK_BINANCE).unwrap();
    let q_okx: Quote = serde_json::from_str(QUOTE_OKX).unwrap();
    let now = std::time::Instant::now;
    // 每次 make 都克隆出独立的堆字符串（真实场景每条事件都是新解析出来的）
    measure("kline(okx)", cap, || {
        BroadcastEvent::Kline(Arc::new(k_okx.clone()), now())
    });
    measure("kline(binance)", cap, || {
        BroadcastEvent::Kline(Arc::new(k_bn.clone()), now())
    });
    measure("tick(okx)", cap, || {
        BroadcastEvent::Tick(Arc::new(t_okx.clone()), now())
    });
    measure("tick(binance)", cap, || {
        BroadcastEvent::Tick(Arc::new(t_bn.clone()), now())
    });
    measure("quote(okx)", cap, || {
        BroadcastEvent::Quote(Arc::new(q_okx.clone()), now())
    });
}
