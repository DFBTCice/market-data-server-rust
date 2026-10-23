1.1.1 接续交付（2026-10-24）。源码固定 `a06fe20`，后续 `64e1268` 记录最终产物与复验证据；**尚未换版，不合并、不打标签**。

本 PR 的三块内容：

1. **A1–A5 + jemalloc**：Gateway 退订释放、StreamMap 接收路径、订阅/运行时指标、回归与正对照、已有 ws_write_wait 真正生效。主审已裁定现网必须带 jemalloc，默认分配器不达标。
2. **内存调查**：musl 分配器高水位为主因，A1 泄漏是真缺陷但只占小头；历史 6.1h jemalloc 斜率 +0.08MiB/h、默认分配器 +16.7MiB/h 的结论已接受。此次最终产物另做实际 ≥2h 对照，进行中，不用历史实验代替。
3. **SIGHUP/log_level**：已并入 1.1.1，不另发 1.1.2。区分文件配置/已应用配置，需重启项持续提示，订阅/日志级别热更；RUST_LOG 优先维持。镜像 YAML 仅 info→warn，容器 RUST_LOG=warn 保留，日志文案变更已接受。

最终产物：a06fe20 干净 worktree（dirty=0），SOURCE_DATE_EPOCH=1792193713，`md-connector/vendored-tls,md-server/jemalloc-stats`，同路径两次从零构建逐字节相同。

```text
SHA-256 3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d
image md-server:1.1.1
ID sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010
```

镜像 FROM md-server:1.0.0-rollback，只叠最终二进制/阶段2配置；实际旁路容器中已读回二进制核对 SHA。latest 和所有回滚资源保留。旧 fa7be337… / a130e411… / exp-v5 均不得部署。

复验：fmt 无输出、clippy 13 行 warning（基线18）、workspace 248/248、md-tests 11/11、compat_1_0_0 5/5、compare selftest 25/25；最终 Docker SIGHUP E2E 129/129，旧版同脚本正对照70/129（59 FAIL）。同窗口240s：Gateway/Legacy 11个tick/kline主题字节一致、订阅13=13、Legacy 9793条quote=0、metrics只新增11项。Binance quote 8/1447条数量配对差异原文保留，按主审裁定不计失败。

两小时同负载采样从北京时间2026-10-24 01:34:34开始，预计03:36后判定；预热后RSS斜率须≤1MiB/h、allocated持平、旧版可见增长。负载脚本现按约定初始化全部K线并执行退订→20s→重订；不改写历史实验。

10-21重启：RestartCount=1、StartedAt=13:37:15Z，历史Docker事件为空；当前OOMKilled=false不足以确认上次原因，故仍记“原因未确认”。业务方决定等待期间不主动重启1.1.0，自行重启只记时间。

**换版日期：待两小时门禁完成后填入。** 业务方已授权最终复验后直接换版；执行前告知业务方转主审，换后立即restore_subs并进行10分钟验收、真实24小时逐分钟采样。观察达标后仍须主审/业务方批准标签与合并，阶段3继续暂停。

复跑命令、原始输出与五段式报告：`evidence/1.1.1/final-artifact/REPORT.md`、README；构建条件：`evidence/1.1.1/build-logs/dist-sha256.txt`；步骤/回滚：`evidence/1.1.1/nas-198/SWAP_PLAN_1.1.1.md`。

补充异常审计（用户追加三点）：`final-artifact/anomalies/REPORT.md`。17:51的日志只证明新版本一次send阻塞10s后踢除，不能从固定文案证实客户端进程停顿；共同客户端/链路原因仍未进一步定位。原dups为粗键命中，quote键漏数量、kline键漏is_closed；已有原文证实数量/闭K变化会误计。330秒追加对照：old control/churn严格同入站重复0/3715，new control/churn均0；new两连接各5个粗键命中全部为3次数量变化+2次is_closed变化，未发现退订修复残留。历史约100条static未留逐条原文，不声称已全部逐条还原。追加连接在预热期关闭，原两小时采样保持连续；后续24h CSV已补写超时/lagged/启动时间三列。
