# NAS 198 换版计划：1.1.0（quote-bnb）→ 1.1.1

状态（2026-10-24）：换前所有门禁已通过，已按业务方授权在提前告知的北京时间03:53–04:05窗口内换版。03:53:15开始，健康恢复中断7.1秒；立即补订阅12=12，换后240s兼容与十分钟验收均通过。1stbotv3主审docs/34已复核通过上述产物/门禁/换版及其消费者侧结果、接受三点异常说明。当前是完整24h观察阶段，**不得重复执行换版**。实际记录及正常订阅事件注释见[post-swap-24h/DEPLOYMENT_RECORD.md](post-swap-24h/DEPLOYMENT_RECORD.md)。等待期间遵守了不主动重启旧版的决定。

当前回滚库存补记（UTC10-24 17:28Z）：原三个停止备份已不在，清理来源未确认；用保留的精确1.1.0镜像重建同名停止备份，ID`9dbf93da4951…`、restart=no。现网未重启，其余两个早期备份仍缺失。写超时新增1次、客户端来源待确认；详见[独立记录](post-swap-24h/anomalies/write-timeout-20261024T163408Z/REPORT.md)，本计划原§5/§6判据不变。

仅操作 Portainer `192.168.1.198:9000` 的 endpoint 3（名称 local，unix socket）；不操作其它业务容器。JWT 仅放 `/tmp/pt_jwt`（600），密码不落盘。绝不打标签或合并 PR；阶段 3 仍须 24h 达标、主审复核和业务方选择币对。

## 1. 唯一发布产物与镜像

- 源码：`a06fe2095e8fc58bff1e038a594b25e7002aff1b`，干净 worktree `dirty=0`。
- 特性：`md-connector/vendored-tls,md-server/jemalloc-stats`。默认分配器已裁定不达标，不作为发布选项。
- 二进制：`dist/md-server-1.1.1-linux-x86_64`。
- SHA-256：`3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d`。
- 固定 `SOURCE_DATE_EPOCH=1792193713`，相同路径连续两次从零构建逐字节一致。复现：`bash evidence/1.1.1/tools/build_final.sh`。原文：`../build-logs/dist-sha256.txt`。
- 镜像：`md-server:1.1.1`，ID `sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010`。
- `FROM md-server:1.0.0-rollback`，仅叠最终二进制、现网阶段 2 配置和来源标签。实际运行容器中读回二进制核对 SHA。
- 配置唯一差异：`log_level: "info"` → `log_level: "warn"`。容器 **`RUST_LOG=warn` 保留**；其它参数、端口、512MiB/swap 1GiB/2CPU、`unless-stopped` 不变。配置与 diff：`../final-artifact/config.final.yaml`、`config.diff`。
- `md-server:latest` 仍为 1.0.0，与 `md-server:1.0.0-rollback` 一致，是升级链回滚依据。保留所有旧镜像和备份容器。
- 历史 `fa7be337…`（脏树、无 SIGHUP 修复）、`a130e411…`（无 jemalloc）、`exp-v5` 均不得上现网。

## 2. 换版前门禁

1. `bash evidence/1.1.1/tools/verify_final.sh`：fmt 无输出；clippy 13 行 warning（不超过基线 18）；工作区 248、md-tests 11、compat_1_0_0 5 均通过；compare selftest 25/25。
2. 最终二进制 Docker SIGHUP/log_level E2E 129/129；同一脚本检测旧版必须暴露缺陷。证据：`../final-artifact/sighup-final/`、`sighup-control-1.1.0/`。
3. `python3 evidence/1.1.1/tools/final_canary.py`：两只 ≤256MiB/2CPU 的旁路，分别运行最终镜像和原版 1.1.0，灌同一份实时现网订阅，Gateway/Legacy 同窗口 240s，tick/kline/subs/metrics/noquote/selftest 均须 PASS。quote 原文独立留证，只豁免主审裁定的 Binance 数量配对噪声，不豁免结构或频率错误。
4. 同一对旁路运行 ≥2h，两类消费者同负载，实际执行退订→等待20s→重订。`sample_metrics.mjs` 每分钟取样。`python3 evidence/1.1.1/tools/analyze_final_memory.py` 必须 exit 0：至少 2h，预热后至少 1h，RSS 回归斜率 ≤1MiB/h、存活堆持平、旧版同负载可见增长。失败或证据不足则继续验证，不能换版。两只旁路结束即删。
5. 检查当前现网身份、镜像 SHA 和回滚容器名无冲突。用换版前**即时** GET 订阅集合，不能复用旁路启动时的快照（运行期订阅会变化）。
6. 用户追加异常审计见 `../final-artifact/anomalies/REPORT.md` 与 `assessment.json`。粗键误计和严格重复已区分，最终镜像严格重复0、旧版订退正对照3715；历史同毫秒断线只证实连接写阻塞，客户端进程/网络具体根因未证明，须保留这一限制，不能写成已证实客户端卡顿。

## 3. 换版（全部门禁通过后）

先在本聊天写出北京时间窗口，供业务方转主审。主审核对 1stbotv3 的重连、重复计数与 K 线来源；不直接向其它会话发消息。

```bash
# 在仓库根目录执行；OUT 为本次换版独立目录，避免覆盖历史证据。
OUT=evidence/1.1.1/nas-198/post-swap-24h
mkdir -p "$OUT"
curl --fail --silent --show-error http://192.168.1.198:8081/api/v1/subscriptions > "$OUT/subscriptions.before.json"
curl --fail --silent --show-error http://192.168.1.198:8081/metrics > "$OUT/metrics.before.txt"
python3 evidence/1.1.0/nas-198/tools/pt_swap.py swap md-server:1.1.1 md-server-1.1.0-quote-bnb-bak
node scripts/compat/restore_subs.mjs "$OUT/subscriptions.before.json" http://192.168.1.198:8081 --apply
```

备份名称 `md-server-1.1.0-quote-bnb-bak`。既有 `md-server-1.0.0-bak`、`md-server-1.1.0-bak` 不复用、不删除。换版中途失败保留现场并按回滚恢复，不能仅因脚本非 0 就遗留无服务状态。

## 4. 立即验收（10 分钟）

- `/health` 200 且 Docker healthy；运行镜像 ID、容器内二进制 SHA、YAML 与本计划完全一致。
- `restore_subs` 后重新 GET，`compare.mjs subs` 缺失 0、多出 0；有消费者同时增删时保留前后快照并解释来源，不能掩盖缺失。
- `md_ws_active_clients` 约 15s 恢复到 2；读取 OMS healthy，消费者无持续重连。1stbotv3 的 reconnects 只多 1、同样K线订退测试期间 duplicates_dropped **增量为0**（不是累计为0）、K线实时来源为market_stream，由主审核对。docs/34已通过：重订间隙一条tick_agg补位符合botd现有契约，随后恢复market_stream。
- `/metrics` 出现六项 `md_jemalloc_*`、`md_ws_subscriptions_active`、`md_ws_kicked_write_timeout_total`。
- 记录 10 分钟 RSS/CPU/订阅/重连/丢弃与写超时计数；再与原版旁路做同窗口 tick/kline/Legacy 比对，正对照用 compare selftest。

## 5. 回滚（已授权换版的恢复步骤）

触发：健康失败、消费者持续重连、未恢复订阅、推送不一致、CPU 明显异常、预热后 RSS 斜率 >3MiB/h，或写超时计数增长且无法归因于消费者卡顿。

```bash
python3 evidence/1.1.0/nas-198/tools/pt_swap.py rollback md-server-1.1.0-quote-bnb-bak
node scripts/compat/restore_subs.mjs evidence/1.1.1/nas-198/post-swap-24h/subscriptions.before.json http://192.168.1.198:8081 --apply
```

失败的 1.1.1 容器改名保留现场，旧镜像和备份不删。回滚后立即验健康、补订阅、确认两消费者恢复。不做会额外中断现网的回滚演练。

## 6. 换版后完整 24 小时

```bash
node scripts/compat/sample_metrics.mjs http://192.168.1.198:8081 86400 60 evidence/1.1.1/nas-198/post-swap-24h/prod.csv --snapshot-every-min 60
```

每分钟采样必须实际覆盖 24h；有休眠或网络缺口须如实记录，证据不足不宣布达标。预热 1h 后 RSS 斜率 ≤1MiB/h、`md_jemalloc_allocated_bytes` 持平、`md_ticks_dropped` 不再增长、`md_ws_kicked_write_timeout_total` 为 0 或有可验证解释、订阅数/连接数正常，OMS 与 1stbotv3 消费正常。保留进程 uptime 以识别自行重启。
新启动的采样器CSV逐分钟增加 `ws_kicked_write_timeout`、`ws_kicked_lagged`、`process_start_time_seconds` 三列；旧的两小时采样不重启、不补造历史列。

主审已确认正常消费者事件：UTC10-23 17:55:51–17:59:58Z（换版前）与20:39:41–20:43:36Z（换版后），botd经正常K线浏览登记币安BNBUSDT 3m KLINE，闲置60秒后释放，订阅数临时+1后复原。换后原文为两轮SUB+/SUB-，是测试的退订/重订过程；保留原始采样，按事件来源注释，不当作泄漏。任何新写超时增长仍须单独解释，不能沿用历史旁路或上述正常订阅的归因。

RSS ≥400MiB 是一般处置阈值，≥480MiB 紧急。本次等 1.1.1 的期间，按业务方补充只记录、不主动重启旧版。

24h 之后五段式报告交业务方转主审。**主审复核和业务方批准后**才打 `v1.1.1`、合并 PR #2；阶段 3 每次一个币对、各观察 24h，币对由业务方决定。

## 7. 现网历史重启

2026-10-21 13:37:15Z（北京时间 21:37:15）启动，`RestartCount=1`；当前 `State.OOMKilled=false`，该历史窗口 Docker events 返回 0 条，容器日志亦无保留证据。**已确认重启，原因未确认，不能写成已证实 OOM**。证据：`../final-artifact/baseline/restart-20261021.*`。
