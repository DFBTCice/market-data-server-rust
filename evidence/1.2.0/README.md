# 1.2.0 证据索引：运行期订阅闲置退订与按需补订

任务书：1stbotv3 主审 2026-11-06「闲置自动退订与按需补订」（业务方转发）。报告：[`REPORT.md`](REPORT.md)。
分支 `feat/runtime-sub-idle-v1.2.0`（从 `origin/main` `d2f2cb6` 新建）。源码提交 `6b3949e`；之后的提交只有文档与证据
（`git diff 6b3949e HEAD -- crates Cargo.toml Cargo.lock` 为空）。

| 目录 / 文件 | 内容 | 复跑命令 |
|---|---|---|
| `preflight/` | 开工前核对（任务书第〇节）：分支 / HEAD / 标签 / 领先落后 / 未跟踪文件；`v1.1.1` 与 `origin/main` 源码树哈希；198 只读探测（健康、`process_start_time_seconds`、jemalloc 指标、13 条订阅） | `bash evidence/1.2.0/tools/preflight.sh` |
| `upstream-probe/` | 交易所对无效订阅的真实应答（OKX 60018 错误不带 `arg`；币安一律 `result:null`），决定了拒绝解析与币安级别白名单的设计 | `node evidence/1.2.0/upstream-probe/probe_upstream_reject.mjs` |
| `gates/baseline-d2f2cb6/` | 开工前基线：fmt 无输出、clippy 13 条、测试 248 个 | — |
| `gates/f1d3c8a/` | 交付门禁（干净 worktree）：fmt / clippy / 全量测试 / md-tests / compat_1_0_0 / 新集成测试 / compare selftest | `bash evidence/1.2.0/tools/verify_gates.sh f1d3c8a` |
| `positive-controls/` | 正对照：13 种故意改坏（有接收方也退订、配置订阅也退订、无视保护期、无视 REST 查询、无退避、无上限、无按需补订、无视开关、不等闲置、1.1.1 退订泄漏回归、gRPC 断开不释放、OKX 错误不解析、SIGHUP 不应用）全部被测试抓住；原样代码 286 个全过 | `bash evidence/1.2.0/tools/run_mutations.sh 6b3949e` |
| `compat-local-switch-off/` | 开关关闭与 v1.1.1 同窗口逐字节对比（本机两个原生构建同时运行，连真实交易所，灌 198 的 13 条订阅，抓 240 秒）。原始抓取（100 个文件、88 MB）打包为 `captures.tar.gz`（4.4 MB），逐文件 sha256 见 `raw-sha256.txt` | 重新抓取：`bash evidence/1.2.0/tools/local_compat.sh 240 6b3949e`；只重跑对比：`cd evidence/1.2.0/compat-local-switch-off && tar -xzf captures.tar.gz && shasum -a 256 -c raw-sha256.txt && cd - && COMPARE_ONLY=1 bash evidence/1.2.0/tools/local_compat.sh 240 HEAD "$PWD/evidence/1.2.0/compat-local-switch-off"` |
| `resource-local-switch-off/` | 开关关闭时资源与 v1.1.1 同量级（ps / lsof 采样 180 秒） | `bash evidence/1.2.0/tools/local_resource_ab.sh 180` |
| `canary-local-switch-on/` | 开关打开的金丝雀（本机，真实交易所，闲置 2 分钟）：OMS 类不退、botd 取消后闲置到期被退、OMS 订阅已退话题自动补订且不重复、从未登记话题按需补订、SIGHUP 不重启开关 | `bash evidence/1.2.0/tools/local_canary.sh 6b3949e` |
| `build-logs/` | 最终产物：`f1d3c8a` 干净 worktree、固定 `SOURCE_DATE_EPOCH`、jemalloc 特性、两次从零构建 sha256 一致 | `bash evidence/1.2.0/tools/build_final.sh f1d3c8a` |
| `nas-198/`（未做） | 198 旁路金丝雀（开关关闭对比 + 开关打开）。需要业务方提供 Portainer 会话；脚本已就绪 | `python3 evidence/1.2.0/tools/canary_198.py dist/md-server-1.2.0-f1d3c8a-linux-x86_64 <sha256>` |

新增测试：`crates/md-gateway/tests/runtime_subs_gateway.rs`（15 个集成测试，假上游 + 真实 `BaseConnector` + 真实网关），
`md-gateway::runtime_subs` 单元测试 6 个，`md-processor` 使用信号 4 个，`md-connector` OKX 错误解析 2 个，`md-grpc` 断开释放 1 个，
`md-config` 4 个，`md-server`（配置接线 3 个、SIGHUP 热更 3 个）。
