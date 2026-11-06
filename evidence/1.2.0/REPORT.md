# 行情服务 1.2.0 第二轮报告：补改、金丝雀与换版准备

- 日期：2026-11-06（北京时间）。对象：主审复核意见「1.2.0 闲置自动退订与按需补订——补改、金丝雀与换版」（业务方转发）。第一轮报告：[`REPORT-r1-889bca8.md`](REPORT-r1-889bca8.md)。
- 分支 `feat/runtime-sub-idle-v1.2.0`（未打标签、未合并、未建 PR）。第二轮源码提交 `42bc897`，产物提交 `12192d9`（与 `42bc897` 的 `crates/`、`Cargo.*`、`config.yaml` 相同）。证据索引：[`README.md`](README.md)。

## 首段

**第三节补改：已完成**。① 新增可选键 `runtime_subscriptions.on_demand_quote`（代码默认 `false`，现网镜像配置也写 `false`），QUOTE 只在它打开时参与按需补订，可单独 SIGHUP 热更；补了 2 个集成测试、1 个配置测试、1 个 SIGHUP 测试和 3 个正对照。② 仓库示例配置 OKX 地址改为 443（`config.yaml`、DEPLOYMENT / RELEASE_WORKFLOW 样例、`deploy.sh` 模板）。③ 重出产物、重跑门禁与开关关闭同窗口对比，全部通过。另按复核报告的建议补写了 OPS_GUIDE 的三条语义说明。

**新产物**：`dist/md-server-1.2.0-12192d9-linux-x86_64`，SHA-256 **`cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf`**（jemalloc 特性，两次从零构建一致）。第一轮的 `f6ab760c…` 作废。

**金丝雀两部分**：**198 上没做**——本机 Portainer 会话仍是 HTTP 401，需要业务方提供。同样的流程在本机连真实交易所做了，结果如下（**本机结果不能代替 198 金丝雀**，198 用的产物和环境不同）：
- 第 1 部分（字节对比，开关关闭的 1.2.0 对 1.1.1，同窗口 240 秒）：push-gateway、push-legacy、noquote、subs、metrics、selftest 全部 PASS；quote 只有已裁定的 `quote.binance.BNBUSDT` 数量噪声（1316 条中 8 条）。
- 第 2 部分（开关打开、报价补订关闭）：试跑（闲置 2 分钟）12/12 判据通过，现网参数复跑（闲置 10、保护 10、检查 60 秒）12/12 判据通过。

**报价话题确认未被补订**（本机，两次运行）：订阅 `quote.binance.BTCUSDT`、`quote.binance.ETHUSDT`、`quote.okx.BTC-USDT-SWAP`、`quote.okx.ETH-USDT-SWAP` 后，订阅列表里从未出现新的 QUOTE，这 4 个话题收到 0 条；`md_quotes_processed` 速率在订阅报价话题前后为 170→146 条/秒、194→132 条/秒（没有跳升，变化来自行情本身）；配置里的 BNB 报价照常推送。

**需要业务方决定 / 提供**：
1. 提供 Portainer 会话（本机按 DEPLOYMENT §4.6 第 1 步换 JWT 写入 `/tmp/pt_jwt`，密码不要发给我），我执行 198 金丝雀两部分（约 25 分钟，只建删带标签的旁路容器与 `md-server:exp-120-*` 镜像，现网只读）。
2. 金丝雀通过后，批准换版窗口（低峰时间）。按你们的决定直接以开关打开换版：报价补订关闭，容器环境 `RUST_LOG=warn,md_runtime_subs=info`。我会在执行前把时间告诉你转主审，工具和计划已就绪（`nas-198/SWAP_PLAN_1.2.0.md`）。

---

## 一、做了什么

### 1. 报价补订开关（源码 `42bc897`）

| 项 | 内容 |
|---|---|
| 配置 | `runtime_subscriptions.on_demand_quote: bool`，`#[serde(default)]` = `false`；属 `runtime_subscriptions` 段，随整段 SIGHUP 热更（单改这一个键也会立即生效，日志 `runtime_subscriptions changed: {…, on_demand_quote=false} -> {…, on_demand_quote=true} (applied)`） |
| 行为 | 按需补订时，QUOTE 目标在 `on_demand_quote=false` 时直接跳过（不路由、不计上限拒绝、不发上游消息，只记一行 debug）。配置名单里的报价、POST 登记的报价不受影响；POST 登记的报价照常参与闲置退订 |
| 日志 | 开关打开 / 参数变化的 INFO 行追加 `on_demand_quote=…`；打开状态下改参数会立即触发一次补订检查，单独打开报价补订后约 200ms 生效 |
| 测试 | `quote_topics_are_not_subscribed_on_demand_by_default`（关闭：名单外 quote 话题不补订 QUOTE、不计上限拒绝；同一连接的 tick 照常补订；名单内 BNB 报价照常推送、名单外无数据）、`quote_topics_are_subscribed_on_demand_when_enabled`（运行中打开后补订 QUOTE、推送到达）、`on_demand_quote_defaults_off_and_parses`、`on_demand_quote_alone_is_hot_applied`、`settings_follow_yaml`（映射） |
| 正对照 | M14 关闭时仍补订报价、M15 打开后仍不补订报价、M16 YAML 的键没有映射到设置——都被抓住 |

### 2. 示例配置 443

`config.yaml`、`DEPLOYMENT.md` §6.1 样例、`RELEASE_WORKFLOW.md` 样例、`deploy.sh` 配置模板里的 `wss://ws.okx.com:8443/ws/v5/…` 改为 `wss://ws.okx.com/ws/v5/…`（与现网配置写法相同）。单元测试里的 8443 是当时抓样本的历史地址，没有改。

### 3. 文档（按复核报告「建议修」）

- OPS_GUIDE §8：`on_demand_quote` 的含义与流量估算；SIGHUP 从配置名单删掉但仍有人接收的目标会按运行期订阅补回；闲置退订后 REST latest 返回最后缓存的旧值、REST 查询不触发补订；`md_runtime_subscriptions_limit_rejected_total` 是**尝试次数**；只关报价补订的热关闭方法。
- DEPLOYMENT §12.3：按业务方决定改为开关打开上线；产物 sha 依赖构建路径（只能在 `/tmp/md-final-<短提交>[-target]` 下复现）；两级回退。
- CHANGELOG `[1.2.0]`：新键、443、新产物 sha、测试数。

### 4. 198 金丝雀与换版工具（证据工具，不进产物）

- `tools/canary_198.py`：第 1 部分（old=1.1.1 原样、off=1.2.0 开关关闭、short=1.2.0 试跑配置）同时运行，old/off 同窗口 240 秒 compare，short 跑三类消费者 420 秒；第 2 部分 prod 容器用 `nas-198/config.release.yaml`（即换版镜像配置）跑三类消费者 900 秒。每个 1.2.0 容器都从运行中读回 `/usr/local/bin/md-server` 的 sha 断言 = 产物 sha，并读回配置；全程每 10 秒采样 docker stats（内存、CPU）；前后读现网 `/metrics` 断言 `process_start_time_seconds` 不变。
- `tools/canary_consumers.mjs`：三类消费者。1stbotv3 类按 botd 顺序先退 WS 话题、约 50ms 后 DELETE 自己登记的 KLINE（TICK 不 DELETE）；报价类订阅 4 个名单外报价话题与 2 个名单内 BNB 报价话题。
- 换版：`tools/build_release_image.py`（`md-server:1.0.0-rollback` + 产物 + `config.release.yaml`，记录镜像 ID 与配置 sha）、`tools/execute_swap.py`（断言金丝雀已通过、现网是 1.1.1；换容器时只把 `RUST_LOG` 改成 `warn,md_runtime_subs=info`，其余参数沿用；健康后立刻 `restore_subs.mjs`；容器内二进制与配置逐字节核对；失败自动回退到 `md-server-1.1.1-bak`）、`tools/check_after_swap.py`（10 分钟验收，只读）、`tools/observe_24h.sh`（24 小时每分钟采样）。`scripts/compat/sample_metrics.mjs` 只追加了 7 列（运行期订阅 6 项 + 各 quote 序列入站累计），已对 198 只读试采一行。
- 配置：`nas-198/config.release.yaml` 与 `config.release.diff`（相对现网 `config.final.yaml` 只在末尾追加 `runtime_subscriptions` 段）。

## 二、怎么验证的（原文）

### 1. 门禁（`gates/12192d9/`，干净 worktree）

```
$ bash evidence/1.2.0/tools/verify_gates.sh 12192d9
commit=12192d966daf7513f4357cf0a696836ae472c87b
fmt: exit=0; diff lines=0
clippy: exit=0; warning lines=13
workspace-test: exit=0; passed=290 failed=0
md-tests: exit=0; test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
compat_1_0_0: exit=0; test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
runtime_subs_gateway: exit=0; test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.31s
compare-selftest: exit=0; OK   selftest: metrics: 改类型必须被发现  (期望=fail, failed=1, inconclusive=0)
dirty_after=0
```

### 2. 正对照（`positive-controls/summary.txt`）

```
$ bash evidence/1.2.0/tools/run_mutations.sh 42bc897
commit=42bc897e6b9180214cf50a5264a28131b8196b3e
M1-receivers-ignored: CAUGHT …          M2-config-not-protected: CAUGHT …        M3-grace-ignored: CAUGHT …
M4-rest-reads-ignored: CAUGHT …         M5-no-reject-backoff: CAUGHT …           M6-no-limit: CAUGHT …
M7-no-on-demand: CAUGHT …               M8-switch-ignored: CAUGHT …              M9-unsubscribe-without-idle: CAUGHT …
M10-ws-unsubscribe-keeps-receiver-1.1.1-regression: CAUGHT …                     M11-grpc-keeps-receiver-after-disconnect: CAUGHT …
M12-okx-error-stream-not-parsed: CAUGHT …                                        M13-sighup-does-not-apply-runtime-section: CAUGHT; failed=[on_demand_quote_alone_is_hot_applied runtime_subscriptions_change_is_hot_applied_and_pushed ]
M14-quote-on-demand-ignores-off-switch: CAUGHT; exit=101; failed=[quote_topics_are_not_subscribed_on_demand_by_default quote_topics_are_subscribed_on_demand_when_enabled ]
M15-quote-never-on-demand-even-when-on: CAUGHT; exit=101; failed=[quote_topics_are_subscribed_on_demand_when_enabled ]
M16-yaml-on-demand-quote-not-mapped: CAUGHT; exit=101; failed=[settings_follow_yaml ]
M0-unmutated: exit=0; passed=290 failed=0
overall=PASS
```

（M1–M12 每行的失败测试名单见 `summary.txt` 原文，与第一轮相同或更多。）

### 3. 最终产物（`build-logs/dist-sha256.txt`）

```
$ bash evidence/1.2.0/tools/build_final.sh 12192d9
commit: 12192d966daf7513f4357cf0a696836ae472c87b dirty=0
SOURCE_DATE_EPOCH=1793977930
worktree=/tmp/md-final-12192d9
CARGO_TARGET_DIR=/tmp/md-final-12192d9-target
features=md-connector/vendored-tls,md-server/jemalloc-stats
command: cargo zigbuild --locked --offline -p md-server --release --target x86_64-unknown-linux-musl --features md-connector/vendored-tls,md-server/jemalloc-stats
cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf  dist/md-server-1.2.0-12192d9-rebuild-1-linux-x86_64
cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf  dist/md-server-1.2.0-12192d9-rebuild-2-linux-x86_64
cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf  dist/md-server-1.2.0-12192d9-linux-x86_64
dist/md-server-1.2.0-12192d9-linux-x86_64: ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked, stripped
cmp: identical; dirty=0 after both builds
```

二进制内含 `md_jemalloc_`、`md_runtime_subscriptions_enabled`、`on_demand_quote`、`on-demand subscribe skipped, on_demand_quote=false` 等标记。

### 4. 开关关闭与 1.1.1 同窗口对比（`compat-local-switch-off/`，本机，2026-11-06T15:29:50Z 起 240 秒）

```
$ bash evidence/1.2.0/tools/local_compat.sh 240 12192d9
old: a06fe2095e8fc58bff1e038a594b25e7002aff1b …   new: 12192d966daf7513f4357cf0a696836ae472c87b …
after: total=13 stillMissing=0 extraVsBaseline=0      (两侧各一行)
selftest         OK   selftest: metrics: 改类型必须被发现  (期望=fail, failed=1, inconclusive=0) | exit=0
push-gateway     RESULT: PASS | exit=0
push-legacy      RESULT: PASS | exit=0
noquote-legacy   RESULT: PASS | exit=0
subs             RESULT: PASS | exit=0
metrics          RESULT: PASS | exit=0
quote            QUOTE-OK (only ruled quote.binance.BNBUSDT quantity noise, if any)
```

| 话题 | Gateway old/new（逐字节相同的对应记录） | 旧 `/ws` old/new |
|---|---|---|
| tick.binance.BTCUSDT | 10481 / 10481（10481） | 10481 / 10481 |
| tick.binance.ETHUSDT | 9696 / 9696（9696） | 9695 / 9696 |
| tick.binance.BNBUSDT | 1511 / 1511（1511） | 1511 / 1511 |
| tick.okx.BTC-USDT-SWAP | 14497 / 14497（14497） | 14497 / 14497 |
| tick.okx.ETH-USDT-SWAP | 15650 / 15650（15650） | 15650 / 15650 |
| tick.okx.BNB-USDT-SWAP | 530 / 530（530） | 530 / 530 |
| kline.5m.binance.BTCUSDT | 741 / 741（741） | 741 / 741 |
| kline.5m.binance.ETHUSDT | 759 / 759（758） | 759 / 759 |
| kline.5m.okx.BTC-USDT-SWAP | 430 / 430（429） | 430 / 430 |
| kline.5m.okx.ETH-USDT-SWAP | 454 / 454（453） | 454 / 454 |

metrics：`old 样本 645, new 样本 651; 删除=0, 新增=6`，新增的就是 6 个 `md_runtime_subscriptions*` 族；noquote：`数据消息 54749 条, 其中 quote 0 条`；subs：`old=13 条, new=13 条, 缺失=0, 非预期多出=0`。
quote 单独留证：`quote.okx.BNB-USDT-SWAP` 1484 条对应记录逐字节相同；`quote.binance.BNBUSDT` `对应记录 1316 条中 8 条不同`（挂单数量，docs/33 §4 已裁定不计）。原始抓取 `captures.tar.gz` + `raw-sha256.txt`。

资源（`resource-local-switch-off/`，180 秒，各挂一个消费者，两侧都收到约 3 万条）：

```
old: samples=18 rss_mib mean=11.5 max=15.5 cpu_pct mean=1.58 max=4.20 external_established_conns=[5] cpu_time_end=0:02.99
new: samples=18 rss_mib mean=11.6 max=15.8 cpu_pct mean=1.38 max=2.70 external_established_conns=[5] cpu_time_end=0:02.96
```

第一次资源采样无效：两边服务端启动时端口被占（`AddrInUse`，当时有别的进程占着 54051 / 13081），消费者收到 0 条。脚本已改为端口可配置、启动前检查端口、启动后检查健康、消费者收到 0 条即判无效；改后重跑得到上面的结果。

### 5. 开关打开金丝雀（本机，真实交易所，`RUST_LOG=warn,md_runtime_subs=info`，灌入 198 的 13 条订阅）

**试跑**（`canary-local-switch-on-short/`，闲置 2 分钟、检查 10 秒、保护 1 分钟、报价补订关闭）：

```
INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED (idle_minutes=2, check_interval_seconds=10, startup_grace_minutes=1, max_runtime_subscriptions=200, on_demand_quote=false)
INFO md_runtime_subs: runtime subscription added on demand: exchange=okx type=TICK symbol=SOL-USDT-SWAP interval=- receivers=1 connector=okx
INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=TICK symbol=SNDKUSDT interval=- idle=2m00s connector=binance
INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=TICK symbol=SOLUSDT interval=- idle=2m09s connector=binance
INFO md_runtime_subs: runtime subscription added on demand: exchange=binance type=TICK symbol=SOLUSDT interval=- receivers=1 connector=binance
INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe DISABLED (current subscriptions are kept as they are)
INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED (… on_demand_quote=false)
```

**现网参数复跑**（`canary-local-switch-on-prod/`，配置 = `nas-198/config.release.yaml`）：

```
INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED (idle_minutes=10, check_interval_seconds=60, startup_grace_minutes=10, max_runtime_subscriptions=200, on_demand_quote=false)
INFO md_runtime_subs: runtime subscription added on demand: exchange=okx type=TICK symbol=SOL-USDT-SWAP interval=- receivers=1 connector=okx
INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=TICK symbol=SNDKUSDT interval=- idle=10m49s connector=binance
INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=TICK symbol=SOLUSDT interval=- idle=10m49s connector=binance
INFO md_runtime_subs: runtime subscription added on demand: exchange=binance type=TICK symbol=SOLUSDT interval=- receivers=1 connector=binance
```

两次的判据（`summary.json`，`pass: true`）：

| 判据 | 试跑（闲置 2 分钟） | 现网参数（闲置 10 分钟） |
|---|---|---|
| OMS 类话题不被退、重复 0 | 通过 | 通过（BNB 两个 tick 收到 6105 / 2366 条，重复 0） |
| 配置订阅全程不少（每次采样） | 通过 | 通过 |
| 1stbotv3 类：先退 WS 再 DELETE 的 KLINE 立即消失、不被补回 | t=91 DELETE，t=91 已消失 | t=60.4 DELETE，t=60.5 已消失 |
| 1stbotv3 类：TICK 闲置到期被退、在重订前不被补回 | 取消 t=91 → t=221.9 被退（日志 `idle=2m09s`） | 取消 t=60.3 → t=713.4 被退（日志 `idle=10m49s`） |
| 补入但没人接收的运行期订阅被退 | SNDKUSDT t=121.1 | SNDKUSDT t=653.2（保护期结束后） |
| 重订已退话题 → 自动补订、推送不翻倍 | 5 秒内补回，898 条，重复 0 | 5 秒内补回，924 条，重复 0 |
| 从未登记的 tick 话题按需补订 | OKX SOL 5 秒内补订 | OKX SOL 5 秒内补订，4259 条 |
| 名单外报价话题不补订 QUOTE、无数据 | 4 个话题均 0 条，订阅列表无新 QUOTE | 同左 |
| 名单内 BNB 报价照常推送 | 3461 / 2500 条 | 7520 / 5492 条 |
| `md_quotes_processed` 速率（订阅报价话题前→后） | 170.1 → 145.9 条/秒（比值 0.86） | 193.6 → 132.0 条/秒（比值 0.68） |

结束时计数（两次相同）：`md_runtime_subscriptions 4`、`idle_unsubscribed_total 2`、`demand_subscribed_total 2`、`demand_failed_total 0`、`limit_rejected_total 0`、`md_ticks_dropped 0`、`md_quotes_dropped 0`。
试跑结束后改配置文件发 SIGHUP：`before: enabled=1 start=1793978159 subs=14` → `after HUP (false): enabled=0 … subs=14` → `after HUP (true): enabled=1 … subs=14`。
**限定**：macOS 上 `/metrics` 的 `process_*` 为 0，所以本机金丝雀没有 RSS / CPU 前后对比；198 金丝雀用 docker stats 每 10 秒采样，并读现网 `/metrics` 前后对比。

## 三、没做什么与原因

- **198 旁路金丝雀两部分**：本机 `/tmp/pt_jwt` 仍过期（`GET /api/endpoints/3` → `401 Invalid JWT token`），密码须业务方当场提供。脚本已就绪并做过语法检查；其中 Portainer 调用（建镜像、起容器、读回文件、日志、stats、删除）沿用 1.1.1 金丝雀与换版时用过的接口，但**新脚本本身还没在 198 上跑过**。
- **换版与 24 小时采样**：须金丝雀通过并经业务方批准。
- RSS / CPU 在 198 上的前后对比：随 198 金丝雀补。
- 没有改 OMS、1stbotv3、153；没有动 198（本轮只对 198 做了 GET：采样器试采一行）。

## 四、兼容性声明（docs/24 §三 8 条，开关关闭；与第一轮相同，用新产物重验）

| # | 约束 | 结论与依据 |
|---|---|---|
| 1 | `tick.*` / `kline.*` 推送不变 | 满足。10 个话题 Gateway 与旧 `/ws` 逐字节一致（§二-4）；`compat_1_0_0` 5/5 |
| 2 | REST latest、`/api/v1/subscriptions` JSON 不变 | 满足。开关关闭时 GET 不追加字段，`compare subs` PASS |
| 3 | 旧 `/ws` 不含 quote | 满足。`noquote-legacy` PASS（54749 条，quote 0） |
| 4 | gRPC / proto 不变 | 满足。`crates/md-proto` 未改动 |
| 5 | 旧 `config.yaml` 可直接启动 | 满足。新键可选、有默认值；现网 `config.final.yaml` 原样启动、日志 0 条 WARN/ERROR |
| 6 | `md-tests` 真实执行 | 满足。11 个用例逐个执行 |
| 7 | 指标只多不少 | 满足。删除 0、TYPE/HELP 变更 0、新增 6 个族 |
| 8 | 开关关闭时资源同量级 | 本机满足（RSS 11.5 / 11.6 MiB，CPU 1.58% / 1.38%，连接 5 / 5）；198 上随金丝雀补 |

开关打开、报价补订关闭时，对报价的影响只有一条：配置名单里的报价不变，名单外的报价话题收不到数据（与 1.1.1 相同）。

## 五、需要对方做的事

1. **业务方——提供 Portainer 会话**：本机按 DEPLOYMENT §4.6 第 1 步换 JWT 写入 `/tmp/pt_jwt`（权限 600），然后告诉我执行：

   ```bash
   python3 evidence/1.2.0/tools/canary_198.py dist/md-server-1.2.0-12192d9-linux-x86_64 cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf
   ```

   约 25 分钟：第 1 部分 3 个旁路容器同时运行（198 出口多 15 条交易所连接，错开 5 秒启动），第 2 部分 1 个；各 256 MiB / 2 核，结束删除容器与 3 个 `md-server:exp-120-*` 镜像；现网只读。
2. **业务方——批准换版窗口**（金丝雀通过后）：换版按 `nas-198/SWAP_PLAN_1.2.0.md`，执行前我把时间告诉你转主审。
3. **主审——复核**：本机可复跑 `verify_gates.sh 12192d9`、`run_mutations.sh 42bc897`、`local_compat.sh 240 12192d9`、`MODE=short|prod local_canary.sh 12192d9`；`build_final.sh 12192d9` 只能在同一路径复现 sha（复核报告第二节已指出）。
