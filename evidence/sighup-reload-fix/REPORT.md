# SIGHUP 配置重载：调查与修复报告

> 分支 `fix/sighup-config-reload`（基于 `eb78524`，即 PR #2 分支 `fix/gateway-unsubscribe-leak-v1.1.1` 的 HEAD）。
> **只做了本地提交：未推送、未建 PR、未打标签、未改版本号；198 线上未做任何操作。**
> 修复后的静态二进制仅在本机 Docker 里验证过，**未上现网**。

## 1. 结论

1. 你给的三条判断**逐条复核，全部成立**；用线上同一个二进制（SHA-256 `d73f7a4c…ba60`）和 1.1.1 分支 HEAD 都复现了，两者行为完全一致。
2. **币安同样有问题**：两个地址（`binance.stream_base_url`、`binance.stream_base_url_public`）检测是齐的，没有"漏检"；
   但"基线被污染"（第二次 SIGHUP 误报成功）和"重连不换地址"与 OKX 完全相同。
3. 调查中还发现 6 项同类问题（见 §2.3），其中最实际的一条：**线上 `RUST_LOG=warn` 下，只改 OKX Business 地址再发 SIGHUP，日志里没有任何输出**——因为"成功"那行是 INFO，被过滤了，而"需要重启"的提示根本没有产生。
4. 已修复。保留"地址变更需要重启"的设计，没有做连接器热重建。验收 5 条全部通过（§4）。

## 2. 调查

### 2.1 方法

- 读代码：`main.rs` 的 `wait_for_signal()` / `apply_subscription_changes()`、`okx.rs`、`binance.rs`、`base.rs`、`md-config`。
- 对线上二进制做 `strings`：只有三条"`… changed (requires restart)`"文案（binance 两个、okx public 一个），**没有 business**。
- 写了端到端脚本（`scripts/compat/sighup_reload_e2e.mjs`）：**真实 md-server 进程 + 真实 SIGHUP + 假上游 WebSocket 服务**，
  每条断言都写成"正确行为应该是什么"，所以有缺陷的版本会变红。分别跑了：
  线上 1.1.0 二进制（Docker，`linux/amd64`）和 HEAD `eb78524`（本机原生）。两组都是 59/92 通过，**失败的 33 条完全相同**。

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
| `log_level` 被标注 `(applied)`，实际 YAML 的 `log_level` 从未驱动日志过滤器（由 `RUST_LOG` 决定） | s9 |
| 读取 / 解析失败只是 WARN | s5 |

> 198 线上情况：`evidence/1.1.0/nas-198/md-server.inspect.stage2.json` 记录的启动命令没有 `--port-offset`、没有环境变量覆盖，所以线上不会触发端口偏移那条误报；`RUST_LOG=warn` 则是确实存在的。

## 3. 修复

核心是 `crates/md-server/src/reload.rs`（新）：

1. **区分"文件中的新配置"和"已应用配置"。** `ReloadState` 只保存进程真正在用的配置。
   每次 SIGHUP 都拿文件和它比较；只有**热更成功的订阅列表**才会写进去，需重启的字段永远不写。
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

改动文件：`crates/md-server/src/reload.rs`（新）、`main.rs`（删掉旧 SIGHUP 代码；启动改用 `load_effective_config`；`wait_for_signal` 增加 `port_offset` 参数）、
`crates/md-server/Cargo.toml` + `Cargo.lock`（仅 dev-dependencies：`async-trait`、`futures-util`、`tokio-tungstenite`，均为工作区已有版本）、
文档（`CHANGELOG.md`、`OPS_GUIDE.md` 第一节、`README.md`、`CONTEXT.md`）、`scripts/compat/{fake_upstream,sighup_reload_e2e}.mjs`。
**行情数据路径、`md-connector`、`md-config`、配置文件格式均未改动。**

## 4. 验收对照

| 验收要求 | 结果 | 证据 |
|---|---|---|
| (a) 仅改 Business 地址，SIGHUP 明确提示需要重启 | ✅ | `s1-url-okxBusiness`；单元测试 `okx_business_url_only_change_requires_restart_and_is_not_success`；`s10`（`RUST_LOG=warn` 下也可见） |
| (b) 改任一地址后连发两次 SIGHUP，均不误报已应用 | ✅ 4 个地址 × 2 次 | `s1-url-{okxPublic,okxBusiness,binanceMarket,binancePublic}`；单元测试 `every_url_stays_pending_across_consecutive_reloads_and_clears_on_revert`（3 次 + 改回原值消失） |
| (c) 重启后连接器实际取得新地址 | ✅ | `s3-restart-picks-new-urls`（4 类连接都连新地址、不再连旧地址）；Rust 测试 `start_connectors_connects_each_connector_to_its_configured_url`（地址↔连接器对应关系，按订阅帧内容核对） |
| (d) 订阅列表热更新不受影响 | ✅ | `s4`：OKX 逐笔增 ETH / 删 BTC、Binance 增 ETH、OKX K 线增 ETH 的订阅帧均发出，第二次 SIGHUP 不重复下发，且与待重启地址并存；单元测试 `subscription_hot_update_*` |
| (e) YAML 读取 / 解析失败，保留原运行配置并明确报错 | ✅ | `s5`：语法错误、校验失败、文件不存在三种，ERROR、进程不退出、无任何订阅动作；修好后订阅热更恢复；单元测试 `load_failure_keeps_running_config_and_touches_nothing` |

## 5. 验证数据

| 项目 | 修复前 | 修复后 |
|---|---|---|
| 端到端（14 场景 / 92 断言）线上 1.1.0 二进制，Docker | 59 通过 / 33 失败 | — |
| 端到端 HEAD `eb78524`，原生 | 59 / 33（失败项与上行完全相同） | — |
| 端到端 修复后，原生 debug | — | **92 / 92** |
| 端到端 修复后，静态 musl 二进制，Docker（线上同款） | — | **92 / 92** |
| `md-server` 单元测试 | 5 | 21 |
| `cargo test --workspace` | — | 223 通过，0 失败 |
| `cargo fmt --all -- --check` | — | 无输出 |
| `cargo clippy --workspace --all-targets` | 13 条警告 | 13 条（全在 md-gateway / md-connector，`md-server` 为 0） |

**变异验证**（`verify/mutation-tests.md`）：每次只把修复改坏一处，跑 `cargo test -p md-server`，随后立即还原。6 种改坏全部能编译、全部被测试抓到：

| 改坏的方式 | 失败的测试数 |
|---|---|
| 把文件配置整体记为已应用（即原 `current_cfg = new_cfg;`） | 5 |
| 漏掉 OKX Business 地址检测 | 5 |
| 连接器缺失时仍视为已应用（静默吞掉） | 2 |
| 订阅下发失败仍视为已应用（报成功、不重试） | 1 |
| 订阅热更成功后不推进已应用列表（每次 SIGHUP 重复下发） | 3 |
| 启动接线：OKX Business 连接器被接到 Public 地址 | 1（接线测试） |

还原后源文件与备份逐字节一致。

端到端脚本调试中发现并修掉一个**脚本自身**的时序竞态（Docker 日志流延迟导致启动帧/心跳帧落进检查窗口），与被测程序无关；`evidence/` 里的结果均为加固后的最终版本。

## 6. 兼容性与风险

- **日志文案有变化**（目的就是让提示可靠）：需重启项改为 `<YAML 路径> changed: <旧值> -> <文件值> (NOT applied, requires restart)`；不再有 `log_level: … (applied)`；读取失败从 WARN 变 ERROR。
  依赖旧文案的告警 / 脚本需要对照 `OPS_GUIDE.md` 第一节调整；上述保留的关键串不变。
- **更多情形会报"需要重启"**：这是修复本身（以前误报成功的）。无变更的 SIGHUP 仍是成功。
- **订阅热更**：行为不变；同一批次内下发顺序改为按符号排序（之前来自哈希集合，顺序不定）。
- 线上 `RUST_LOG=warn` 下，"成功"仍是静默的（INFO）；现在**只要出现 WARN / ERROR 就代表有事要处理**，已写入运维文档。
- 地址类变更仍**必须重启**（按设计）。

## 7. 没做 / 需要决定

- **YAML `log_level` 仍不驱动日志过滤器**（既有行为，本次只是不再谎称"已应用"）。要不要让它真正生效是另一个决定。
- 没有加"待重启项数量"的 Prometheus 指标（想法，未做，超出本次范围）。
- **合并方式 / 版本归属**：本分支叠在 PR #2 之上。PR #2 也改过 `crates/md-server`，所以我实际试了把代码提交单独摘到 `feat/quote-topic-v1.1.0`（1.1.0 线）：
  只有 `crates/md-server/Cargo.toml` 一处平凡冲突（`[dev-dependencies]` 紧挨着 PR #2 新增的 jemalloc 依赖，保留我方的 dev-dependencies 即可）；
  解决后 `cargo test --locked -p md-server` 21 个全部通过（临时工作树，已删除；文档和脚本提交未试摘）。
  是否并入 1.1.1、单独发 1.1.2，由主审决定。
- **换版**：未做。产物（`dist/` 不入库）：`md-server-sighup-reload-fix-linux-x86_64`，SHA-256 见 `ARTIFACTS.txt`。

## 8. 复现

```bash
cargo test -p md-server                                   # 21 个单元 / 接线测试
cargo build -p md-server
node scripts/compat/sighup_reload_e2e.mjs --runner native --bin target/debug/md-server --out /tmp/out
# 线上同款（静态二进制 + Docker）
cargo zigbuild -p md-server --release --target x86_64-unknown-linux-musl --features md-connector/vendored-tls
# 用 FROM scratch 的探针镜像包一层（COPY 二进制 → /md-server，ENTRYPOINT ["/md-server"]），然后：
node scripts/compat/sighup_reload_e2e.mjs --runner docker --image <镜像> --out /tmp/out
```

目录：`before-head-eb78524-native/`、`before-prod-1.1.0-docker/`、`after-fixed-native/`、`after-fixed-musl-docker/`（各含 `results.md` / `results.json` 与每个场景的
`md-server.log`、`upstream-events.jsonl`、每步配置 `config.step*.yaml`）、`verify/`（fmt / clippy / test 原始输出、变异验证记录）、`ARTIFACTS.txt`。
