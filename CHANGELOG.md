# 更新日志（CHANGELOG）

本文件记录 `market-data-server` Rust 版的重要变更。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [1.1.0] - 2026-10-09

新增 **quote（买一卖一报价）** 独立主题。遵循"只增不改"：开关默认关闭，不改配置升级时
行为与 1.0.0 完全一致；tick/kline、既有 REST/gRPC、旧 `/ws` 的输出不变。

### 新增（Features）

- **`quote.<exchange>.<SYMBOL>` 主题**：WS Gateway（`/ws/v1/data`）订阅/退订，推送
  `{"type":"quote","topic":"quote.binance.BNBUSDT","data":{exchange,symbol,bid_price,bid_qty,ask_price,ask_qty,last_price,timestamp,connector_receive_ts}}`。
  数值为字符串、时间戳为毫秒整数；币安 bookTicker 无成交价，`last_price` 为空字符串（字段仍输出，Quote 不使用 omitempty）；OKX 取 `tickers.last`。
- **数据源**：币安 U 本位 `<symbol>@bookTicker`（Public 类流，独立连接 `wss://fstream.binance.com/public/stream`）；
  OKX 公共频道 `tickers`（独立连接）。quote 走**独立连接器**（`binance-quote` / `okx-quote`），既有 tick/kline 连接不受影响。
- **REST**：`GET /api/v1/data/latest/quote/{exchange}/{symbol}`；`POST/DELETE /api/v1/subscriptions` 的 `type` 接受 `QUOTE`，
  `GET /api/v1/subscriptions` 以 `data_type: 3` 列出（`TICK=1`、`KLINE=2` 不变）。
- **限频**：每个 quote 主题对外 ≤10 条/秒（相邻 ≥100ms，窗口内合并并保证最后一个值被推出）。
- **配置（全部可选，旧配置原样可用）**：`connectors.{binance,okx}.quotes_enabled`（默认 `false`）、
  `connectors.{binance,okx}.subscribe_quotes`（默认空，SIGHUP 热更）、`processor.quote_channel_buffer`（默认 1000）、
  `connectors.binance.stream_base_url_public`（默认 `wss://fstream.binance.com/public/stream`）。
- **监控（只新增，旧指标名/标签不变）**：`md_quotes_processed`、`md_quotes_dropped`；
  `md_ingestion_latency_ms{type="quote"}`、`md_ws_messages_sent_total{kind="quote"}`、`md_broadcast_lagged_total{topic_kind="quote"}`
  为既有指标的新增标签取值；quote 连接器以 `exchange="binance-quote"|"okx-quote"` 暴露 `md_connector_*`。
- **gRPC**：**无新增 RPC**；`admin.proto` 的 `DataType` 仅追加 `QUOTE = 3`（既有取值与字段号不变）。
- **兼容性对比工具**：`scripts/compat/capture_ws.mjs`（抓 WS 推送原文）、`scripts/compat/compare.mjs`
  （push 逐字节对比 / subs / metrics 只增不减 / noquote / selftest 正对照）。
- **部署/验收辅助脚本**：`scripts/compat/analyze_quote.mjs`（quote 间隔、节流、非空、字段顺序，带正对照）、
  `scripts/compat/sample_process.sh`（进程指标采样）、`scripts/compat/restore_subs.mjs`（升级重启后补回丢失的动态订阅）。

### 兼容性说明

- 旧 `/ws`（Legacy）**永不推送 quote**；订阅 `quote.*` 会被明确拒绝（`quote topics are not supported on legacy /ws, use /ws/v1/data: …`）。
- 开启 quote 后 `GET /api/v1/subscriptions` 会**多出** `data_type: 3` 的条目（消费者若对未知 `data_type` 严格校验需留意）。
- `POST/DELETE /api/v1/subscriptions` 对非法 `type` 的错误文案仍是 `type must be TICK or KLINE`（未改动，以保证逐字节兼容）。
- 通过 REST/gRPC 动态增加的订阅（含 QUOTE）不写回配置，重启后丢失，以 `config.yaml` 为准。
- SIGHUP：`subscribe_quotes` 可热更（前提是 quote 连接器已在运行）；`quotes_enabled`、`stream_base_url_public`、`quote_channel_buffer` 变更需重启。

### 修复 / 测试（Fixes & Tests）

- **`md-tests` 不再静默跳过**：此前快照缺失或为 0 字节时测试直接 `return`（显示 passed 实则没执行），
  且 `scripts/capture-snapshots.sh` 因 `curl -f` 会在 4xx 时丢弃响应体、留下空的 `error_*.json`。
  现在快照缺失/为空会让测试失败，脚本改为"校验后才落盘"，失败即非 0 退出；补齐 4 个空快照并新增对
  `subscriptions.json`、`error_bad_request.json` 等此前无断言快照的检查。
  （注：新补的 `error_*` / `ws_gateway_*` 快照取自 NAS 测试服务器上的 Rust 1.0.0 基线，而非 Go 版；
  `tick_*` / `kline_*` / `subscriptions` 仍是最初从 Go 版抓取的快照。）
- 新增与 1.0.0 的逐字节回放测试（`crates/md-gateway/tests/compat_1_0_0.rs`），用 NAS 上 1.0.0 的 60 秒真实推送
  （193 行 Gateway + 60 行 Legacy，共 253 行）回放，要求新版输出逐字节相同。

### 基线说明（收编）

NAS 测试服务器（192.168.1.198）上 1.0.0 实际运行的二进制，包含若干**从未提交到 git** 的改动。
1.1.0 将它们以单独一次提交收编（内容未改），因此 1.1.0 的"旧行为"基线 = 198 现网行为，而不是字面的 v1.0.0 tag：

- Binance aggTrade 的 `timestamp` / `exchange_event_ts` 取事件时间 `E`（原为成交时间 `T`）；
- 入库延迟直方图桶加密（`BUCKETS_MS`），新增 `md_ingestion_latency_e2e_ms`；
- `Tick.exchange_trade_ts`（serde skip，不进线格式）；
- TCP keepalive retries 仅在 Linux 设置（macOS 可编译）。

## [1.0.0] - 2026-06-06

首个**可用于生产环境长期在线**的版本。在 1:1 重写 Go 版功能的基础上，完成了
断点重连、可观测性、资源治理等多项加固，并补齐了与 Go 版等价的监控体系与部署能力。

### 新增（Features）

- **多维入库延迟指标**：`md_ingestion_latency_ms` 按 `{exchange, type, symbol, interval}`
  分维度统计，可在 Grafana 任意聚合（交易所 / 单标的 / 周期），完全对齐 Go 版 dashboard。
- **网关内部延迟指标**：`md_gateway_internal_latency_ms{topic}` 统计 publish→发送给客户端
  的耗时，支持按 topic 的推送吞吐 TOP10。
- **WebSocket 连接治理指标**：`md_ws_active_clients`（活跃连接数）、
  `md_ws_kicked_lagged_total`（慢客户端踢出数）、`md_ws_messages_sent_total{kind}`。
- **标准进程指标**：零依赖读取 `/proc` 输出 `process_resident_memory_bytes`、
  `process_cpu_seconds_total`、`process_open_fds`、`process_max_fds`、
  `process_start_time_seconds`、`process_uptime_seconds`。
- **Grafana dashboard 模板** `dashboards/md-server-rust.json`，与 Go 版面板一一对应。
- **慢客户端自动踢出**：连续 3 次 broadcast Lagged 即主动断开，保护整体推送链路。
- **失败 stream 黑名单**：订阅持续失败的 stream 进入黑名单并在重连时跳过，到期自动恢复。
- **可选 `vendored-tls` feature**（`md-connector`，默认关闭）：交叉编译时让 OpenSSL
  随源码静态编译，支持在 macOS 上产出 x86_64 Linux 全静态二进制。
- **`Dockerfile.cn`**：国内/内网镜像加速构建（基础镜像 daocloud、apt 阿里云、crates rsproxy）。

### 修复（Fixes）

- **TCP Keepalive 对 TLS 连接失效**：原实现仅对明文连接生效，现对 `wss://`（NativeTls）
  与明文连接均正确设置 keepalive，使用 `socket2::SockRef` 安全借用，避免 fd 所有权问题。
- **时间戳取值可能 panic**：Binance/OKX 解析中的 `SystemTime` 取值改为 `unwrap_or(0)`，
  规避系统时钟早于 UNIX 纪元时的崩溃。
- **`current_subscriptions` 读锁竞争丢数据**：trait 方法改为 `async`，由 `try_read` 改为
  `read().await`，消除锁竞争下返回空列表的问题。
- **关停时序竞争**：连接器任务改用 `JoinSet` 统一等待，确保在 processor 关停前结束写入。

### 性能（Performance）

- **release 编译优化**：启用 `lto`、`codegen-units=1`、`strip`、`panic=abort`，减小体积、提升运行时性能。
- **订阅分批**：大批量订阅按 `MAX_STREAMS_PER_MSG` 分片并间隔发送，避免单帧过大被交易所拒绝。
- **重连抖动（jitter）**：在重连退避基础上叠加随机抖动，避免大量连接同时重连造成雪崩。

### 重构（Refactor）

- 移除未使用的 `graceful_shutdown_timeout` 死代码。
- `BroadcastEvent` 携带 `Instant` 发布时刻，支撑端到端延迟测量。
- `Histogram` 通用化，支持自定义桶边界。

### 文档（Docs）

- `README.md`：补充交叉编译静态 Linux 二进制、Docker（国内镜像）构建说明。
- `DEPLOYMENT.md`：新增 §2.1.1 交叉编译、§4.5 `Dockerfile.cn`、§4.6 Portainer API
  远程一键替换部署流程；§7 监控章节更新为多维指标与对应告警规则。

### 部署记录（Ops）

- 已通过 Portainer API 在内网 Debian 12（x86_64）主机上原生构建并替换 `md-server`
  容器，验证 `/health`、`/metrics`（多维指标生效）、Binance/OKX 实时数据流入、
  gRPC 50051 / 网关 8081 端口均正常，丢弃数为 0。
