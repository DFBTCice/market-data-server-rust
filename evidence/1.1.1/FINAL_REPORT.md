# 1.1.1 总结报告（交主审审核）

任务：1.1.0 现网内存增长调查 + Gateway 退订泄漏修复（1.1.1），阶段 3 的前置条件。
提交人：行情负责人 AI（模型：Claude Sonnet 5.5）　日期：2026-10-11

| 项 | 位置 |
|---|---|
| PR | https://github.com/DFBTCice/market-data-server-rust/pull/2 （base = `feat/quote-topic-v1.1.0`，head = `fix/gateway-unsubscribe-leak-v1.1.1`，最新提交 `5fb47c5` 起，见 `git log`） |
| 调查报告（时间线、D1–D7、增长构成、"5×"、准入自检、没做/没证明） | `evidence/1.1.1/INVESTIGATION_REPORT.md` |
| 证据索引 | `evidence/1.1.1/README.md` |
| 换版计划（仅计划，未执行） | `evidence/1.1.1/nas-198/SWAP_PLAN_1.1.1.md` |
| 原始构建/测试输出 | `evidence/1.1.1/build-logs/` |
| 汇总表 / 图 | `evidence/1.1.1/report/`（`d3-table.md`、`d3-overview.png`、`d4-prod-rss.png`、`quote-byte-noise.txt`） |
| 文档 | `CHANGELOG.md`（1.1.1）、`OPS_GUIDE.md`（RSS ≥400 MiB 告警、退订/写超时语义）、`DEPLOYMENT.md`（§12.2 换版/回滚、`md-server:latest` 为何仍指向 1.0.0） |

**未打任何标签；现网 198 全程只读；换版未执行。**

---

## 一、做了什么

### 代码（`crates/md-gateway`、`md-server`、`md-connector`、`md-processor`）

| 项 | 内容 |
|---|---|
| A1 | Gateway 退订释放 broadcast 接收端（每连接 `StreamMap<主题, 流>`），同主题重复订阅幂等。修复前"订阅→退订→再订阅"会重复推送 N 份且接收端不释放；现网 1.1.0 已用探针确认（`probe/prod-1.1.0-before.json`）。 |
| A2 | 接收路径等价改写，输出字节不变。 |
| A3 | `md_ws_subscriptions_active`、`md_tokio_*`、限频的丢弃日志（1 秒一条、被抑制的条数带到下一条）、调试环境变量（强制重连）。 |
| A4 | 退订/重复推送集成测试（`crates/md-gateway/tests/unsubscribe_gateway.rs`、`ws_subscriptions_gauge.rs`），附正对照。 |
| A5 | `api_gateway.ws_write_wait`（默认 10s）此前被解析但从未生效，现在对 Gateway 连接生效；停读客户端被断开（`md_ws_kicked_write_timeout_total`）。**这是唯一的行为变化**，`"0s"` 关闭。 |
| 可选构建 | `--features md-server/jemalloc-stats`：jemalloc 作为全局分配器，并暴露 `md_jemalloc_*` 指标。默认构建仍是 musl 分配器，与 1.1.0 相同。 |

未改动：Legacy `/ws`、tick/kline/quote 语义与字节、proto、REST、配置（无新键）。

### 调查（D1–D7，证据路径见调查报告各节）

| 项 | 结论 | 证据 |
|---|---|---|
| D1 逻辑泄漏 vs 分配器 | **不是逻辑泄漏**。jemalloc 容器里存活堆（`allocated`）稳定在 6.6–10 MiB；14 个 musl 容器全部上涨（起止 +4 ~ +138 MiB），无消费者时也缓慢上涨。主因是 musl 分配器高水位（RSS 只升不降）。 | `d3-load/`、`report/d3-analysis.txt` |
| D2 固定滞留 | broadcast 环满时约 1.8–2.4 MiB/主题；现网 39 主题上界约 88 MiB，需消费者长时间停读才出现。A5 把滞留限制在 `ws_write_wait` 内。 | `d2-retention-sizing.txt` |
| D3 旁路等价负载 ≥6h + 正对照 | 见下表。 | `d3-load/`、`report/d3-table.md` |
| D4 现网只读逐分钟采样 | 258.3 → 314.0 MiB（07:07Z–17:29Z，10.37 小时，+5.17 MiB/h，台阶式）。**触发事件没能确定。** | `d4-prod-readonly/prod.csv`、`report/d4-prod-rss.png` |
| D5 `md_ticks_dropped`=493 | 行情毫秒级突发打满入口队列（容量 1000）；同一时刻（14:27:57Z）现网和多个旁路容器同时丢弃，来源在上游。 | 报告 §8 |
| D6 重连后孤儿 task | 没有；tokio 存活 task 数回到基线。 | `report/d6-d7.txt` |
| D7 OKX 频繁重连 | 旁路 0.20–0.32 次/连接·小时，现网 0.16；ping 15s 与 25s 一样，且在相互独立的容器间同时发生，属网络/OKX 侧。 | `report/d6-d7.txt` |

增长构成（报告 §5）：基线约 8%；环滞留 ≤11%（实测）/≤28%（上界）；A1 泄漏 ≲5%（约 3.4 KiB/接收端）；分配器高水位 ≳75%。
1.1.0 比 1.0.0 快约 5 倍（报告 §6）：同负载下两个版本增长一样（c9 1.0.0 +2.39，c4 1.1.0 网关逻辑 +2.56 MiB/h）；现网负载本身变了（K 线吞吐约 ×5.4，WS 实际投递 0.19→35.2 条/秒）。

---

## 二、怎么验证的

### 构建与测试（原始输出在 `build-logs/`）

| 项 | 结果 | 文件 |
|---|---|---|
| `cargo fmt --all -- --check` | 无输出，exit 0 | `cargo-fmt-check.log` |
| `cargo clippy --workspace --all-targets` | 13 行 warning（同口径基线 18，`rg -c '^warning: '`），exit 0 | `cargo-clippy.log` |
| `cargo test --workspace` | 207 通过 / 0 失败，exit 0 | `cargo-test-workspace.log` |
| `compat_1_0_0` | 5/5 | `cargo-test-compat_1_0_0-nocapture.log` |
| `md-tests` | 11/11 | `cargo-test-md-tests-nocapture.log` |
| A4（含正对照） | 修复后 6/6；只回退修复的代码上 3/3 失败 | `a4-tests/` |
| A5（含正对照） | 修复后通过；去掉超时实现的变异版上"被踢"用例失败 | `a5-write-timeout/` |

### 旁路等价负载（39 订阅、2 核，限额 256 MiB，1 小时预热后的斜率）

| 容器 | 配置 | 时长 h | 斜率 MiB/h | 角色 |
|---|---|---|---|---|
| **c17** | **候选 A：A1+A2+A5 + jemalloc**，每 20 分钟停读 60 秒 | 6.12 | **+0.08** | 推荐候选，达标 |
| c15 | 候选 B：A1+A2+A5，默认分配器，同负载 | 6.50 | +16.74 | **不达标** |
| c16 | A1 版（无 A5），默认分配器，同负载 | 6.50 | +7.59 | 对照（n=1，与 c15 的大小关系有随机性） |
| c9 | 1.0.0 原版二进制 | 5.83 | +2.39 | 正对照 |
| c4 | 1.1.0 网关逻辑 | 6.95 | +2.56 | 正对照 |
| c12 | 1.1.0 网关逻辑（停读负载） | 4.00 | +12.03 | 正对照 |
| c11 | A1 版（停读负载） | 4.00 | +20.68 | 正对照 |
| c2/c3/c10 | jemalloc 版（不同负载） | 4.0–6.95 | +0.19 / +0.22 / +0.85 | jemalloc 4 个容器均 ≤1 MiB/h |

完整 c0–c17 表见 `report/d3-table.md`。结论的因果支撑：同样代码换 jemalloc 后 RSS 持平，换回 musl 则上涨；musl 在无消费者时也涨。

### 同窗口 `compare.mjs`（旧 = `md-server:1.1.0-quote-bnb`，新 = 候选 A，各 240 秒，Gateway + Legacy）

| 项 | 结果 |
|---|---|
| tick/kline（13 个主题）逐字节 | 全部一致 |
| Legacy 无 quote | PASS |
| 订阅集合 | 39/39 一致 |
| metrics | 只新增 11 条，删除 0、TYPE/HELP 变更 0 |
| selftest | 25/25 |
| **`quote.binance.BNBUSDT` 逐字节** | **偶发 FAIL，见下** |

`quote.binance.BNBUSDT` 逐字节项（原始输出：`compat-*/compare-push-gateway.log`；汇总：`report/quote-byte-noise.txt`）：

| 比较对象 | 次数 | 不同条数 / 对应条数 |
|---|---|---|
| 1.1.1 对 1.1.0 | 4 | 7/1241 FAIL、0/1200 PASS、2/1204 FAIL、1/1171 FAIL |
| **1.1.0 对 1.1.0（对照）** | 2 | 5/639 FAIL、0/1072 PASS |

- 不同的字段只有 `bid_qty` 或 `ask_qty`；价格、timestamp、字段名/顺序、外层结构、OKX quote 全部一致。
- 1.1.1 对 1.1.0 合计 10/4816（0.21%），1.1.0 对 1.1.0 合计 5/1711（0.29%），无法区分。
- 原因是推断：对应键是 `(E, bid, ask)`，不含数量，同一毫秒内数量变化的多条 bookTicker 无法唯一配对；叠加两条独立币安连接的事件序列差异和 100ms quote 节流窗口的相位差（节流自 1.1.0 就有，1.1.1 未改）。没有抓上游原始事件，无法逐条证实。
- **我没有修改 `compare.mjs`。** 失败的几次全部原样保留（`compat-final-v5-attempt1-quote-key-collision/` 附 `NOTE.md`）。

---

## 三、没做什么与原因

| 没做 / 没证明 | 原因 |
|---|---|
| **没有换版** | 任务书要求业务方明确批准、低峰、可回滚、换后补订阅；计划已写好，未执行。阶段 3 在 §四(3) 的 24 小时观察完成前继续暂停。 |
| 现网 RSS 台阶（16:17Z 单分钟 +23.9 MiB）的触发事件 | 现网只读，无法注入或装 jemalloc 统计。THP（`anon_thp`=30 MiB，`thp_collapse_alloc`=15，旁路均为 0）只是未验证的线索；后续只读采样 24 分钟没有新台阶。 |
| 旁路量级与现网 | 旁路 musl 最高 161 MiB，现网已 314 MiB；方向和机制复现，量级未完全复现。 |
| Legacy `/ws`、gRPC 流的停读滞留 | 与 Gateway 有同样的暴露面；任务书要求 Legacy 不变，A5 只覆盖 Gateway。 |
| 17 订阅 vs 39 订阅的受控实验 | 没做；"5×"只用同负载 1.0.0 vs 1.1.0 加现网计数器解释，置信度中。 |
| `tick_channel_buffer` 8192、`prctl(PR_SET_THP_DISABLE)` | 未实施、未验证，仅作建议。 |
| 修改 `compare.mjs` | 验收工具的口径应由主审决定。 |
| WS 原始抓取入库 | 约 84 MB（`compat-*/{old,new}-{gateway,legacy}/`），只留在工作区；对比结论在各 `compare-*.log`。 |
| `Dockerfile.bin`、`Dockerfile.prebuilt`、`monitoring/` | 与本任务无关，未入库。 |

收尾状态：旁路容器已全部删除；现网采样已停并冻结；过期实验镜像已删，NAS 上保留 `md-server:exp-v4`（候选 B 二进制）和 `md-server:exp-v5`（候选 A 二进制）；二进制 sha256 见 `build-logs/dist-sha256.txt`。

---

## 四、兼容性声明（§三 8 条逐条）

| # | 结论 | 依据 |
|---|---|---|
| 1 | tick/kline 去掉 `connector_receive_ts` 后同窗口逐字节一致（13 个主题）。**例外**：Binance quote 的 `bid_qty`/`ask_qty` 偶发差异，1.1.0 对 1.1.0 同样出现。 | `compat-final/`、`compat-final-v5/`、`report/quote-byte-noise.txt` |
| 2 | REST 不变：1.1.1 没改 REST 代码、proto 和配置；订阅集合同窗口 39/39 一致。**REST 响应体和错误文案这次没有重新抓取对比。** | `git diff beaac94..HEAD`、`compare-subs.log` |
| 3 | Legacy `/ws` 不含 quote：`noquote` 对比 PASS。 | `compare-noquote-legacy.log` |
| 4 | proto 没有任何改动。 | `git diff beaac94..HEAD -- proto crates/md-proto` 为空 |
| 5 | 配置无新增必填键，沿用 1.1.0 的 `config.yaml`。**启动日志本轮没有重新逐行对比。** | 代码 diff 无 `md-config` 改动 |
| 6 | `md-tests` 真实执行 11/11，0 SKIP。 | `build-logs/cargo-test-md-tests-nocapture.log` |
| 7 | `/metrics` 只增不减：新增 11 条（含 `md_ws_subscriptions_active`、`md_ws_kicked_write_timeout_total`、`md_tokio_*`、`md_jemalloc_*`），删除 0、类型变更 0。 | `compat-final-v5/compare-metrics.log` |
| 8 | 资源量级：旁路 ≥6 小时实测（候选 A 的 c17 +0.08 MiB/h）；现网未换版，没有现网数据。 | §二 |

---

## 五、需要对方做的事

1. **业务方**：批准一次低峰换版，并在候选 A（`md-server:1.1.1`，含 jemalloc，推荐）和候选 B（`md-server:1.1.1-default-alloc`，不满足斜率条件，只能配合 400 MiB 告警 + 定期低峰重建）之间选择。步骤、回滚、换后补订阅、24 小时观察清单见 `nas-198/SWAP_PLAN_1.1.1.md`。
2. **1stbotv3 / OMS**：确认停读 ≥10 秒的 Gateway 连接会被断开（A5），客户端需要自动重连并重订阅；如不接受，可把 `api_gateway.ws_write_wait` 设为 `"0s"`，但滞留保护随之失效。
3. **现网 RSS 约 314 MiB（限额 512 MiB）**：平均约 +5.2 MiB/h，按平均斜率约 17 小时后到 400 MiB（10 月 11 日 10:30Z 前后），台阶式增长不可预测。建议在 400 MiB 前换版，或至少低峰重建一次（同镜像同配置，重建后补订阅）。
4. **主审**：裁定 `quote.binance` 逐字节项的验收口径。建议口径：tick/kline 必须逐字节 PASS；quote.binance 要求字段名/顺序/外层结构 OK、不同条数 ≤1% 且只出现在数量字段；或在 `compare.mjs` 的 quote 对应键里加入 `bid_qty|ask_qty`。
5. **主审**：审核通过后告知是否合并 PR #2；合并、打标签、发布均由你们决定，我没有做。
