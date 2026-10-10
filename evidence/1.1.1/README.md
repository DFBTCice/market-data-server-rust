# evidence/1.1.1 —— 1.1.0 现网内存增长调查 + Gateway 退订泄漏修复（1.1.1）证据索引

先看：[`INVESTIGATION_REPORT.md`](INVESTIGATION_REPORT.md)（调查报告：时间线、D1–D7 结论、增长构成、"5×"解释、准入条件自检、没做/没证明）。
换版计划（仅计划，未执行）：[`nas-198/SWAP_PLAN_1.1.1.md`](nas-198/SWAP_PLAN_1.1.1.md)。

## 目录

| 路径 | 内容 | 对应任务书 |
|---|---|---|
| `INVESTIGATION_REPORT.md` | 调查报告（含逐条 D1–D7 结论与证据路径） | §三、§六(2) |
| `report/d3-table.md` | c0–c17 + 现网的汇总表（版本/分配器/负载/RSS/斜率） | D3 |
| `report/d3-analysis.txt`、`d3-overview.png` | 每个对照容器的斜率、台阶、jemalloc 统计；全部容器 RSS 总览图 | D1、D3 |
| `report/d4-prod-rss.png` | 现网逐分钟 RSS + 事件线（RECONNECT / TICKS_DROPPED / WS_CLIENTS / SUB+）+ 吞吐 | D4、§六(2) 带注释的图 |
| `report/d6-d7.txt` | D6（重连后 tokio task 回基线）、D7（OKX 重连频率统计） | D6、D7 |
| `report/quote-byte-noise.txt` | compare.mjs 对 `quote.binance` 逐字节项的噪声量化（含 1.1.0 对 1.1.0 对照） | §二 A2、报告 §12.9 |
| `d3-load/` | c0–c17 每分钟 CSV、每小时 `/metrics` 快照、撤掉消费者后 idle 采样、最终 metrics、停读注入日志、事件日志 | D1、D3 |
| `d4-prod-readonly/` | 现网 `md-server` 逐分钟只读采样 `prod.csv`（07:07Z–17:29Z，623 条）、事件日志、小时快照、cgroup/THP 只读采样 | D4 |
| `d2-retention-sizing.txt` | 固定滞留（broadcast 环）实测与 39 主题上界估算 | D2 |
| `a4-tests/` | A4 集成测试：修复后 6/6 通过；在"只回退修复"的代码上 3/3 失败（正对照） | A4 |
| `a5-write-timeout/` | A5 写超时测试：修复后通过；去掉超时实现的变异版上失败（正对照） | A5 |
| `probe/` | 退订泄漏探针：现网 1.1.0 与旁路 1.1.0 原版出现重复推送，1.1.1 无 | A1 |
| `build-logs/` | `cargo fmt --check` / `clippy` / `test --workspace` / `compat_1_0_0` / `md-tests` 原始输出；musl 构建日志；`dist-sha256.txt` | §六(1) |
| `compat-live/` | 早期同窗口对比（1.1.0 vs 1.1.1 初版）；`quote-same-version-control/` = 1.1.0 对 1.1.0 的对照 | A2 |
| `compat-final/` | 同窗口对比：1.1.0 vs v4（A1+A2+A5，默认分配器），全 PASS | A2 |
| `compat-final-attempt1-topics-not-subscribed/` | 首次尝试失败存档（两侧默认只有 10 条订阅，部分主题"空对空"） | A2 |
| `compat-final-v5-attempt1-quote-key-collision/` | v5（推荐候选）第 1 次：`quote.binance` 逐字节项 2/1204 FAIL，附 `NOTE.md` 分析 | A2 |
| `compat-final-v5/` | v5（推荐候选）第 2 次，同一未改动的工具：`quote.binance` 逐字节项 1/1171 FAIL，其余全 PASS | A2 |
| `compat-control-110-vs-110/` | 对照：两个 1.1.0 容器之间同窗口比较，本次全 PASS | A2 |
| `nas-198/SWAP_PLAN_1.1.1.md` | 1.1.1 NAS 换版计划（分阶段、回滚、换后补订阅、24h 观察清单） | §六(3) |
| `tools/` | 旁路实验工具与脚本：`pt_exp.py`（只动 `purpose=md-mem-1.1.1` 容器与 `md-server:exp-*` 镜像）、各组启动脚本、`analyze_*.py`、`final_tables.py`、`quote_noise.py`、`compat_final.sh`、`build_final.sh`、`thp_sampler.py` | 复现 |

## 入库范围

为控制仓库体积，**compat-* 目录下的 WebSocket 原始抓取（`old-/new-gateway/`、`old-/new-legacy/`，约 84 MB，`*.raw.jsonl` / `*.ts`）没有提交**，只留在评审人的工作区；
对比结论与不一致样本的原文在各 `compare-*.log` 里，`report/quote-byte-noise.txt` 是用这些原始抓取跑 `tools/quote_noise.py` 的输出。其余证据（CSV、快照、日志、脚本、报告）均已入库。

## 关键数字（出处在报告表格里）

- 根因：musl 分配器高水位（存活堆 `allocated` 持平 6.6–10 MiB，RSS 只升不降）；musl 的 14 个对照容器全部上涨，jemalloc 的 4 个容器 1 小时预热后斜率 ≤ +0.85 MiB/h；推荐候选 A（A1+A2+A5+jemalloc）c17 在 6.12 小时、含 18 次停读注入和两次行情突发的负载下斜率 **+0.08 MiB/h**；默认分配器候选（c15）+16.74 MiB/h，**不达标**。
- 正对照：c9（1.0.0 原版）+2.39、c4（1.1.0 网关逻辑）+2.56、c12 +12.03、c11 +20.68 MiB/h。
- 现网 1.1.0：258.3 → 314.0 MiB（10.37 小时，+5.17 MiB/h，台阶式）；增长触发事件未能证明（报告 §7、§12.1）。

## 复现

见报告 §13。旁路容器全部已删除；保留镜像 `md-server:exp-v4`（候选 B 二进制）、`md-server:exp-v5`（候选 A 二进制）供复核，二进制 sha256 见 `build-logs/dist-sha256.txt`。
**本目录不含任何账号、密码或 token**（Portainer 会话 JWT 只放在 `/tmp/pt_jwt`，权限 600，不入库）。
