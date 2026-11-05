# compat-final-v5 第 1 次同窗口对比：push-gateway 1 项 FAIL（未删除，如实保留）

- 抓取时间 2026-10-10T17:10Z，old = `md-server:1.1.0-quote-bnb`，new = `md-server:exp-v5`（候选 A：A1+A2+A5+jemalloc）。
- 其余项：push-legacy PASS、noquote-legacy PASS、subs PASS（39/39）、metrics PASS（只新增 11 条、无删除/改类型）、selftest 25/25。
- **唯一 FAIL**：`quote.binance.BNBUSDT` 逐字节 —— 1204 条对应记录中 **2 条**不同。tick（6 个主题）与 kline（7 个主题）全部逐字节一致；`quote.okx.BNB-USDT-SWAP` 1250 条逐字节一致。

## 原因分析（原始数据：`old-gateway/quote.binance.BNBUSDT.raw.jsonl` / `new-gateway/…`）

`compare.mjs` 的 quote 对应键是 `exchange|symbol|timestamp(E)|bid_price|ask_price`，不含数量。两条独立的币安 WS 连接
收到的 bookTicker 事件序列本身就不完全相同，再叠加 1.1.0 起就有的 quote 节流（100 ms 窗口、尾沿只保留最新，
`md-processor` `QuoteThrottleState`，1.1.1 未改动，`git diff beaac94 -- crates/md-processor/src/lib.rs` 无 quote/throttle 行），
两侧会选中不同的事件：

| 键 (E, bid, ask) | old（1.1.0）收到 | new（1.1.1）收到 |
|---|---|---|
| 1791652333032, 750.020, 750.030 | E=…032 qty 40.21/4.80（recv …098）；E=…032 qty 40.88/4.80（recv …203） | E=…032 qty 40.21/4.80；随后是 E=…106 qty 40.88/**4.73**（old 侧不存在这条） |
| 1791652334975, 750.030, 750.040 | bid_qty 6.19 / ask 0.08 | bid_qty 3.98 / ask 0.08 |

old 侧自己就有 4 个"同键不同载荷"的组（new 侧 3 个）：同一 E 毫秒内、价格不变而数量变化的多条 bookTicker，
本来就不能靠 (E, bid, ask) 唯一配对。**值的差异来自上游事件序列/节流相位，不是格式、字段顺序或外层结构变化**
（`字段名/顺序`、`外层结构` 两项在该主题上均为 OK）。

## 处理

- **不改 compare.mjs 来让它通过**（改验收工具要由主审决定）；
- 用**同一个、未改动的工具**重跑同窗口对比：`../compat-final-v5/`；
- 另做"对照"：两个 **1.1.0** 容器（不含任何 1.1.1 代码）之间用同一工具比较：`../compat-control-110-vs-110/`，
  用来说明这个 quote 逐字节项在两个独立连接之间本身就可能出现 FAIL（与 1.1.1 无关）。
