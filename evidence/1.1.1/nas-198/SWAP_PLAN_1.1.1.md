# NAS 198 换版计划：md-server 1.1.0（quote-bnb）→ 1.1.1

> **状态：仅计划，未执行。** 现网 198 在本次调查中全程只读（只读 `/metrics`、`/api/v1/subscriptions`、`/health`、
> `/ws/v1/data` 订阅探针、Portainer 容器只读统计）。换版需要**业务方明确批准一次**，并在**低峰**进行。
> 操作入口与 1.1.0 相同：Portainer（192.168.1.198:9000）的 Docker API，只动 endpoint 3（local = 198）；不碰 153（Go 版）、OMS、策略系统、1stbotv3。

## 0. 为什么要换、为什么要尽快

- 现网 `md-server`（1.1.0）RSS：07:07Z 258 MiB → 17:29Z 314 MiB（逐分钟只读采样 `evidence/1.1.1/d4-prod-readonly/prod.csv`，10.4 小时平均约 +5.2 MiB/h，呈"台阶"上涨且从不回落；16:17Z 一次 +23.9 MiB，此后约 1 小时持平）。
  容器内存限额 512 MiB（swap 共 1 GiB）。按平均斜率估算，约 **17 小时后（≈10 月 11 日 10:30Z = 北京时间 18:30）达到 400 MiB**，约 38 小时后触及 512 MiB；台阶式增长不可预测，请以实时 RSS 为准。
- **如果业务方暂时不批准换版**：至少在 RSS ≥ 400 MiB 之前，低峰期**重建一次容器**（同镜像同配置：`pt_swap.py swap md-server:1.1.0-quote-bnb md-server-1.1.0-quote-bnb-bak`，之后补订阅）清零高水位。这只是缓解，不是修复。
- 1.1.1 解决什么：A1 退订泄漏/重复推送、A5 停读客户端写超时（限制环滞留）、可观测性；**推荐候选再加 jemalloc 分配器**，因为旁路等价负载下只有它让 RSS 斜率达标（见调查报告 §D3、§准入条件）。

## 1. 候选与产物（需要业务方选择）

| 候选 | 内容 | 旁路 ≥6h 结论 | 建议 |
|------|------|---------------|------|
| **A（推荐）`md-server:1.1.1`** | A1+A2+A3+A5 + **jemalloc 分配器**（`--features md-connector/vendored-tls,md-server/jemalloc-stats`） | 6 小时含每 20 分钟 60 秒停读注入、含行情突发：RSS 斜率 ≈ 0（c17），存活堆 6.7 MiB 持平；4/4 个 jemalloc 容器均持平 | 业务方批准即换 |
| B（备选）`md-server:1.1.1-default-alloc` | 同上但**默认分配器**（musl 自带，与 1.1.0 一致） | 同样负载下 RSS 仍上涨（c15：见调查报告表，斜率远高于 1 MiB/h）；**不满足准入条件(2)** | 仅当业务方不愿改分配器；需配合 §6 的 400 MiB 告警 + 定期低峰重建 |

产物（构建命令、sha256 见 `evidence/1.1.1/build-logs/dist-sha256.txt`）：

- A：`dist/md-server-1.1.1-linux-x86_64` → 镜像 `md-server:1.1.1`
- B：`dist/md-server-1.1.1-default-alloc-linux-x86_64` → 镜像 `md-server:1.1.1-default-alloc`

镜像构建方式与 1.1.0 相同：`FROM md-server:1.1.0-quote-bnb`，**仅替换** `/usr/local/bin/md-server`（OS 层 / HEALTHCHECK / CMD / 配置全沿用）；
旁路用 `evidence/1.1.1/tools/pt_exp.py build`（仅允许 `md-server:exp-*` 标签），现网镜像标签 `md-server:1.1.1` 由同样的 Docker build API 调用生成（换版当天执行）。
**`md-server:latest` 不动**（仍是 1.0.0，理由见 DEPLOYMENT §12.2）。**配置文件不改**：现网配置里已有 `ws_write_wait: "10s"`，A5 随换版自动生效。

## 2. 换版前（T-1 天，不碰现网）

1. **知会消费者**（1stbotv3 / OMS 价格源）：1.1.1 起 Gateway 客户端若 10 秒写不动数据会被服务端断开。消费者必须**自动重连并重新订阅**（1.1.0 换版时 OMS 已验证能在约 10 秒内重连）。
   需要对方确认：①断线后会重连；②重连后会重发 `subscribe`；③不依赖"长时间停读但连接保持"。
2. **旁路金丝雀**（≤256 MiB、2 CPU、用完即删）：用 `md-server:1.1.1` 与 `md-server:1.1.0-quote-bnb` 各起一个，`restore_subs.mjs` 补入同一份现网订阅，
   `evidence/1.1.1/tools/compat_final.sh`（改 `NEW_IMG`）同窗口 240 秒对比：`compare.mjs push|subs|metrics|noquote`、`selftest` 均须 PASS。
   本次已对最终候选做过两次（见 `evidence/1.1.1/compat-final-v5-attempt1-quote-key-collision/` 与 `compat-final-v5/`）：除 `quote.binance.BNBUSDT` 的逐字节项外全部 PASS；
   该项因上游/节流噪声会偶发 FAIL（仅 bid_qty/ask_qty，1.1.0 对 1.1.0 同样出现，详见调查报告 §12.9），**验收口径建议**：tick/kline 必须逐字节 PASS；quote.binance 要求字段名/顺序/外层结构 OK、不同条数 ≤ 1% 且只出现在数量字段。
3. **留基线（只读）**：`docker inspect md-server`（保存到 `baseline/`）、`GET /api/v1/subscriptions`、`GET /metrics`、`GET /health`、60 秒 Gateway 抓取。

## 3. 换版（低峰窗口，业务方批准后；约 6~8 秒中断）

```bash
cd /Users/lihao/Desktop/web3/rust/rest重构
# 0) 登录 Portainer 取会话令牌（只保存 JWT 到 /tmp/pt_jwt，权限 600；不落任何密码）
# 1) 记录换版前订阅（含运行期动态订阅；换容器后会丢）
curl -s http://192.168.1.198:8081/api/v1/subscriptions > baseline/subscriptions.before.json
# 2) 换版：旧容器改名为 md-server-1.1.0-quote-bnb-bak 并停止（restart=no），按旧容器参数（内存 512MiB/swap/2CPU/端口/日志/重启策略）用新镜像重建同名 md-server
python3 evidence/1.1.0/nas-198/tools/pt_swap.py swap md-server:1.1.1 md-server-1.1.0-quote-bnb-bak
# 3) 立刻补订阅
node scripts/compat/restore_subs.mjs baseline/subscriptions.before.json http://192.168.1.198:8081 --apply
```

注意：`md-server-1.1.0-bak`（1.1.0 阶段 1，quote 关）与 `md-server-1.0.0-bak` 已存在，**不要复用这两个名字**；本次备份容器名为 `md-server-1.1.0-quote-bnb-bak`（符合工具的 `md-server-*-bak` 白名单）。

## 4. 换版后立即验收（10 分钟内）

| 项 | 判据 |
|----|------|
| 健康 | `/health` 200；容器 healthy |
| 消费者 | `md_ws_active_clients` 在约 15 秒内回到 2；OMS 容器保持 healthy |
| 订阅 | `compare.mjs subs baseline/subscriptions.before.json <换版后>`：缺失 0、多出 0（`data_type:3` 的 2 条 quote 仍在） |
| 逐字节 | 旁路同窗口起一个 `md-server:1.1.0-quote-bnb`（补同样订阅），`compare.mjs push`：tick/kline 全部 PASS；quote.binance 见上文验收口径（数量字段偶发差异属已知噪声） |
| 指标只增 | `compare.mjs metrics`：删除 0、TYPE/HELP 变更 0；新增 `md_ws_subscriptions_active` / `md_ws_kicked_write_timeout_total` / `md_tokio_*` /（候选 A）`md_jemalloc_*` |
| `md_ws_subscriptions_active` | 等于两个消费者当前订阅的主题数之和（不随时间只增） |
| 资源 | 10 分钟 CPU（1.1.0 现网约 1.7~2% 核）与 RSS（新进程起点约 25 MiB） |

## 5. 回滚（任一验收不通过，或 24h 观察期内出现下列任一情况）

回滚触发：消费者持续断线重连、`md_ws_kicked_write_timeout_total` 增长且确认非消费者卡顿、推送与旧版不一致、RSS 斜率 > 3 MiB/h（1 小时预热后）、CPU 明显高于旧版、健康检查失败。

```bash
python3 evidence/1.1.0/nas-198/tools/pt_swap.py rollback md-server-1.1.0-quote-bnb-bak
node scripts/compat/restore_subs.mjs baseline/subscriptions.before.json http://192.168.1.198:8081 --apply
```

回滚工具：停止并改名当前 `md-server`（保留现场，名为 `md-server-<ts>-failed`）→ 备份容器改回 `md-server` → 重启策略改回 `unless-stopped` → 启动 → 等健康。
`md-server:1.1.0-quote-bnb` 镜像保留不删。回滚演练：旁路可演练镜像可用性；生产容器上的回滚演练会再中断消费者约 6~8 秒，计划内**不做**，除非业务方要求。

## 6. 换版后 24 小时观察项

- `process_resident_memory_bytes` 逐小时（旁路预期：1 小时预热后斜率 ≤ 1 MiB/h；候选 B 预期会继续上涨）；告警阈值 400 MiB（见 OPS_GUIDE §七 / DEPLOYMENT 告警规则）。
- `md_jemalloc_allocated_bytes`（候选 A）：存活堆，预期 6~12 MiB 持平；`md_jemalloc_resident_bytes` 随负载起落、会回落。
- `md_ws_subscriptions_active`：退订后回落；`md_ws_active_clients`：稳定在 2。
- `md_ws_kicked_write_timeout_total`：预期 ≈ 0；增长 = 有消费者卡顿 → 查消费者，不是服务端问题。
- `md_ticks_dropped`：行情突发时会有丢弃（来源见调查报告 §D5），日志现在限频且带队列深度；关注是否出现**无突发的持续丢弃**。
- `md_connector_reconnect_total`（OKX 每连接器约 0.2 次/小时属正常）、`md_tokio_alive_tasks`（重连前后同水平）。
- 1stbotv3 / OMS 消费正常；tick/kline/quote 的推送频率与旧版同窗口比较无差异。
- 24 小时后用同一套 `sample_metrics.mjs` 采样出 RSS 曲线，写入 `evidence/1.1.1/nas-198/post-swap-24h/`。

## 7. 本计划**不包含**

- 不改 `md-server:latest`；不改配置文件；不改 153、OMS、策略系统、1stbotv3。
- 不打 `v1.1.0` / `v1.1.1` 标签。
- 阶段 3（扩展品种）仍然暂停，直到：本换版完成 + 24 小时观察满足调查报告「准入条件」第 (3) 项。
