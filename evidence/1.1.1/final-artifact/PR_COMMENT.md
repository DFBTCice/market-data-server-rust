1.1.1 接续交付（2026-10-24）。源码固定 `a06fe20`，后续提交仅维护复验/换版工具与证据。**北京时间03:53:15已换版，十分钟验收通过，完整24h观察中；不合并、不打标签、不扩quote。**

本 PR 的三块内容：

1. **A1–A5 + jemalloc**：Gateway 退订释放、StreamMap 接收路径、订阅/运行时指标、回归与正对照、已有 ws_write_wait 真正生效。主审已裁定现网必须带 jemalloc，默认分配器不达标。
2. **内存调查**：musl 分配器高水位为主因，A1 泄漏是真缺陷但只占小头；历史 6.1h jemalloc 斜率 +0.08MiB/h、默认分配器 +16.7MiB/h 的结论已接受。此次最终产物实际7261秒对照通过，不用历史实验代替。
3. **SIGHUP/log_level**：已并入 1.1.1，不另发 1.1.2。区分文件配置/已应用配置，需重启项持续提示，订阅/日志级别热更；RUST_LOG 优先维持。镜像 YAML 仅 info→warn，容器 RUST_LOG=warn 保留，日志文案变更已接受。

最终产物：a06fe20 干净 worktree（dirty=0），SOURCE_DATE_EPOCH=1792193713，`md-connector/vendored-tls,md-server/jemalloc-stats`，同路径两次从零构建逐字节相同。

```text
SHA-256 3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d
image md-server:1.1.1
ID sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010
```

镜像 FROM md-server:1.0.0-rollback，只叠最终二进制/阶段2配置；实际旁路及换后生产容器中均已读回二进制核对 SHA。latest 和所有回滚资源保留。旧 fa7be337… / a130e411… / exp-v5 均不得部署。

复验：fmt 无输出、clippy 13 行 warning（基线18）、workspace 248/248、md-tests 11/11、compat_1_0_0 5/5、compare selftest 25/25；最终 Docker SIGHUP E2E 129/129，旧版同脚本正对照70/129（59 FAIL）。同窗口240s：Gateway/Legacy 11个tick/kline主题字节一致、订阅13=13、Legacy 9793条quote=0、metrics只新增11项。Binance quote 8/1447条数量配对差异原文保留，按主审裁定不计失败。

两小时同负载实际01:34:34–03:35:35（7261秒），每侧122有效样本、最大gap61秒、无重启，分析exit0。预热后全样本new RSS +0.0395、old +9.8207MiB/h；排除消费者退出点，持续两连接在线完整1h的new +0.0724、old +10.0034MiB/h，new allocated 3.724–4.243MiB持平。负载实际执行全部K线初始化及退订→20s→重订；不改写历史实验。见`final-artifact/memory-2h/REPORT.md`。

10-21重启：RestartCount=1、StartedAt=13:37:15Z，历史Docker事件为空；当前OOMKilled=false不足以确认上次原因，故仍记“原因未确认”。业务方决定等待期间不主动重启1.1.0，自行重启只记时间。

**实际换版：北京时间2026-10-24 03:53:15，健康恢复中断7.1秒。** 已提前告知03:53–04:05窗口；旧1.1.0容器保留为`md-server-1.1.0-quote-bnb-bak`、停止且restart=no。新容器`5d6996ca9dc0`、healthy、RestartCount0，配置/资源与计划相符。即时订阅12条，新进程初始10条，立即补回两条BNB TICK，重新GET验证12=12、缺失0、多出0；较两小时前13条少的ETHUSDT 1h有换前动态退订原文。

换后同窗口240s：Gateway/Legacy 10个tick/kline主题逐字节PASS，Legacy6683条quote=0、subs12=12、metrics新增11指标族（35个样本）、无删除或TYPE/HELP变化；临时2个验证连接已关闭。04:03:29十分钟验收通过（600.068秒、38次主动查询、10个分钟样本）：业务连接2、订阅仍12=12、掉tick0、写超时0，RSS55.86MiB；OMS只读healthy。生产初始RSS约57MiB，不用旁路约22MiB替代。

主审复核（2026-10-24，1stbotv3仓库docs/34，业务方转达并只读核对原文）：最终产物、两小时门禁、换版、十分钟验收及三点异常说明均接受。botd reconnects 146→147；同样的K线订退测试 duplicates_dropped 130522→130522，**增量0**（旧版+38），不是累计值为0。50条K线49条market_stream，1条在重订间隙按既有契约用tick_agg临时补位，随后恢复market_stream；BNB两币对报价正常。

24h事件注释：UTC10-23 17:55:51–17:59:58Z为换版前，20:39:41–20:43:36Z为换版后；botd正常浏览登记币安BNBUSDT 3m KLINE，闲置60秒后自动释放。换后两轮SUB+/SUB-原文与测试订退/重订一致，订阅12↔13后复原；保留数据，不判订阅泄漏。后续写超时如增长，仍逐次单独解释，不沿用历史旁路结论。

真实24h逐分钟采样从北京时间10-24 03:54:13开始，预计10-25 03:54:13之后按实际跨度/连续性验收；每小时后台跟进，不以休眠或网络缺口凑时长。五段式换版记录：`evidence/1.1.1/nas-198/post-swap-24h/DEPLOYMENT_RECORD.md`。观察达标后仍须主审/业务方批准标签与合并，阶段3继续暂停。

复跑命令、原始输出与五段式报告：`evidence/1.1.1/final-artifact/REPORT.md`、README；构建条件：`evidence/1.1.1/build-logs/dist-sha256.txt`；步骤/回滚：`evidence/1.1.1/nas-198/SWAP_PLAN_1.1.1.md`。

补充异常审计（用户追加三点）：`final-artifact/anomalies/REPORT.md`。17:51的日志只证明新版本一次send阻塞10s后踢除，不能从固定文案证实客户端进程停顿；共同客户端/链路原因仍未进一步定位。原dups为粗键命中，quote键漏数量、kline键漏is_closed；已有原文证实数量/闭K变化会误计。330秒追加对照：old control/churn严格同入站重复0/3715，new control/churn均0；new两连接各5个粗键命中全部为3次数量变化+2次is_closed变化，未发现退订修复残留。历史约100条static未留逐条原文，不声称已全部逐条还原。追加连接在预热期关闭，原两小时采样保持连续；后续24h CSV已补写超时/lagged/启动时间三列。
