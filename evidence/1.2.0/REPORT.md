# 行情服务 1.2.0 交付报告：运行期订阅闲置退订与按需补订

- 日期：2026-11-06（北京时间）。对象：1stbotv3 主审任务书「闲置自动退订与按需补订」（业务方转发）。
- 分支：`feat/runtime-sub-idle-v1.2.0`（从 `origin/main` `d2f2cb6` 新建，已推送到 GitHub；未打标签、未合并、未建 PR）。
- 源码提交 `6b3949e`；之后的提交只有文档与证据，`git diff 6b3949e HEAD -- crates Cargo.toml Cargo.lock` 为空。证据索引：[`README.md`](README.md)。

## 首段

**做了哪几条**：第〇节核对完成，本地 / GitHub / 198 三方一致后才开发；第三节 1–7 条全部实现（开关默认关闭、只管运行期订阅、接收方判定、闲置退订、按需补订含三项保护、与 POST/DELETE/GET 的关系、指标、文档）；
第四节验收中，门禁、9 类集成场景、正对照、开关关闭同窗口逐字节对比、开关打开金丝雀都已完成并附原文，其中**同窗口对比与金丝雀是在本机连真实交易所做的**；
**198 旁路金丝雀未做**：本机没有有效的 Portainer 会话（`/tmp/pt_jwt` 已过期，HTTP 401），脚本已就绪；换版未做（须业务方批准）。

**门禁**：`cargo fmt --all --check` 无输出；`cargo clippy --workspace --all-targets` warning 13 条（与基线 13 条位置相同）；
`cargo test --workspace` 286 个全过（基线 248 个，新增 38 个）；`md-tests` 11 个、`compat_1_0_0` 5 个真实执行；正对照 13/13 被抓住。

**产物**：`dist/md-server-1.2.0-f1d3c8a-linux-x86_64`，SHA-256 `f6ab760cc2b62562b188104f10efaabf075723c6c8feae9dd2e14816c61e13ba`（`f1d3c8a` 干净 worktree、`SOURCE_DATE_EPOCH=1793974425`、
`--features md-connector/vendored-tls,md-server/jemalloc-stats`、两次从零构建 `cmp` 一致）。

**需要谁决定什么**：
1. 业务方：是否提供 Portainer 会话，让我跑 198 旁路金丝雀（第五节第 1 条，约 10 分钟，只建删 3 个带标签的旁路容器，现网只读）。
2. 业务方：198 金丝雀通过后，是否批准以**开关关闭**换版；打开开关是另一次配置变更，另行批准并提前通知主审。打开时是否同时把容器环境改成 `RUST_LOG=warn,md_runtime_subs=info`（否则退订 / 补订的 INFO 明细在现网看不到，只能看指标）。
3. 主审：确认第一节「设计取舍」里 6 处我自行定下的做法。

---

## 一、做了什么

### 第〇节：开工前核对（`preflight/`，命令 `bash evidence/1.2.0/tools/preflight.sh`）

| 项 | 结果 |
|---|---|
| 当前分支与 HEAD | `fix/gateway-unsubscribe-leak-v1.1.1` @ `56f14fd`（与主审所见一致） |
| `git fetch --all --tags` | 无新内容（本地缓存即最新） |
| origin/main | `d2f2cb6` |
| 标签 | `v1.0.0`→`07b9ef7`；`v1.1.0`→`beaac94`；`v1.1.1`→`a06fe20` |
| 本地相对 origin/main | 领先 0、落后 3（`5939dd0`、`71b3ebd`、`d2f2cb6`）；`56f14fd` 与 `a06fe20` 都是 origin/main 的祖先 |
| 已跟踪文件改动 | 无 |
| 未跟踪文件 | 1138 个（折叠后 58 项：`evidence/1.1.0/...`、`evidence/1.1.1/compat-*/...` 原始数据、`Dockerfile.bin`、`Dockerfile.prebuilt`、`monitoring/`），全部原样保留；切分支前后清单逐行一致 |
| origin/main 与 v1.1.1 源码差异 | `crates/`、`Cargo.toml`、`Cargo.lock`、`Dockerfile`、`config.yaml` 树哈希在 `a06fe20` / `v1.1.1` / `56f14fd` / `origin/main` 上完全相同（`crates` = `4c39382d…`）；文档 / 证据以外只有 `.gitattributes` 和两个 compat 脚本 |
| 198 跑的产物 | `process_start_time_seconds=1792785202`（2026-10-23T19:53:22Z，即 1.1.1 换版那一秒，之后未重启）；`/metrics` 输出 6 项 `md_jemalloc_*`（jemalloc 构建）；换版记录 `swap-result.json` 从该运行容器读回的二进制 SHA `3ffa1277…c79d` = `a06fe20` dirty=0 的最终产物 |

三方一致，从 `origin/main` 新建 `feat/runtime-sub-idle-v1.2.0`，并在新代码上复核了任务书第二节的事实（位置未变：`md-server/src/reload.rs`、`md-gateway/src/rest.rs` 的 `remove_subscription`、`md-connector/src/base.rs` 的 `desired_subs`）。
**限定**：本次未能从运行容器直接读回二进制 SHA（Portainer 会话过期），上面一行是"进程未重启 + 换版时读回的 SHA"的证据链。

### 第三节：实现（`6b3949e`）

| 任务书条目 | 实现 |
|---|---|
| 1 开关 | 新配置段 `runtime_subscriptions`（整段可选，`deny_unknown_fields`）：`idle_unsubscribe=false`、`idle_minutes=10`、`check_interval_seconds=60`、`startup_grace_minutes=10`、`max_runtime_subscriptions=200`；0 值校验。闲置退订与按需补订**同一个开关**。旧 `config.yaml` 原样可用（有测试锁定）。SIGHUP 可热更整段（见设计取舍 4） |
| 2 只处理运行期订阅 | 运行期 = 各连接器 `desired_subs` 中不在配置名单里的目标。配置名单按规范键（交易所小写、币对大写、级别原样）比较，所以配置 `BTCUSDT` 与 POST 的 `btcusdt` 视为同一目标、按配置处理；SIGHUP 部分失败时保护"已应用 ∪ 文件"两份名单 |
| 3 接收方判定 | 规范话题上的接收端数（`Processor::receiver_count`，Gateway WS / 旧 `/ws` / gRPC 都经 `Processor::subscribe`）；REST `GET /api/v1/data/latest/{tick,kline,quote}/…` 记一次使用（路径大小写不敏感）；新接收端登记时刻也算使用 |
| 4 闲置退订 | 每 `check_interval` 检查；闲置起点取"首次观察到无接收端"的检查时刻（实际闲置只会 ≥ `idle`）；保护期从开关打开时刻算；INFO 日志 `runtime subscription idle-unsubscribed: exchange=… type=… symbol=… interval=… idle=… connector=…` |
| 5 按需补订 | 新接收端登记后约 200ms 合批补订，每次检查再兜底一次；INFO `runtime subscription added on demand: …`。保护：①只接受已配置的交易所和类型（与 POST 同一路由，quote 需 quote 连接器在运行）；②上游明确拒绝（OKX 60018/60012）撤回、缓存、指数退避 5 分钟→1 小时；③达到上限不补订、WARN、计数；另加每连接器令牌桶限速（设计取舍 3） |
| 6 与现有接口 | POST 照旧；DELETE 照旧且对已退订目标返回成功；GET 仅在开关打开时追加 `source`、`last_active_at` |
| 7 指标 | 只追加 6 个族：`md_runtime_subscriptions_enabled`、`md_runtime_subscriptions`、`…_idle_unsubscribed_total`、`…_demand_subscribed_total`、`…_demand_failed_total`、`…_limit_rejected_total` |
| 8 文档 | `CHANGELOG.md` `[1.2.0]`；`OPS_GUIDE.md` 第一节热更表 + 第八节（开关、参数、日志串、指标、PromQL、临时关闭、排障）；`DEPLOYMENT.md` §12.3（构建、先关后开、换版后仍需 `restore_subs.mjs`、回滚）；`CONTEXT.md` 术语；`README.md` 指标列表 |

改动文件：`md-config`（配置段与校验）、`md-processor`（使用信号、`receiver_count`）、`md-connector`（拒绝上报、OKX 错误解析、`subscriptions_snapshot`）、
`md-gateway`（新模块 `runtime_subs.rs`、REST 接入）、`md-grpc`（断开即释放）、`md-server`（接线、SIGHUP、`/metrics`）。

### 设计取舍（请主审确认）

1. **OKX 订阅错误要从 `msg` 解析**：2026-11-06 实测 OKX 对不存在的合约 / 错误级别返回 `{"event":"error","msg":"Subscribe failed, wrong URL or channel:trades,instId:NOSUCH-USDT-SWAP doesn't exist…","code":"60018"}`，**不带 `arg`**（`upstream-probe/`）。1.1.1 因此解析出空 stream。现在从 `msg` 取 `channel:instId`。这条与开关无关：开关关闭时唯一可见变化是连接器 WARN 日志里 `stream=` 不再为空。
2. **币安无法识别拒绝**：币安对不存在的币对、错误的 K 线级别都应答 `{"result":null}`（实测）。所以币安 K 线按需补订只接受合约支持的 15 个级别；不存在的币对会被补订但收不到数据，接收方离开后闲置退订，总量受上限约束。
3. **每连接器限速**：本功能发出的订阅变更消息经令牌桶（突发 10、每 15 秒补 1，≤ 240 条/小时/连接器）。理由：同一 WS 连接上还有配置订阅（OMS / botd 的价格），OKX 对单连接订阅请求有频率限制；任务书未要求，属额外保护。
4. **`runtime_subscriptions` 可 SIGHUP 热更**：这样打开 / 临时关闭都不必重启（不断连、不丢运行期订阅）。现网配置烧在镜像里、root 所有，热更需 `docker exec -u root` 改文件后发 HUP，容器重建即回到镜像配置（OPS_GUIDE §8.6）。本机已在真实进程上验证 1→0→1。
5. **开关打开时 DELETE 一个仍有接收方的目标，会被按需补订立即补回**：按需补订的定义直接导致；这正是"DELETE 全局切断 OMS"问题的解法。DELETE 本身语义不变。
6. **gRPC 订阅流在客户端断开后立即释放接收端**（`tokio::select!` 加 `tx.closed()`）：否则没成交的话题会被已断开的流一直占着、永远判不成闲置。客户端可见行为不变。

## 二、怎么验证的（原文）

### 1. 门禁（`gates/f1d3c8a/`，干净 worktree，`dirty_after=0`）

```
$ bash evidence/1.2.0/tools/verify_gates.sh f1d3c8a
commit=f1d3c8a24a7c05c7e5e22cf4251e644382de59b5
fmt: exit=0; diff lines=0
clippy: exit=0; warning lines=13
workspace-test: exit=0; passed=286 failed=0
md-tests: exit=0; test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
compat_1_0_0: exit=0; test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
runtime_subs_gateway: exit=0; test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.31s
compare-selftest: exit=0; OK   selftest: metrics: 改类型必须被发现  (期望=fail, failed=1, inconclusive=0)
dirty_after=0
```

基线（`gates/baseline-d2f2cb6/`）：fmt 无输出、clippy 13 条、248 个通过。clippy 的 13 条 warning 所在文件与基线逐条相同（全部是既有代码）。
注：基线第一次运行 `compat_1_0_0` 5 个失败，原因是 `target/` 里缓存的测试二进制嵌入了另一个 worktree 的路径（`/tmp/md-final-a06fe20/...` 缺基线文件）；`cargo clean` 后 248 个全过，与代码无关。

### 2. 集成测试（`crates/md-gateway/tests/runtime_subs_gateway.rs`，假上游，闲置 600ms、检查 100ms）

假上游只向"当前已订阅"的 stream 推送，所以"上游收到退订 / 补订""推送恢复、不翻倍"都直接观察。连续跑 10 次 150/150 通过。

| 任务书场景 | 测试 |
|---|---|
| 运行期订阅无人接收超过闲置 → 上游收到退订、desired_subs 移除 | `idle_runtime_subscription_is_unsubscribed_upstream`（另断言实际闲置 ≥ 600ms、配置订阅没收到退订、指标 +1） |
| 配置名单订阅无人接收 → 永不退订 | `config_subscription_is_never_unsubscribed`（含配置 `BTCUSDT` + POST `btcusdt` 按配置处理） |
| 有 WS 接收方不退；断开后闲置内重连不退 | `ws_receiver_and_quick_reconnect_keep_subscription`、`legacy_ws_receiver_keeps_subscription` |
| 启动保护期内不退 | `startup_grace_defers_idle_unsubscribe` |
| 只读 REST 查询刷新闲置计时 | `rest_reads_refresh_idle_timer` |
| 被退后同一连接再订阅 → 自动补订、推送恢复、不翻倍 | `auto_unsubscribe_then_on_demand_resubscribe_on_same_connection_without_duplicates`（trade_id 精确为 `[3,4,5]`）、`subscribing_a_topic_missing_upstream_subscribes_on_demand` |
| 上游拒绝不反复重试；超过上限不补订、计数 | `upstream_rejected_symbol_is_not_retried`（用 OKX 实测错误格式；客户端反复重订，上游 subscribe 仍只 1 次）、`limit_refuses_on_demand_and_counts` |
| DELETE 已被自动退订的目标 → 成功 | `delete_of_auto_unsubscribed_target_succeeds`（`200 {"success":true}`） |
| 开关关闭 → 不退不补、GET 不追加字段 | `switch_off_changes_nothing`；运行中开关 `hot_toggle_takes_effect_without_restart` |
| GET 追加字段 / DELETE 仍有接收方的目标被补回 | `get_subscriptions_reports_source_and_last_active_when_enabled`、`delete_while_receiving_is_restored_on_demand` |

### 3. 正对照（`positive-controls/summary.txt`，命令 `bash evidence/1.2.0/tools/run_mutations.sh 6b3949e`）

```
commit=6b3949e8b82b311fb4fd691a3cdca68cd79e13c5
M1-receivers-ignored: CAUGHT; exit=101; failed=[legacy_ws_receiver_keeps_subscription subscribing_a_topic_missing_upstream_subscribes_on_demand ws_receiver_and_quick_reconnect_keep_subscription ]
M2-config-not-protected: CAUGHT; exit=101; failed=[config_subscription_is_never_unsubscribed idle_runtime_subscription_is_unsubscribed_upstream ]
M3-grace-ignored: CAUGHT; exit=101; failed=[hot_toggle_takes_effect_without_restart startup_grace_defers_idle_unsubscribe ]
M4-rest-reads-ignored: CAUGHT; exit=101; failed=[rest_reads_refresh_idle_timer ]
M5-no-reject-backoff: CAUGHT; exit=101; failed=[upstream_rejected_symbol_is_not_retried ]
M6-no-limit: CAUGHT; exit=101; failed=[limit_refuses_on_demand_and_counts ]
M7-no-on-demand: CAUGHT; exit=101; failed=[auto_unsubscribe_then_on_demand_resubscribe_on_same_connection_without_duplicates delete_while_receiving_is_restored_on_demand limit_refuses_on_demand_and_counts subscribing_a_topic_missing_upstream_subscribes_on_demand upstream_rejected_symbol_is_not_retried ]
M8-switch-ignored: CAUGHT; exit=101; failed=[hot_toggle_takes_effect_without_restart switch_off_changes_nothing ]
M9-unsubscribe-without-idle: CAUGHT; exit=101; failed=[idle_runtime_subscription_is_unsubscribed_upstream rest_reads_refresh_idle_timer ws_receiver_and_quick_reconnect_keep_subscription ]
M10-ws-unsubscribe-keeps-receiver-1.1.1-regression: CAUGHT; exit=101; failed=[auto_unsubscribe_then_on_demand_resubscribe_on_same_connection_without_duplicates no_events_after_unsubscribe partial_unsubscribe_keeps_other_streams_and_bytes repeated_churn_still_one_copy_per_event resubscribe_after_unsubscribe_delivers_exactly_one_copy unsubscribe_on_one_connection_does_not_affect_another ]
M11-grpc-keeps-receiver-after-disconnect: CAUGHT; exit=101; failed=[grpc_stream_counts_as_receiver_and_releases_on_disconnect ]
M12-okx-error-stream-not-parsed: CAUGHT; exit=101; failed=[subscribe_error_without_arg_carries_stream_from_msg upstream_rejected_symbol_is_not_retried ]
M13-sighup-does-not-apply-runtime-section: CAUGHT; exit=101; failed=[runtime_subscriptions_change_is_hot_applied_and_pushed ]
M0-unmutated: exit=0; passed=286 failed=0
overall=PASS
```

任务书点名的两条：M1「有接收方也退订」、M2「配置订阅也退订」都被抓住。每个改动的 diff 与完整测试输出在同目录 `M*.diff` / `M*.log`。

### 4. 开关关闭与 1.1.1 同窗口逐字节对比（`compat-local-switch-off/`）

本机同时运行 v1.1.1（`a06fe20`）与本分支（`6b3949e`）两个原生构建，同一份现网配置（不含新段），连真实交易所，各灌入 198 的同一份 13 条订阅，2026-11-06T14:07:59Z 起同时抓 240 秒。

```
$ cat evidence/1.2.0/compat-local-switch-off/gate-results.txt
selftest         OK   selftest: metrics: 改类型必须被发现  (期望=fail, failed=1, inconclusive=0) | exit=0
push-gateway     RESULT: PASS | exit=0
push-legacy      RESULT: PASS | exit=0
noquote-legacy   RESULT: PASS | exit=0
subs             RESULT: PASS | exit=0
metrics          RESULT: PASS | exit=0
quote            QUOTE-OK (only ruled quote.binance.BNBUSDT quantity noise, if any)
```

- push：6 个 tick、4 个 kline 话题在 Gateway 与旧 `/ws` 上全部逐字节一致（如 `tick.okx.BTC-USDT-SWAP` 10782=10782 条、`kline.5m.binance.BTCUSDT` 708 条，去掉 `connector_receive_ts` 后完全相同，成交集合无丢失无多出）。
- quote 按 1.1.1 惯例单独留证：`quote.okx.BNB-USDT-SWAP` 1444 条逐字节一致；`quote.binance.BNBUSDT` 1552 条中 9 条挂单数量不同，属 docs/33 §4 已裁定不计的数量配对噪声。第一次运行时 gateway 判据把 quote 也算进去得到 `RESULT: FAIL (1 项差异)`（即这 9 条），按惯例拆分后对同一份抓取数据重跑得到上表；含 quote 的那次对比用同一数据重新生成为 `compare-push-gateway-with-quote.log`。
- metrics：`old 样本 645, new 样本 651; 删除=0, 新增=6`，`TYPE 变更/删除=0, HELP 变更/删除=0`，新增的就是 6 个 `md_runtime_subscriptions*` 族。
- subs：13=13，字段与顺序一致（开关关闭时不追加字段）。

**限定**：这是本机原生构建（macOS arm64、系统分配器）对比，验证的是序列化与订阅行为；198 上用最终产物的同窗口对比随第五节第 1 条一起做。

### 5. 开关打开的金丝雀（`canary-local-switch-on/`，本机，真实交易所）

现网配置 + `runtime_subscriptions`（闲置 2 分钟、检查 10 秒、保护期 1 分钟），`RUST_LOG=warn,md_runtime_subs=info`；`restore_subs.mjs` 灌入 198 的 13 条；
OMS 类连接 A 全程订阅 `tick.binance.BNBUSDT`、`tick.okx.BNB-USDT-SWAP`；botd 类 POST SOLUSDT 的 TICK / KLINE 1m 并订阅，第 90 秒取消（不 DELETE）。

服务端日志原文（`runtime-subs.log`）：

```
2026-11-06T14:12:56.590155Z  INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED (idle_minutes=2, check_interval_seconds=10, startup_grace_minutes=1, max_runtime_subscriptions=200)
2026-11-06T14:13:36.660020Z  INFO md_runtime_subs: runtime subscription added on demand: exchange=okx type=TICK symbol=SOL-USDT-SWAP interval=- receivers=1 connector=okx
2026-11-06T14:15:06.838225Z  INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=TICK symbol=SNDKUSDT interval=- idle=2m00s connector=binance
2026-11-06T14:16:46.840966Z  INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=KLINE symbol=SOLUSDT interval=1m idle=2m10s connector=binance
2026-11-06T14:16:46.841928Z  INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=TICK symbol=SOLUSDT interval=- idle=2m10s connector=binance
2026-11-06T14:16:51.925789Z  INFO md_runtime_subs: runtime subscription added on demand: exchange=binance type=TICK symbol=SOLUSDT interval=- receivers=1 connector=binance
2026-11-06T14:19:07.821142Z  INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe DISABLED (current subscriptions are kept as they are)
2026-11-06T14:19:09.974256Z  INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED (idle_minutes=2, check_interval_seconds=10, startup_grace_minutes=1, max_runtime_subscriptions=200)
```

判据（`summary.json`，`pass: true`）：

| 判据 | 结果 |
|---|---|
| OMS 类话题不被退 | 两个 BNB tick 全程在订阅列表里；收到 911 / 455 条，重复 0 |
| 配置订阅保持 | 10 条配置订阅在全部 73 次采样中都在 |
| botd 取消后闲置到期被退 | 取消 t=90.1s → TICK / KLINE 在 t=225.3s 的采样中消失（日志 `idle=2m10s`） |
| 补入但没人接收的运行期订阅被退 | `SNDKUSDT` 在 t=125.2s 消失（日志 `idle=2m00s`） |
| OMS 类订阅已被退的话题 → 自动补订 | A 在同一连接订 `tick.binance.SOLUSDT`，5 秒内重新出现在列表，收到 302 条，重复 0 |
| 从未登记的话题按需补订 | `tick.okx.SOL-USDT-SWAP` 订阅 0.5 秒后补订，收到 820 条 |
| SIGHUP 不重启开关（OPS_GUIDE §8.6） | `before: enabled=1 start=1793974376 subs=14` → `after HUP (false): enabled=0 … subs=14` → `after HUP (true): enabled=1 … subs=14` |

结束时指标：`md_runtime_subscriptions 4`、`idle_unsubscribed_total 3`、`demand_subscribed_total 2`、`demand_failed_total 0`、`limit_rejected_total 0`。

### 6. 开关关闭时资源同量级（`resource-local-switch-off/`，本机 180 秒，各挂一个消费者）

```
old: samples=18 rss_mib mean=11.7 max=15.6 cpu_pct mean=0.68 max=1.50 external_established_conns=[5] cpu_time_end=0:01.76
new: samples=18 rss_mib mean=11.8 max=15.7 cpu_pct mean=0.66 max=1.50 external_established_conns=[5] cpu_time_end=0:01.73
```

开关关闭时多出的唯一常驻开销是一个空转的管理任务（`md_tokio_alive_tasks` 20 → 21，每 60 秒醒一次、不读订阅）。

### 7. 最终产物（`build-logs/dist-sha256.txt`）

```
$ bash evidence/1.2.0/tools/build_final.sh f1d3c8a
commit: f1d3c8a24a7c05c7e5e22cf4251e644382de59b5 dirty=0
SOURCE_DATE_EPOCH=1793974425
worktree=/tmp/md-final-f1d3c8a
CARGO_TARGET_DIR=/tmp/md-final-f1d3c8a-target
features=md-connector/vendored-tls,md-server/jemalloc-stats
rustc 1.95.0 (59807616e 2026-04-14)
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
cargo-zigbuild v0.22.3:
0.16.0
libprotoc 29.3
command: cargo zigbuild --locked --offline -p md-server --release --target x86_64-unknown-linux-musl --features md-connector/vendored-tls,md-server/jemalloc-stats
f6ab760cc2b62562b188104f10efaabf075723c6c8feae9dd2e14816c61e13ba  dist/md-server-1.2.0-f1d3c8a-rebuild-1-linux-x86_64
f6ab760cc2b62562b188104f10efaabf075723c6c8feae9dd2e14816c61e13ba  dist/md-server-1.2.0-f1d3c8a-rebuild-2-linux-x86_64
f6ab760cc2b62562b188104f10efaabf075723c6c8feae9dd2e14816c61e13ba  dist/md-server-1.2.0-f1d3c8a-linux-x86_64
dist/md-server-1.2.0-f1d3c8a-linux-x86_64: ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked, stripped
cmp: identical; dirty=0 after both builds
```

二进制内含 `md_jemalloc_`、`md_runtime_subscriptions_enabled`、`runtime subscription idle-unsubscribed`、`runtime subscription added on demand` 等标记（脚本逐条断言）。`f1d3c8a` 与源码提交 `6b3949e` 的 `crates/`、`Cargo.*` 无差异。

## 三、没做什么与原因

- **198 旁路金丝雀**：本机 `/tmp/pt_jwt`（11-05 22:35）已过期（`GET /api/endpoints/3` → `401 Invalid JWT token`），按 docs/33 §1 密码须业务方当场提供，未自行获取。脚本 `tools/canary_198.py` 已就绪（三个 256MiB / 2 核旁路容器：1.1.1 原样、1.2.0 开关关闭、1.2.0 开关打开；错开启动；读回二进制 SHA；同窗口 240 秒 compare + 开关打开消费者；结束删容器与镜像；现网只读）。
- **换版与打开后 24 小时采样**：须业务方批准，未做。
- **运行期订阅持久化**：任务书明确不做（另立项）。
- **币安不存在币对的拒绝识别**：上游不报错，无从缓存；靠级别白名单、闲置退订与上限兜底（设计取舍 2）。
- **gRPC `GetLatestTick/Kline` 不计入"只读查询"**：任务书只列 REST；需要的话一行代码可加。
- 未改 OMS、1stbotv3、153；未改推送内容、旧 `/ws`、proto；未打标签、未合并、未建 PR。CI 只在 `main` / `release/*` 触发，推送功能分支不触发 CI 或发布。

## 四、兼容性声明（docs/24 §三 8 条，开关关闭）

| # | 约束 | 结论与依据 |
|---|---|---|
| 1 | `tick.*` / `kline.*` 推送字段、顺序、数值格式、频率不变 | 满足。§二-4：10 个话题 Gateway 与旧 `/ws` 逐字节一致、频率比 1.00；`compat_1_0_0` 5/5 |
| 2 | REST latest、`/api/v1/subscriptions` JSON 不变，`data_type` 既有值不变 | 满足。开关关闭时 GET 不追加字段（`switch_off_changes_nothing` + `compare subs` PASS）；latest 响应未改（只多记一次使用，开关关闭时不记）。开关打开时 GET 每条追加 `source`、`last_active_at`，既有字段与顺序不变 |
| 3 | 旧 `/ws` 不含 quote | 满足。`noquote-legacy` PASS（35112 条数据消息，quote 0） |
| 4 | gRPC 既有 RPC、消息、字段编号不变 | 满足。`crates/md-proto` 未改动；gRPC 只改了服务端任务何时释放接收端 |
| 5 | 旧 `config.yaml` 可直接启动、行为一致 | 满足。仓库 `config.yaml` 与现网 `config.final.yaml` 都不含新段，测试 `legacy_config_without_runtime_section_defaults_off` 锁定；本机两侧服务端日志 0 条 WARN/ERROR |
| 6 | `md-tests` 真实执行 | 满足。11 个用例逐个执行（`gates/f1d3c8a/md-tests.log`，含 tick/kline/subscriptions 快照用例） |
| 7 | 指标名与标签只多不少 | 满足。`compare metrics`：删除 0、TYPE/HELP 变更 0、新增 6 个族（开关关闭时值为 0） |
| 8 | 开关关闭时 CPU / 内存 / 连接数同量级 | 本机满足（§二-6）。198 上的 `docker stats` 随金丝雀补充 |

另：开关关闭时与 1.1.1 唯一的行为差异是两处与开关无关的修复——OKX 订阅错误日志里的 `stream=` 不再为空、gRPC 流断开即释放接收端（设计取舍 1、6）。

## 五、需要对方做的事

1. **业务方（可选，建议做）——198 旁路金丝雀**：在本机按 DEPLOYMENT §4.6 第 1 步用 Portainer 密码换取 JWT，写入 `/tmp/pt_jwt`（权限 600；密码不要发给我、不进任何文件），然后告诉我执行：

   ```bash
   python3 evidence/1.2.0/tools/canary_198.py dist/md-server-1.2.0-f1d3c8a-linux-x86_64 f6ab760cc2b62562b188104f10efaabf075723c6c8feae9dd2e14816c61e13ba
   ```

   约 10 分钟，只建删 3 个带 `purpose=md-runtime-subs-1.2.0` 标签的旁路容器和 2 个 `md-server:exp-120-*` 镜像，现网 md-server 只读（只 GET 订阅列表与指标），198 出口多 15 条交易所连接（错开 5 秒启动）。结果写入 `evidence/1.2.0/nas-198/canary/` 并补进本报告。
2. **业务方——换版批准**：金丝雀通过后再决定是否以**开关关闭**换版（镜像 = 现网配置原样 + 本产物，流程同 SWAP_PLAN / DEPLOYMENT §12.3，换后立即 `restore_subs.mjs`）。打开开关另批，并提前通知主审；届时决定是否把 `RUST_LOG` 改为 `warn,md_runtime_subs=info`。
3. **主审——复核**：
   - 本机复跑：`bash evidence/1.2.0/tools/verify_gates.sh f1d3c8a`、`bash evidence/1.2.0/tools/run_mutations.sh 6b3949e`、`bash evidence/1.2.0/tools/local_compat.sh 240 6b3949e`、`bash evidence/1.2.0/tools/local_canary.sh 6b3949e`、`bash evidence/1.2.0/tools/build_final.sh f1d3c8a`。
   - 确认第一节「设计取舍」1–6。
   - 1stbotv3 侧：开关打开后，botd 删除关注币对的确认框文案（任务书第七节）；botd 现有行为无需改。注意 botd 若在 OKX 用小写或非规范写法订阅话题，本来就收不到推送，也不会触发按需补订。
4. **附带发现（不在本次范围，供决定）**：OKX 推送通知 `WebSocket port 8443 will be retired soon. Please migrate to port 443.`（`upstream-probe/`）。现网配置已经用 443（`wss://ws.okx.com/ws/v5/...`），不受影响；仓库示例 `config.yaml` 仍写 8443，是否顺手改由业务方定。
