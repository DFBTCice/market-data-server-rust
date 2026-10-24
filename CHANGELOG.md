# 更新日志（CHANGELOG）

本文件记录 `market-data-server` Rust 版的重要变更。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [1.1.1] - 2026-10-24（NAS 已换版，24h 观察中；未打标签）

本版同时包含 A1–A5、jemalloc 分配器和 SIGHUP/log_level 修复，不另发 1.1.2。
最终源码 `a06fe20`，干净 worktree 固定构建时间、两次从零构建 SHA-256 一致：
`3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d`。
构建条件见 `evidence/1.1.1/build-logs/dist-sha256.txt`；复验状态见 `evidence/1.1.1/final-artifact/REPORT.md`。

### SIGHUP 配置重载与日志级别

调查、复现与验证见 `evidence/sighup-reload-fix/REPORT.md`。**本节修复只涉及 SIGHUP 重载逻辑与日志级别**：行情数据路径（tick/kline/quote 输出）、
REST/gRPC/WS、`/metrics`、配置文件格式均不变。仍然是"地址变更需要重启"的设计，**没有**引入连接器热重建。
修复前的 1.1.0（SHA-256 `d73f7a4c…ba60`）与早期 1.1.1 候选均已复现下列缺陷；最终 1.1.1 已修复。

#### 修复（Fixes）

- **漏检 OKX Business 地址**：SIGHUP 检查了 `okx.stream_base_url_public`，却漏了 `okx.stream_base_url_business`。
  只改 Business 地址时日志是 `config reloaded and applied successfully`，而连接器仍连旧地址；
  在线上同款 `RUST_LOG=warn` 下则是**完全没有任何输出**。
- **把没生效的配置记成"当前配置"**：重载结束无条件 `current_cfg = new_cfg`。改了 URL 并提示需重启后，第二次 SIGHUP 比较的是"自己和自己"，于是报成功，
  连接器却仍用旧地址。现在区分"文件中的新配置"和"已应用配置"：只有真正热更成功的订阅列表 / 日志级别才进入已应用配置；
  需重启的变更每次 SIGHUP 都继续点名，改回原值才消失。**Binance**（`binance.stream_base_url`、`binance.stream_base_url_public`）有同样缺陷，一并修复。
- **自动重连不会应用新地址**（设计如此，现在文档与日志写明）：连接器只持有启动时的地址，断线重连不会重新读 YAML；改地址必须重启。重启后连接器连到新地址（已用端到端测试验证）。
- **`log_level` 从未生效**：YAML 的 `log_level` 只被读出来打印一行，日志过滤器始终取自环境变量 `RUST_LOG`（没设就是 `info`）；
  `main.rs` 里"可通过 RUST_LOG 或配置覆盖"的注释从未实现。1.1.0 / 早期1.1.1候选上改 `log_level`、再发 SIGHUP，日志照旧，SIGHUP 还会写 `log_level: … (applied)` 谎称已应用。
  现在真正生效：**启动时按它初始化过滤器，SIGHUP 可热更（不重连、不重启）**。
  - 优先级：环境变量 `RUST_LOG`（已设置、非空、写法合法）＞ YAML `log_level`（环境变量 `LOG_LEVEL` 可覆盖它）＞ 默认 `info`。
    取"环境变量优先于配置文件"的通行约定：现网都设了 `RUST_LOG=warn`，这样升级后日志量不会被悄悄改变。
  - 两者同时存在且不一致时不装作生效：启动时 WARN 点名两者；SIGHUP 改 `log_level` 时每次提示 `NOT applied … RUST_LOG=… takes precedence`，直到改回或重启。
  - 写成非法值（`fatal`、`verbose`、拼写错误）以前被悄悄忽略，现在启动时报错退出、SIGHUP 时拒绝并保持运行配置；合法值为 `trace` / `debug` / `info` / `warn`（也接受 `warning`）/ `error`，不区分大小写。
  - 不合法的 `RUST_LOG` 以前悄悄退回 `info`，现在退回 YAML 的 `log_level` 并 WARN。
- **同类问题（调查中一并发现并修复）**
  - 带 `--port-offset` 或环境变量覆盖（如 `API_GATEWAY_LISTEN_ADDRESS`）时，每次 SIGHUP 都误报监听地址变更；现在启动与重载共用同一个"生效配置"推导。
  - 订阅列表变更但对应连接器没在运行（OKX tick / K 线：交易所未启用或列表启动时为空）被静默吞掉并报成功；quote 只警告一次。现在每次都点名提示、不报成功、不推进已应用配置。
  - 订阅下发失败（连接器返回错误）也报成功；现在不报成功，保持旧列表，下一次 SIGHUP 重试。
  - `reconnect_delay`、`ping_interval`、`rest_base_url`、`processor.*`、`api_gateway.*` 等只在启动时读取的字段，SIGHUP 以前完全不检测；现在用结构体穷尽解构统一覆盖，**新增配置项不分类就编译不过**。
  - 配置读取 / 解析 / 校验失败从 WARN 提升为 ERROR，并明确"运行配置保持不变（nothing applied）"。

#### 行为变化（只涉及 SIGHUP 与日志级别，运维需知）

- 以前报"成功"的若干情形现在会报"需要重启"——这是修复的目的（见上）。
- **198 换版保留容器 `RUST_LOG=warn`**，阶段 2 配置仅将 YAML `log_level: "info"` 改为 `"warn"`。实际级别仍为 warn，升级后日志量不变，且不会新增覆盖告警；本次不去掉环境变量。
- 保留的日志串：`config reloaded and applied successfully`、`some config changes require connector restart -- please restart the service`（末尾追加 `(N pending: 路径列表)`）、`failed to reload config`、订阅热更的 `adding/removing N ... subscriptions` 系列；日志 target 仍为 `md_server`。
- 变更的日志文案（日志没有外部消费方，一切走 API）：地址 / 开关等需重启项改为 `<YAML 路径> changed: <旧值> -> <文件值> (NOT applied, requires restart)`；
  启动时的 `log_level: <x>` 行改为 `log filter: <x> (from …)`，`loading config from` 改为 `loaded config from`；
  `log_level` 变更成功时输出 `log_level changed: A -> B (applied)`（这次是真的生效了）。
  如有依赖旧文案的告警 / 脚本，请对照 `OPS_GUIDE.md` 第一节更新。
- 启动顺序变化：先读配置、再初始化日志（过滤器取决于 `log_level`），因此读配置失败的报错只写 stderr（以前这一行本来就是 `eprintln!`）。
- 订阅列表热更的行为不变（对运行中的连接器增量 add/remove，不重连）；同一批次内下发顺序改为按符号排序（稳定、可测试）。

#### 测试

- 单元测试：`md-server` 5 → 42，`md-config` 10 → 14，整个工作区 248 个全部通过。
  - SIGHUP：逐字段检测（每个"只在启动时读取"的字段单独改动都必须被检出）、任一地址连续多次 SIGHUP 不误报、改回原值提示消失、
    订阅热更与待重启并存、连接器缺失 / 下发失败、读取失败保持运行配置、端口偏移推导一致，以及"重启后每个连接器连到配置里对应字段的地址"（进程内假上游）。
  - `log_level`：优先级（含 198 的真实组合）、用真实 subscriber + 捕获层证明热切换确实改变哪些事件通过、`RUST_LOG` 绝不被热更改动、
    切换失败保持旧级别并重试、写法规范化（`INFO` / `warning`）不产生假变更、非法值被拒绝、与待重启地址 / 订阅热更并存。
- 端到端脚本 `scripts/compat/sighup_reload_e2e.mjs`（+ `fake_upstream.mjs`，零依赖）：真实进程、真实 SIGHUP、假上游；19 个场景、129 条断言，
  支持原生进程与 Docker（线上同款静态二进制）两种运行方式。修复前（HEAD `eb78524` 原生进程、线上 1.1.0 二进制 Docker）都是 70/129 通过，失败的 59 条完全相同；
  修复后 129/129（原生进程、Docker 静态二进制两种方式均是）。
- 变异验证（`evidence/sighup-reload-fix/verify/run_mutations.py`）：21 种故意改坏（地址检测 / 基线污染 / 订阅与日志级别的各种"假装已应用" / `RUST_LOG` 优先级 / 校验 / 启动接线），全部被抓住；
  其中 2 种启动接线类变异单元测试抓不住（全局 subscriber 只能装一次），由端到端抓住。

### Gateway 修复与内存调查

**内存增长调查 + Gateway 退订泄漏修复**，作为阶段 3（扩展品种）的前置条件。完整调查见
`evidence/1.1.1/INVESTIGATION_REPORT.md`；换版计划见 `evidence/1.1.1/nas-198/SWAP_PLAN_1.1.1.md`。
遵循"只增不改"：tick/kline/quote 的语义与逐字节输出、REST/gRPC、旧 `/ws`（Legacy）均不变；
新增配置项无（沿用已有的 `api_gateway.ws_write_wait`），新增监控只增不减。

#### 修复（Fixes）

- **Gateway（`/ws/v1/data`）退订不释放接收端（A1）**：此前 `unsubscribe` 只把主题从订阅集合里删掉，
  对应的 broadcast 接收端仍留在连接的接收列表里并继续被轮询。后果：同一连接对同一主题"订阅→退订→再订阅" N 次后，
  该主题的每条事件被**重复推送 N 份**，接收端也不会释放（每个泄漏的接收端约 3~4 KiB，且持续消耗 CPU：旁路对照 6 小时 CPU 约为修复后的 2.5 倍）。
  现网 1.1.0 已确认存在重复推送（`evidence/1.1.1/probe/prod-1.1.0-before.json`）。
  修复：每个连接用 `StreamMap<主题, 流>` 持有接收端，退订即移除；同主题重复订阅为幂等。
- **接收路径等价改写（A2）**：新的接收路径对外输出的字节与 1.1.0 完全相同：`compat_1_0_0` 回放、`md-tests` 通过；
  `compare.mjs` 同窗口对比中 tick/kline（Gateway 与 Legacy）逐字节一致，订阅集合、metrics（只新增不删除）一致。
  **主审已裁定不计失败的工具噪声**：`quote.binance.BNBUSDT` 的逐字节项在两条独立连接间有噪声（仅 `bid_qty`/`ask_qty`，0.1%~0.6% 条数，1.1.0 对 1.1.0 同样出现），见 `evidence/1.1.1/report/quote-byte-noise.txt` 与调查报告 §12.9。Legacy `/ws` 不改动。
- **Gateway 写超时（A5，数据连接的行为变化）**：`api_gateway.ws_write_wait`（默认 `10s`）在 1.1.0 及以前被解析但**从未生效**。
  客户端停止读取（进程挂起 / 半开连接 / 严重卡顿）时，服务端对该连接的 `send` 会一直阻塞，连接处理协程不再轮询接收端，
  broadcast 环（每主题 4096 条）里的事件被一直持有。1.1.1 起写入超过 `ws_write_wait` 即断开该连接（消费者需自行重连并重新订阅）。
  设为 `"0s"` 可恢复旧行为。新增指标 `md_ws_kicked_write_timeout_total`。
- **分配器（见下"调查结论"）**：特性 `--features md-server/jemalloc-stats` 使用 jemalloc 作为全局分配器。**本次现网镜像必须启用此特性**；未启用的默认构建仍用 musl，内存斜率不达标。

#### 新增（Features，均为只增）

- **`md_ws_subscriptions_active`**（gauge）：所有 WS 连接当前持有的 broadcast 接收端总数（RAII 计数，连接断开自动扣减）。
  用来在现网直接发现"订阅数只增不减"。
- **`md_ws_kicked_write_timeout_total`**、**`md_tokio_alive_tasks` / `md_tokio_workers` / `md_tokio_global_queue_depth`**；
  `--features md-server/jemalloc-stats` 构建额外暴露 `md_jemalloc_{allocated,active,resident,retained,mapped,metadata}_bytes`。
- **订阅/退订 debug 日志**（`RUST_LOG=md_gateway=debug`）：订阅、退订、当前接收端数。
- **丢弃告警限频**：connector 的 tick/kline/quote 入口队列满时，同一连接器同一类数据每秒最多一条 WARN；
  被抑制的丢弃条数累计到下一条日志里（日志里的条数之和 == 实际丢弃总数），并带"入口队列深度 / 累计丢弃数"
  （此前每丢一条打一条 WARN：1.0.0 对照容器 6 小时产生约 1.6 千行）。
- **仅调试用环境变量**：`MD_DEBUG_FORCE_RECONNECT_SECS` / `MD_DEBUG_FORCE_RECONNECT_ONLY`（强制周期性重连，默认不设即无任何影响）。
- **调查/验证工具**：`scripts/compat/{load_consumers,stall_client,sample_metrics,md_ws_dup_probe}.mjs`、
  `crates/md-processor/examples/retention_sizing.rs`、`evidence/1.1.1/tools/`。

#### 测试（A4，均带"在修复前代码上必须失败"的正对照）

- `crates/md-gateway/tests/unsubscribe_gateway.rs`（6 个）：退订后无事件、重订恰好 1 份、反复抖动仍 1 份、重复订阅无额外副本、
  部分退订不影响其它主题且字节不变、一个连接退订不影响另一个连接。修复前代码上 3 个失败（`evidence/1.1.1/a4-tests/`）。
- `crates/md-gateway/tests/ws_subscriptions_gauge.rs`：gauge 随订阅/退订/断开增减。
- `crates/md-gateway/tests/write_timeout_gateway.rs`（3 个）：停读客户端被踢（对照：`ws_write_wait=None` 时连接被永远钉住）、健康客户端不受影响。
  去掉超时实现的变异版上"被踢"用例失败（`evidence/1.1.1/a5-write-timeout/`）。

#### 调查结论（内存增长，详见调查报告）

- **不是逻辑泄漏**：同样的代码换 jemalloc，存活堆（`allocated`）在 7 小时、含 30 分钟停读与行情突发的负载下稳定在 6~10 MiB；
  musl 自带分配器的 14 个对照容器全部上涨（起止 +4 ~ +138 MiB）；jemalloc 的 4 个容器预热 1 小时后斜率为 +0.08 ~ +0.85 MiB/h（均 ≤ 1 MiB/h；起止 +0.2 ~ +8 MiB，停读造成的峰值最高 56.8 MiB，之后回落）。
  上涨来自 musl 分配器的**高水位（只升不降）**，触发因素包括消费者卡顿（环里滞留事件）和行情突发，且无消费者时也会继续缓慢上涨。
- **A1 泄漏真实但很小**（约 3.4 KiB/个接收端），不是内存增长主因，但会造成重复推送与 CPU 浪费。
- **固定滞留有上界**：广播环容量 4096，满环约 1.8~2.4 MiB/主题（现网 39 主题合计上界约 88 MiB，需消费者长时间停读才会出现）；A5 把停读造成的滞留限制在 `ws_write_wait`（10 秒）内。
- 单靠 A5（默认分配器）在旁路等价负载下**仍不满足** RSS 斜率 ≤ 1 MiB/h；**A5 + jemalloc** 满足（详见报告 §D3 与换版计划的候选选择）。
- 现网 `md_ticks_dropped` 的来源：行情突发时 tick 入口队列（1000）在毫秒级被打满；同一时刻现网与全部旁路容器同时出现丢弃，证明来自上游行情而非本进程故障。

#### 文档更正

- 1.0.0 发布说明里的"失败 stream 黑名单"、`MAX_STREAMS_PER_MSG` 订阅分批、重连抖动（jitter）三项在代码中并不存在，已在 1.0.0 条目中划线更正。

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
- ~~**失败 stream 黑名单**：订阅持续失败的 stream 进入黑名单并在重连时跳过，到期自动恢复。~~
  **【1.1.1 更正】代码中不存在黑名单机制。** 实际行为：订阅被交易所拒绝时，该 stream 只从 `active_streams`
  移除并累加 `md_connector_subscribe_failed_total`（见 `base.rs::dispatch_event` 的 `SubscribeError` 分支）；
  下次重连同步订阅时会再次尝试，不会跳过。1.0.0 发布说明里的这条描述有误。
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
- ~~**订阅分批**：大批量订阅按 `MAX_STREAMS_PER_MSG` 分片并间隔发送，避免单帧过大被交易所拒绝。~~
  **【1.1.1 更正】代码中不存在 `MAX_STREAMS_PER_MSG` 分片逻辑**（`sync_subscriptions_on_connect` 一次性发送全部待订阅 stream）。
- ~~**重连抖动（jitter）**：在重连退避基础上叠加随机抖动，避免大量连接同时重连造成雪崩。~~
  **【1.1.1 更正】代码中不存在抖动/退避**：`base.rs::run_connection_loop` 使用固定的 `reconnect_delay`（与 Go 版一致）。

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
