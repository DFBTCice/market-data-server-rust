# 1.2.0 证据索引：运行期订阅闲置退订与按需补订

任务书：1stbotv3 主审 2026-11-06「闲置自动退订与按需补订」；第二轮：主审复核意见（2026-11-06，补改、金丝雀与换版）。
当前报告：[`REPORT.md`](REPORT.md)（第二轮）；第一轮报告：[`REPORT-r1-889bca8.md`](REPORT-r1-889bca8.md)。
分支 `feat/runtime-sub-idle-v1.2.0`（从 `origin/main` `d2f2cb6` 新建）。源码提交：第一轮 `6b3949e`，第二轮 `42bc897`（`on_demand_quote`、示例配置 443）；
之后的提交只有文档与证据（`git diff 42bc897 HEAD -- crates Cargo.toml Cargo.lock config.yaml` 为空）。

## 第二轮（当前，源码 `42bc897`，产物提交 `12192d9`）

| 目录 / 文件 | 内容 | 复跑命令 |
|---|---|---|
| `gates/12192d9/` | 门禁（干净 worktree）：fmt / clippy / 全量测试 / md-tests / compat_1_0_0 / 集成测试 / compare selftest | `bash evidence/1.2.0/tools/verify_gates.sh 12192d9` |
| `positive-controls/` | 正对照 16 种（第一轮 13 种 + 报价补订 3 种），全部被抓住；原样代码 290 个全过 | `bash evidence/1.2.0/tools/run_mutations.sh 42bc897` |
| `build-logs/` | 最终产物：`12192d9` 干净 worktree、固定 `SOURCE_DATE_EPOCH`、jemalloc 特性、两次从零构建 sha256 一致（**sha 只能在同一路径 `/tmp/md-final-12192d9[-target]` 下复现**） | `bash evidence/1.2.0/tools/build_final.sh 12192d9` |
| `compat-local-switch-off/` | 开关关闭与 v1.1.1 同窗口逐字节对比（本机两个原生构建，真实交易所，198 的订阅，240 秒）；原始抓取打包 `captures.tar.gz`，逐文件 sha 见 `raw-sha256.txt` | `bash evidence/1.2.0/tools/local_compat.sh 240 12192d9`；只重跑对比：解包后 `COMPARE_ONLY=1 bash evidence/1.2.0/tools/local_compat.sh 240 HEAD "$PWD/evidence/1.2.0/compat-local-switch-off"` |
| `resource-local-switch-off/` | 开关关闭时资源与 v1.1.1 同量级（ps / lsof 采样 180 秒） | `bash evidence/1.2.0/tools/local_resource_ab.sh 180` |
| `canary-local-switch-on-short/` | 开关打开试跑（本机，闲置 2 分钟、报价补订关闭）：OMS / 1stbotv3（先退 WS 再 DELETE）/ 报价三类消费者；SIGHUP 不重启开关 | `MODE=short bash evidence/1.2.0/tools/local_canary.sh 12192d9` |
| `canary-local-switch-on-prod/` | 现网参数复跑（本机，`nas-198/config.release.yaml`：闲置 10、保护 10、检查 60 秒），同三类消费者 | `MODE=prod bash evidence/1.2.0/tools/local_canary.sh 12192d9` |
| `nas-198/config.release.yaml`、`config.release.diff` | 换版镜像配置（现网配置 + 开关打开、报价补订关闭）与相对现网配置的 diff | — |
| `nas-198/SWAP_PLAN_1.2.0.md` | 换版计划（前置条件、步骤、两级回退、24 小时采样） | — |
| `nas-198/canary/`（未做） | 198 旁路金丝雀两部分。需要业务方提供 Portainer 会话；脚本就绪 | `python3 evidence/1.2.0/tools/canary_198.py dist/md-server-1.2.0-12192d9-linux-x86_64 <sha256>` |
| `tools/` | 上述脚本，另有 `build_release_image.py`、`execute_swap.py`、`check_after_swap.py`、`observe_24h.sh`（换版用，须业务方批准） | — |

## 不随轮次变化

| 目录 | 内容 |
|---|---|
| `preflight/` | 开工前三方核对（`bash evidence/1.2.0/tools/preflight.sh`） |
| `upstream-probe/` | 交易所对无效订阅的真实应答（OKX 60018 不带 `arg`；币安一律 `result:null`） |
| `gates/baseline-d2f2cb6/` | 开工前基线：fmt 无输出、clippy 13 条、测试 248 个 |

## 第一轮（源码 `6b3949e`，产物 `f6ab760c…`，已被第二轮产物取代）

`gates/r1-f1d3c8a/`、`r1-canary-local-switch-on/`、`r1-resource-local-switch-off/`；第一轮的同窗口对比与构建日志在 git 历史 `889bca8` 中。
