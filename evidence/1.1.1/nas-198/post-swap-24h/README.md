# 1.1.1 NAS换版与24小时观察

当前结论见[DEPLOYMENT_RECORD.md](DEPLOYMENT_RECORD.md)。

- preflight.json：只读核对旧容器、镜像、资源与备份名。
- subscriptions.before/after.json、compare-subs.log、restore.log：即时12=12恢复，缺失0、多出0。
- swap.log、swap-started.json、swap-result.json、container-check.json：容器切换7.1s、身份/二进制/配置核对及旧备份状态。
- prod.csv、sampler-state.json、observer.log：每分钟真实24h采样与进程生命周期，写超时/lagged/启动时间已单列。
- compat/：换后原版旁路同窗口240s，抓取期间生产临时增加两个验证连接，明确记录起止。
- acceptance-checks.jsonl、acceptance-10min.json：真实十分钟验收，未满时长不得通过。
- hourly/：每小时独立的身份、健康、订阅、内存回归与服务端日志。统一只读工具为 `python3 evidence/1.1.1/tools/check_post_swap.py --hour 20261024T0309Z --since 2026-10-24T02:13:07Z`（仓库根目录执行；每次使用新的UTC目录名与上次检查时间）。工具拒绝覆盖已有check.json，不改生产或采样；原始分钟样本仍由原进程持续写入。小时结论都是阶段性数据，不能代替24h判据。

所有长期结果仍以实际采样覆盖为准。不得提前写24h达标；不打标签、不合并PR、不扩quote。
