# Phase 2 & 3 TDD 完成记录

## Phase 2：数据通路 + 服务层 ✅

### 步骤 6: md-processor（Cache + PubSub + Metrics）✅

- DashMap 缓存：`latest_ticks` / `latest_klines`
- Cache key: Tick `"exchange:SYMBOL"`, Kline `"exchange:SYMBOL:interval"`
- tokio::sync::broadcast PubSub，topic 格式：`tick.{exchange}.{SYMBOL}` / `kline.{interval}.{exchange}.{SYMBOL}`
- ProcessorMetrics：AtomicU64 计数器
- `handle_tick()` / `handle_kline()`：缓存 + 发布
- `subscribe(topic)` / `get_latest_tick()` / `get_latest_kline()`
- 12 tests pass

### 步骤 7: md-grpc（MarketDataService + AdminService）✅

- `MarketDataServiceImpl`：subscribe_ticks / subscribe_klines（server streaming），get_latest_tick / kline
- `AdminServiceImpl`：add / remove / get_subscriptions
- 类型转换：`tick_to_proto` / `kline_to_proto` / `data_type_to_proto`
- 19 tests pass

### 步骤 8: md-gateway（REST + WebSocket）✅

- REST：`GET/POST/DELETE /api/v1/subscriptions`，`GET /api/v1/data/latest/tick/:exchange/:symbol`，`GET /api/v1/data/latest/kline/:exchange/:symbol/:interval`
- WebSocket Gateway (`/ws/v1/data`)：`{"action":"subscribe","streams":[...]}` → `{"type":"tick","topic":"...","data":{...}}`
- WebSocket Legacy (`/ws`)：`{"op":"subscribe","args":[...]}` → `{"topic":"...","data":{...}}`
- 数据推送：`select_all` + `Box::pin` 从 broadcast 接收并转发
- 9 tests pass

### 步骤 9: md-server 集成 ✅

- Processor 替代直接 mpsc channel
- gRPC: MarketDataServiceServer + AdminServiceServer
- API Gateway: REST + WebSocket via axum
- Connectors 注册表共享

---

## Phase 3：收尾 + 运维 ✅

### 步骤 10: OKX 连接器 ✅

- `OkxAdapter`：实现 `ExchangeAdapter` trait
- trades → Tick 解析，candle → Kline 解析
- `target_to_streams`：TICK → `trades:{SYMBOL}`，KLINE → `candle{interval}:{SYMBOL}`
- `build_subscribe_msg`：`{"op":"subscribe","args":[{"channel":"trades","instId":"BTC-USDT-SWAP"}]}`
- heartbeat: `"ping"` 消息
- 13 tests pass

### 步骤 11: 完善 md-server ✅

- 同时启动 Binance + OKX 连接器
- SIGTERM / SIGINT 优雅关停（`tokio::signal::unix`）
- SIGHUP 热重载配置（重新加载 config.yaml）
- `axum::serve::with_graceful_shutdown` 支持

### 步骤 12: 运维端点 ✅

- `/health`：健康检查，返回 `{"status":"ok","service":"md-server"}`
- `/metrics`：Prometheus 格式指标输出
  - `md_ticks_processed` / `md_klines_processed`
  - `md_ticks_dropped` / `md_klines_dropped`

### 步骤 13: 兼容性验证框架 ✅

- `scripts/capture-snapshots.sh`：从 Go 版抓取 JSON 快照
- `crates/md-tests/tests/json_compat.rs`：4 个兼容性测试
  - tick / kline JSON roundtrip
  - WebSocket gateway 格式验证
  - 错误响应格式验证
- 快照不存在时自动跳过（不阻塞 CI）

---

## 最终测试统计

```
md-domain     23 tests ✅
md-config      7 tests ✅
md-connector  26 tests ✅ (13 Binance + 13 OKX)
md-processor  12 tests ✅
md-proto       9 tests ✅
md-grpc       19 tests ✅
md-gateway     9 tests ✅
md-tests       4 tests ✅ (compatibility, skip if no fixtures)
────────────────────────
总计         109 tests ✅
```

每个步骤严格遵循：RED -> GREEN -> REFACTOR
