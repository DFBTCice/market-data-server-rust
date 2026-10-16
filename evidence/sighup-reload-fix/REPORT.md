# SIGHUP 配置重载 + `log_level` 生效：调查与修复报告

> 分支 `fix/sighup-config-reload`（基于 `eb78524`，即 PR #2 分支 `fix/gateway-unsubscribe-leak-v1.1.1` 的 HEAD，线性叠在其上）。
> 未打标签、未改版本号；**198 线上未做任何操作**。修复后的静态二进制只在本机 Docker 里验证过，**未上现网**。

## 1. 结论

1. SIGHUP 部分，你给的三条判断**逐条复核，全部成立**；用线上同一个二进制（SHA-256 `d73f7a4c…ba60`）和 1.1.1 分支 HEAD 都复现了，两者行为一致。
2. **币安同样有问题**：两个地址（`binance.stream_base_url`、`binance.stream_base_url_public`）检测是齐的，没有"漏检"；
   但"基线被污染"（第二次 SIGHUP 误报成功）和"重连不换地址"与 OKX 完全相同。
3. 调查中还发现 6 项同类问题（见 §2.3），其中最实际的一条：**线上 `RUST_LOG=warn` 下，只改 OKX Business 地址再发 SIGHUP，日志里没有任何输出**——因为"成功"那行是 INFO，被过滤了，而"需要重启"的提示根本没有产生。
4. SIGHUP 已修复。保留"地址变更需要重启"的设计，没有做连接器热重建。
5. **`log_level` 不起效也已修复**：YAML 的 `log_level` 以前只被读出来打印一行，从未接到日志过滤器；现在**启动时生效，SIGHUP 可热更（不重连、不重启）**。
   优先级是 `RUST_LOG`（已设置时）＞ YAML `log_level` ＞ `info`（取舍与理由见 §3.2、§6）。
   **注意线上 198：容器里设了 `RUST_LOG=warn`，所以换版后实际级别仍是 warn；要让 YAML 接管，需要在换版时去掉这个环境变量。**
6. 验收全部通过（§4），验证数据见 §5。

## 2. 调查

### 2.1 方法

- 读代码：`main.rs` 的 `wait_for_signal()` / `apply_subscription_changes()`、`okx.rs`、`binance.rs`、`base.rs`、`md-config`。
- 对线上二进制做 `strings`：只有三条"`… changed (requires restart)`"文案（binance 两个、okx public 一个），**没有 business**。
- 写了端到端脚本（`scripts/compat/sighup_reload_e2e.mjs`）：**真实 md-server 进程 + 真实 SIGHUP + 假上游 WebSocket 服务**，
  每条断言都写成"正确行为应该是什么"，所以有缺陷的版本会变红。分别跑了：
  线上 1.1.0 二进制（Docker，`linux/amd64`）和 HEAD `eb78524`（本机原生）。
  两组都是 70/129 通过，**失败的 59 条完全相同**（其中 SIGHUP 相关 31 条，`log_level` 相关 28 条）——说明线上 1.1.0 二进制与 1.1.1 分支 HEAD 有同样的缺陷。

### 2.2 三条判断

| # | 你的判断 | 结论 | 实测（场景） |
|---|---|---|---|
| 1 | 漏查 OKX Business 地址 | ✅ 成立 | 只改 Business 地址：第 1、2 次 SIGHUP 都是 `config reloaded and applied successfully`；新上游 0 连接，旧上游连接仍在 |
| 2 | 把未应用的配置记成当前配置 | ✅ 成立 | OKX Public / Binance market / Binance public：第 1 次提示需重启，**第 2 次变成"成功"**，连接器仍连旧地址 |
| 3 | 自动重连不会补上新配置 | ✅ 成立（设计行为，按你的建议保留） | 改 4 个地址 + SIGHUP 后掐断旧上游：4 类连接全部**回连旧地址**，新地址 0 连接；重启后全部连新地址 |

线上二进制上的原始日志（`before-prod-1.1.0-docker/`）：

```
# OKX Public，连续两次 SIGHUP
WARN md_server:   okx.stream_base_url_public changed (requires restart)
WARN md_server: some config changes require connector restart -- please restart the service
INFO md_server: config reloaded and applied successfully          <-- 第 2 次，连接器仍用旧地址

# OKX Business，连续两次 SIGHUP
INFO md_server: config reloaded and applied successfully          <-- 第 1 次就误报
INFO md_server: config reloaded and applied successfully

# RUST_LOG=warn（198 实际的日志级别；脚本另放开 md_connector=info 仅用于探测连接就绪）下改 Business 地址：两次 SIGHUP 均无任何输出
```

### 2.3 同类问题（均已复现，均已修复）

| 问题 | 复现 |
|---|---|
| 带 `--port-offset` / 环境变量覆盖时，每次 SIGHUP 都误报"监听地址变更"（重载没走启动时的 env 覆盖 + 端口偏移） | s6、s7 |
| 订阅列表变了但连接器没在运行（OKX tick / K 线：交易所未启用或启动时列表为空）→ **静默吞掉并报成功**；quote 只警告一次，第二次报成功 | s8 |
| 订阅下发失败（连接器返回错误）：旧代码只打一条 WARN，随后仍报成功，并且基线前进、不会重试 | 代码阅读（旧 `main.rs` 约 740–840 行）；新行为见单元测试 `failed_apply_keeps_old_list_and_is_retried_on_next_reload` |
| `reconnect_delay`、`ping_interval`、`rest_base_url`、`processor.*`、`api_gateway.*` 等只在启动时读取的字段，旧代码的 SIGHUP 完全不检测（只查了监听地址、`enabled`/`quotes_enabled` 开关和 3 个地址） | 代码阅读（旧 `main.rs` 约 576–640 行）；新行为见单元测试 `every_restart_only_field_is_detected_individually`（逐字段改动，全部必须被检出） |
| **`log_level` 从未生效**：YAML 的值只被打印，日志过滤器始终取自 `RUST_LOG`（没设就是 info）；SIGHUP 还把 `log_level` 变更标注成 `(applied)` | s9a、s9b、s9c、s9e、s9f |
| 读取 / 解析失败只是 WARN | s5 |

`log_level` 在旧二进制上的表现（`before-head-eb78524-native/`，线上 1.1.0 同样）：

```
# YAML log_level: warn，没设 RUST_LOG：启动照样输出全部 INFO（s9b）
INFO md_server: log_level: warn
INFO md_server: gRPC: 127.0.0.1:…
INFO md_server: starting Binance connector
…
# SIGHUP 把 info 改成 warn（s9a）：声称已应用，下一次无变更的 SIGHUP 仍在输出 INFO
INFO md_server:   log_level: info -> warn (applied)
INFO md_server: config reloaded and applied successfully
INFO md_server: received SIGHUP, reloading config from …          <-- 级别其实没变
```

> 198 线上情况：`evidence/1.1.0/nas-198/md-server.inspect.stage2.json` 记录的启动命令没有 `--port-offset`、没有环境变量覆盖，所以线上不会触发端口偏移那条误报；
> `RUST_LOG=warn` 是确实存在的；YAML 里写的是 `log_level: "info"`（`config.yaml.running-1.0.0`、`config.yaml.stage2-quote-bnb`）。**所以在 198 上，"改 YAML 的 log_level"此前从来没起过作用，实际一直是 warn。**

## 3. 修复

### 3.1 SIGHUP 配置重载

核心是 `crates/md-server/src/reload.rs`（新）：

1. **区分"文件中的新配置"和"已应用配置"。** `ReloadState` 只保存进程真正在用的配置。
   每次 SIGHUP 都拿文件和它比较；只有**热更成功的订阅列表 / 日志级别**才会写进去，需重启的字段永远不写。
   所以重复 SIGHUP 会**持续提示**，改回原值后提示**自动消失**；不再有 `current_cfg = new_cfg`。
2. **启动与重载共用同一个"生效配置"推导**：`load_effective_config` = YAML + 环境变量覆盖 + 端口偏移。
3. **字段分类用结构体穷尽解构（不写 `..`）**：`Config` 新增任何字段，`restart_only_fields` 会编译失败，必须显式归类；默认"需重启"。OKX Business 就是这样漏掉的；今后新增字段如果忘了归类，编译不过，不会再静默漏检。
4. **订阅列表热更保持原有行为**（对运行中的连接器增量 add/remove）。唯一区别：连接器缺失 / 下发失败的分组不再静默，作为待处理持续提示、不推进已应用配置，失败的下次 SIGHUP 重试。
5. **日志**：待重启项逐条点名（YAML 路径、旧值、文件值）；URL 类额外写明"自动重连不会重读配置"；读取失败为 ERROR 并写明"运行配置保持不变"。
   保留 `config reloaded and applied successfully`、`some config changes require connector restart -- please restart the service`（末尾追加 `(N pending: …)`）、`failed to reload config`，日志 target 仍是 `md_server`。

修复后同样的场景（静态二进制，Docker，`RUST_LOG=warn,md_connector=info`）：

```
WARN md_server:   connectors.okx.stream_base_url_business changed: "ws://…:54673/ws" -> "ws://…:54674/ws" (NOT applied, requires restart; connectors keep using the startup address; auto-reconnect does not re-read the config file)
WARN md_server: some config changes require connector restart -- please restart the service (1 pending: connectors.okx.stream_base_url_business)
ERROR md_server: failed to reload config: Parse error: …, keeping current running config (nothing applied)
```

### 3.2 `log_level` 真正生效

新增 `crates/md-server/src/logging.rs`，并改 `main.rs`（启动顺序）、`reload.rs`（SIGHUP 步骤 0）、`md-config`（校验）：

1. **过滤器套一层 `tracing_subscriber::reload`**：SIGHUP 直接切换过滤器（会重建 callsite 缓存），不重连、不清订阅；输出格式不变（默认 fmt 层、stdout）。
2. **优先级：`RUST_LOG`（已设置、非空、写法合法）＞ YAML `log_level`（环境变量 `LOG_LEVEL` 可覆盖）＞ `info`。**
   - 为什么 `RUST_LOG` 在前：环境变量优先于配置文件是通行约定；而且**现网都设了 `RUST_LOG=warn`**（198、compose、systemd 样例）。
     如果反过来让 YAML 优先，198 上 YAML 写的是 `info`，换版后日志量会被悄悄放大。
   - 两者同时存在且不一致时**不装作生效**：启动时 WARN 点名两者（`config log_level "info" is overridden by RUST_LOG=warn: …`）；
     SIGHUP 改 `log_level` 时每次提示 `log_level changed: A -> B (NOT applied, requires restart; RUST_LOG=… takes precedence …)`，直到改回或重启。
   - `RUST_LOG` 本身写错（旧版本悄悄退回 info）：现在退回 YAML 的 `log_level`，并 WARN。
3. **SIGHUP 时先应用日志级别**：调高详细度时，同一次重载后面的日志就能看到；调低时，"applying…"那行仍在旧级别下输出，"(applied)"在新级别下输出。
   切换失败则保持旧级别，作为待处理项，下一次 SIGHUP 重试。
4. **校验**：`log_level` 必须是 `trace` / `debug` / `info` / `warn`（也接受 `warning`）/ `error`，不区分大小写。别的写法（`fatal`、`verbose`、拼写错误）
   **加载配置时直接拒绝**（启动失败 / SIGHUP 保持运行配置），不再悄悄忽略。这样做还因为：`EnvFilter` 遇到不认识的单词会把它当成 target 名，**所有日志都被静默**。
   环境变量 `LOG_LEVEL` 覆盖进来的值走同一道校验；生效配置里存规范化后的小写写法，`INFO` 与 `info` 不会被当成变更。
5. **启动顺序**：先读配置、再初始化日志（过滤器取决于 `log_level`）。读配置失败的报错只能写 stderr（这一行原本就是 `eprintln!`）。

SIGHUP 切换日志级别的实际输出（原生进程，没设 `RUST_LOG`，`s9a`）：

```
INFO md_server: received SIGHUP, reloading config from …
INFO md_server:   log_level changed: info -> warn, applying...        <-- 切换前，仍在 info 下输出
（warn 级别下：无变更的 SIGHUP 没有任何输出；待重启提示以 WARN 出现；YAML 解析失败以 ERROR 出现）
INFO md_server:   log_level changed: error -> info (applied)           <-- 切换后，在新级别 info 下输出
INFO md_server: config reloaded and applied successfully
```

198 的真实组合（YAML `info` + `RUST_LOG=warn`，`s9f`）：

```
WARN md_server: config log_level "info" is overridden by RUST_LOG=warn: RUST_LOG takes precedence; unset RUST_LOG (and restart) to let log_level and SIGHUP control the log level
（之后 SIGHUP 改 log_level：）
WARN md_server:   log_level changed: info -> debug (NOT applied, requires restart; RUST_LOG=warn takes precedence over log_level; unset RUST_LOG and restart to let log_level (and SIGHUP) control the log level)
WARN md_server: some config changes require connector restart -- please restart the service (1 pending: log_level)
```

改动文件：`crates/md-server/src/reload.rs`（新）、`logging.rs`（新）、`main.rs`、`crates/md-config/src/lib.rs`（`log_level` 校验）、
`crates/md-server/Cargo.toml` + `Cargo.lock`（仅 dev-dependencies：`async-trait`、`futures-util`、`tokio-tungstenite`，均为工作区已有版本）、
文档（`CHANGELOG.md`、`OPS_GUIDE.md`、`README.md`、`CONTEXT.md`、`DEPLOYMENT.md`、`config.yaml` 示例注释）、`scripts/compat/{fake_upstream,sighup_reload_e2e}.mjs`。
**行情数据路径、`md-connector`、配置文件格式均未改动。**

## 4. 验收对照

| 验收要求 | 结果 | 证据 |
|---|---|---|
| (a) 仅改 Business 地址，SIGHUP 明确提示需要重启 | ✅ | `s1-url-okxBusiness`；单元测试 `okx_business_url_only_change_requires_restart_and_is_not_success`；`s10`（`RUST_LOG=warn` 下也可见） |
| (b) 改任一地址后连发两次 SIGHUP，均不误报已应用 | ✅ 4 个地址 × 2 次 | `s1-url-{okxPublic,okxBusiness,binanceMarket,binancePublic}`；单元测试 `every_url_stays_pending_across_consecutive_reloads_and_clears_on_revert`（3 次 + 改回原值消失） |
| (c) 重启后连接器实际取得新地址 | ✅ | `s3-restart-picks-new-urls`（4 类连接都连新地址、不再连旧地址）；Rust 测试 `start_connectors_connects_each_connector_to_its_configured_url`（地址↔连接器对应关系，按订阅帧内容核对） |
| (d) 订阅列表热更新不受影响 | ✅ | `s4`：OKX 逐笔增 ETH / 删 BTC、Binance 增 ETH、OKX K 线增 ETH 的订阅帧均发出，第二次 SIGHUP 不重复下发，且与待重启地址并存；单元测试 `subscription_hot_update_*` |
| (e) YAML 读取 / 解析失败，保留原运行配置并明确报错 | ✅ | `s5`：语法错误、校验失败、文件不存在三种，ERROR、进程不退出、无任何订阅动作；修好后订阅热更恢复；单元测试 `load_failure_keeps_running_config_and_touches_nothing` |
| (f) YAML `log_level` 在**启动时**生效 | ✅ | `s9b`：`warn` 时启动阶段没有任何 INFO 行（旧版本照常输出全部 INFO），连接器照常连上；`s9e`：环境变量 `LOG_LEVEL` 覆盖同样生效 |
| (g) SIGHUP **热更** `log_level`，且不重连 | ✅ | `s9a`：info→warn→error→info，每一级都证明"该消失的消失、该保留的保留"（INFO 消失而 WARN 仍在；WARN 消失而 ERROR 仍在），全程 0 次重连 / 断线；单元测试用真实 subscriber 证明热切换改变了哪些事件能通过 |
| (h) 与 `RUST_LOG` 冲突时**不装作生效** | ✅ | `s9c`（连发两次，每次都提示 NOT applied，不报成功；改回后提示消失）、`s9f`（198 的真实组合）；单元测试 `rust_log_overrides_and_is_never_touched_by_apply` 等 |
| (i) 非法 `log_level` 不被悄悄忽略 | ✅ | `s9d`：启动 → 进程自己以退出码 1 退出并点名错误值；SIGHUP → ERROR、保留运行配置、运行中的级别不变、修好后恢复；`md-config` 单元测试 |

## 5. 验证数据

端到端脚本现为 19 个场景 / 129 条断言（原 14 个 / 92 条，新增 `s9a`–`s9f` 取代旧 `s9`）。

| 项目 | 修复前 | 修复后 |
|---|---|---|
| 端到端：线上 1.1.0 二进制，Docker | 70 通过 / 59 失败 | — |
| 端到端：HEAD `eb78524`，原生 | 70 通过 / 59 失败（失败项与上行完全相同） | — |
| 端到端：修复后，原生 debug | — | **129 / 129** |
| 端到端：修复后，静态 musl 二进制，Docker（线上同款） | — | **129 / 129** |
| `md-config` 单元测试 | 10 | 14 |
| `md-server` 单元测试 | 5 | 42 |
| `cargo test --workspace` | — | 248 通过，0 失败（`verify/test.log`） |
| `cargo fmt --all -- --check` | — | 无输出 |
| `cargo clippy --workspace --all-targets` | 13 条警告 | 13 条（全在 md-gateway / md-connector，`md-config`、`md-server` 为 0） |
| `--features md-server/jemalloc-stats` 编译 | — | 通过（`cargo check`） |

**变异验证**（`verify/mutation-tests.md`，由 `verify/run_mutations.py` 生成）：每次只把修复改坏一处，随后立即还原；共 **21 种**改坏，全部能编译、**全部被抓住**。

| 类别 | 改坏的方式（节选） | 抓住它的 |
|---|---|---|
| SIGHUP（M1–M6） | 把文件配置整体记为已应用（原 `current_cfg = new_cfg`）；漏掉 OKX Business 检测；连接器缺失 / 下发失败仍视为已应用；订阅热更后不推进已应用列表；启动时把 Business 连接器接到 Public 地址 | 单元测试（M1 7 个、M2 6 个、M3 2、M4 1、M5 3、M6 1） |
| `log_level` 热更逻辑（L1–L6） | `RUST_LOG` 覆盖时仍记为已应用；不真正切换却记为已应用；切换失败仍记为已应用；没变也每次切换；不检查 `RUST_LOG`；不做规范化 | 单元测试（每种至少 1 个失败） |
| 启动方案与过滤器句柄（L7–L11） | 启动时忽略 `RUST_LOG`；忽略 YAML 的 `log_level`；冲突时不再告警；热更不检查 `RUST_LOG` 覆盖；热更只返回成功不重载 | 单元测试 |
| 配置校验（L12–L13） | 非法 `log_level` 不再被拒绝；任何拼写都当合法 | `md-config` 单元测试 |
| 启动接线（B1–B2） | `logging::init` 不使用启动方案；`main` 传错级别 | **单元测试抓不住**（全局 subscriber 只能装一次，单元测试仍 42/42 通过）；**端到端抓住**（分别 7 条、6 条断言变红） |

还原后源文件与备份逐字节一致，且 `git status` 干净。

**脚本自身的问题（如实）**：
- 端到端脚本调试中发现并修掉一个时序竞态（Docker 日志流延迟导致启动帧 / 心跳帧落进检查窗口），与被测程序无关。
- 在旧二进制上跑新脚本时发现：`s9d` 的"启动时以非 0 退出"检查会**空过**——旧版本不校验 `log_level`，会一直运行到被超时杀掉，而被杀时脚本已关闭它的输出管道，
  进程关停时写日志失败，以 Rust panic 的退出码 101 退出（实测：关闭管道后发 SIGTERM → 101；管道保持打开时发 SIGTERM → 0），于是"退出码非 0"成立。
  已收紧为"进程自己很快退出且退出码为 1，而不是跑到超时被杀"；`evidence/` 里的结果均为收紧后的最终版本。

## 6. 兼容性与风险

- **日志文案有变化**（日志没有外部消费方，一切走 API，所以不当作兼容性问题；仍列出以便对照）：
  需重启项改为 `<YAML 路径> changed: <旧值> -> <文件值> (NOT applied, requires restart)`；启动时的 `log_level: <x>` 行改为 `log filter: <x> (from …)`；
  `loading config from` 改为 `loaded config from`；读取失败从 WARN 变 ERROR；`log_level` 变更成功时输出 `… (applied)`（这次是真的生效）。保留的关键串不变。
- **线上 198 换版后的日志级别**：YAML `info` + 容器 `RUST_LOG=warn` → 实际**仍是 warn**（与升级前行为一致），启动时多一条 WARN 说明。
  想用 YAML + SIGHUP 控制日志级别：换版时去掉 `RUST_LOG` 环境变量，并把 YAML 改成 `log_level: "warn"`（要和现状一致）。
  **不要只去掉 `RUST_LOG` 而不改 YAML**——YAML 现在写的是 `info`，日志量会比现状大。
- **校验更严格**：`log_level` 写成 `trace / debug / info / warn / error` 以外的值，启动会失败。现网 YAML（198 的两份、仓库示例、deploy.sh）写的都是 `info` 或 `warn`，不受影响。
- **`debug` / `trace` 很吵**：连接器对收到的每条 WebSocket 消息写一行 debug 日志。现在 YAML + SIGHUP 可以随时切到 `debug`，运维文档已写明"临时排查、查完改回"。
- **更多情形会报"需要重启"**：这是修复本身（以前误报成功的）。无变更的 SIGHUP 仍是成功。
- **订阅热更**：行为不变；同一批次内下发顺序改为按符号排序（之前来自哈希集合，顺序不定）。
- 线上 `RUST_LOG=warn` 下，"成功"仍是静默的（INFO）；**只要出现 WARN / ERROR 就代表有事要处理**，已写入运维文档。
- 地址类变更仍**必须重启**（按设计）。
- 性能：没有做专门测量。被过滤掉的日志在 `tracing` 里按 callsite 缓存兴趣，几乎无开销；`reload` 层只多一次读锁。数据路径（tick / kline / quote 输出）代码未改动。

## 7. 没做 / 需要决定

- **`RUST_LOG` 与 YAML 的优先级**：我按"环境变量优先"做了（理由见 §3.2）。如果你希望"YAML 优先"，不是改一行的事：
  `md-config` 的 `log_level` 带默认值 `info`，必须先区分"YAML 里显式写了"和"用了默认值"，否则 `RUST_LOG` 会永远被默认值盖住。需要的话再做。
- 没有加"待重启项数量"的 Prometheus 指标（想法，未做，超出本次范围）。
- **合并状态 / 版本归属**：已按指示在本地用合并提交（`--no-ff`）并入 `fix/gateway-unsubscribe-leak-v1.1.1`（合并提交 `c61fbbb`；
  合并后该分支的树与 `fix/sighup-config-reload` 完全一致，并复跑了 md-config 14 / md-server 42 个测试）。
  **尚未推送**：origin 上该分支仍是 `eb78524`，PR #2 暂无变化。**未打标签、版本号未改、未换版。**
  PR #2 也改过 `crates/md-server`，我在上一轮试过把 SIGHUP 的代码提交单独摘到 `feat/quote-topic-v1.1.0`（1.1.0 线）：只有 `crates/md-server/Cargo.toml` 一处平凡冲突（`[dev-dependencies]` 紧挨着 PR #2 新增的 jemalloc 依赖）；
  这一轮新增的 `log_level` 提交没有重新试摘。是否作为 1.1.1 一并发布、还是单独发 1.1.2、何时换版，由主审决定。
- **换版**：未做。产物（`dist/` 不入库）：`md-server-sighup-loglevel-fix-linux-x86_64`，SHA-256 与可复现构建方法见 `ARTIFACTS.txt`。
  198 换版清单新增一条：是否去掉 `RUST_LOG=warn`、YAML 是否改 `log_level: "warn"`（见 §6）。

## 8. 复现

```bash
cargo test -p md-config -p md-server                      # 14 + 42 个单元 / 接线测试
cargo build -p md-server
node scripts/compat/sighup_reload_e2e.mjs --runner native --bin target/debug/md-server --out /tmp/out
# 只跑 log_level 的场景
node scripts/compat/sighup_reload_e2e.mjs --runner native --bin target/debug/md-server --out /tmp/out --only s9a-log-level-hot-apply,s9b-log-level-from-config-at-startup,s9c-rust-log-takes-precedence,s9d-invalid-log-level-rejected,s9e-log-level-env-override,s9f-prod-198-combination

# 线上同款（静态二进制 + Docker）；可复现构建见 ARTIFACTS.txt
SOURCE_DATE_EPOCH=<提交时间> cargo zigbuild -p md-server --release --target x86_64-unknown-linux-musl --features md-connector/vendored-tls
# 用 FROM scratch 的探针镜像包一层（COPY 二进制 → /md-server，ENTRYPOINT ["/md-server"]），然后：
node scripts/compat/sighup_reload_e2e.mjs --runner docker --image <镜像> --out /tmp/out

# 变异验证（需要源文件没有未提交修改；结束时自动还原并核对）
python3 evidence/sighup-reload-fix/verify/run_mutations.py /tmp/out
```

目录：`before-head-eb78524-native/`、`before-prod-1.1.0-docker/`、`after-fixed-native/`、`after-fixed-musl-docker/`（各含 `results.md` / `results.json` 与每个场景的
`md-server.log`、`upstream-events.jsonl`、每步配置 `config.step*.yaml`）、`verify/`（fmt / clippy / test 原始输出、变异验证脚本与记录）、`ARTIFACTS.txt`。
