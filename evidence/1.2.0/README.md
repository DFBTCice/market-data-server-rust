# 1.2.0 证据索引：运行期订阅闲置退订与按需补订

任务书：1stbotv3 主审 2026-11-06「闲置自动退订与按需补订」；第二轮：主审复核意见（2026-11-06，补改、金丝雀与换版）；换版前修正：主审复核意见（2026-11-07）。
当前报告：[`nas-198/SWAP_FINAL_FIX_REPORT.md`](nas-198/SWAP_FINAL_FIX_REPORT.md)（换版前最后一项，工具 `a08f8f3`，最终版本演练）；
上一份：[`nas-198/SWAP_TOOLS_REPORT.md`](nas-198/SWAP_TOOLS_REPORT.md)（换版前修正，`67abacc`）；金丝雀：[`nas-198/CANARY_REPORT.md`](nas-198/CANARY_REPORT.md)；
第二轮：[`REPORT.md`](REPORT.md)；第一轮：[`REPORT-r1-889bca8.md`](REPORT-r1-889bca8.md)。
分支 `feat/runtime-sub-idle-v1.2.0`（从 `origin/main` `d2f2cb6` 新建）。源码提交：第一轮 `6b3949e`，第二轮 `42bc897`（`on_demand_quote`、示例配置 443）；
之后产品代码未改：`git diff 42bc897 HEAD -- crates Cargo.toml Cargo.lock config.yaml` 只有测试文件 `crates/md-gateway/tests/runtime_subs_gateway.rs`（`c790a68`，3 个测试）。

## 换版前最后一项（2026-11-07，工具 `a08f8f3`，产物仍为 `cbc6b053…`）

| 目录 / 文件 | 内容 | 复跑命令 |
|---|---|---|
| `nas-198/SWAP_FINAL_FIX_REPORT.md` | 五段式报告：10 分钟验收的 K 线待 botd 确认、回退失败分支加固、人工兜底、最终版本演练 | — |
| `nas-198/swap-tools-since-67abacc.diff` | 本轮工具 diff | `git diff 67abacc a08f8f3 -- evidence/1.2.0/tools` |
| `nas-198/drill/` | 最终版本演练：换版 3 次、一级回退 4 次、二级回退正常 / 重建 / 「1.1.1 起不来 → 放回 1.2.0」、清理与现网只读快照 | `python3 evidence/1.2.0/tools/drill_198.py`（需 `/tmp/pt_jwt`，工具须已提交） |
| `positive-controls/offline-tools/` | 离线测试的正对照 14 种 + 原样代码 | `bash evidence/1.2.0/tools/run_offline_mutations.sh` |

## 换版前修正（2026-11-07，`67abacc`）

| 目录 / 文件 | 内容 | 复跑命令 |
|---|---|---|
| `nas-198/SWAP_TOOLS_REPORT.md` | 五段式报告：6 项修正、演练结果、现网消费者变化、需要业务方做的事 | — |
| `nas-198/swap-tools-since-879cdd7.diff` | 本轮换版 / 回退工具的完整 diff | `git diff 879cdd7 67abacc -- evidence/1.2.0/tools` |
| `nas-198/drill-67abacc/` | 198 旁路演练：换版两次、一级回退 4 次、二级回退正常路径与「回退容器不存在 → 重建再切换」、清理与现网只读快照 | — |
| `nas-198/drill-attempt1-toggle-bug/` | 第一次演练现场（第 3 个开关因工具正则 bug 失败，已修复） | — |
| `positive-controls/swaplib/` | 离线测试的正对照（把 `swaplib.py` 改坏 3 种，测试都失败） | 见报告 §2.1 |
| `positive-controls/r4-c790a68/` | 新增正对照 M17 / M18 与原样全量 293 个 | `ONLY="M17\|M18" bash evidence/1.2.0/tools/run_mutations.sh c790a68 <输出目录>` |
| `tools/swaplib.py` 等 | `execute_swap.py`、`build_release_image.py`、`check_after_swap.py`、`runtime_toggle.py`、`rollback_to_111.py`、`drill_198.py`、`observe_24h.sh`、`analyze_24h.py` | 离线测试：`python3 evidence/1.2.0/tools/test_swaplib_offline.py` |

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
| `nas-198/CANARY_REPORT.md` | **198 旁路金丝雀报告**（2026-11-06 16:05–16:46Z，未换版） | — |
| `nas-198/canary/` | 第一次完整运行：第 1 部分字节对比（`gate-results.json`，原始抓取 `captures.tar.gz` + `raw-sha256.txt`）与第 2 部分 short / prod（判据误报，见报告）；docker stats、现网快照、容器日志、读回的二进制 sha 与配置 | `python3 evidence/1.2.0/tools/canary_198.py dist/md-server-1.2.0-12192d9-linux-x86_64 <sha256>` |
| `nas-198/canary-rerun/` | 第 2 部分判据修正后的重跑（short、prod 同时运行，均 `pass: true`） | `python3 evidence/1.2.0/tools/canary_198.py --switch-on-only dist/md-server-1.2.0-12192d9-linux-x86_64 <sha256>` |
| `nas-198/cleanup-check.txt` | 金丝雀结束后的 198 清理核对（旁路容器、镜像均无；现网未重启） | — |
| `tools/` | 上述脚本；换版 / 回退工具已在「换版前修正」中重写（见上节） | — |

## 不随轮次变化

| 目录 | 内容 |
|---|---|
| `preflight/` | 开工前三方核对（`bash evidence/1.2.0/tools/preflight.sh`） |
| `upstream-probe/` | 交易所对无效订阅的真实应答（OKX 60018 不带 `arg`；币安一律 `result:null`） |
| `gates/baseline-d2f2cb6/` | 开工前基线：fmt 无输出、clippy 13 条、测试 248 个 |

## 第一轮（源码 `6b3949e`，产物 `f6ab760c…`，已被第二轮产物取代）

`gates/r1-f1d3c8a/`、`r1-canary-local-switch-on/`、`r1-resource-local-switch-off/`；第一轮的同窗口对比与构建日志在 git 历史 `889bca8` 中。
