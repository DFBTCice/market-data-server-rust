# 最终产物交付记录（2026-10-24，复验进行中）

对照 docs/33 §5：第 1、3、4 条已完成，第 2 条 Docker E2E 与 240s 同窗口均通过、2h 旁路仍在运行，第 5 条确认发生过重启但原因证据不足；第 6 条说明评论见下方 PR 记录。第 7 条已获业务方“最终复验后直接换版”的授权，尚未执行；第 8 条仍须完整 24h、主审复核和业务决定。
最终 SHA-256：`3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d`。镜像 `md-server:1.1.1` = `sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010`。
没有将尚未完成的两小时/24小时观察写成通过；当前不需要重复决定分配器、日志优先级或版本归属。

## 一、做了什么

- 固定源码 `a06fe20`、干净 worktree、`SOURCE_DATE_EPOCH=1792193713`、相同 target 路径，两次从零构建 vendored-tls + jemalloc 静态 x86_64 musl 二进制，SHA 完全一致。
- 基于 `md-server:1.0.0-rollback` 构建最终镜像。现网阶段 2 配置仅改 YAML log_level 为 warn，容器 RUST_LOG=warn 保留，`latest` 和回滚资源保留。
- CHANGELOG 未发布内容合并入 1.1.1；修正 OPS_GUIDE 中“只热更订阅列表”的旧句，保留 RSS ≥400MiB 告警，并记录本次等待期间不主动重启的授权。
- 验证工具纠正负载顺序：初始化订阅全部 K 线，按约定退订→等待20s→重订，日志明确记录每轮操作。此修改只影响本次复验，不改写历史实验或行情二进制。
- 现网仅 GET：当前容器 RestartCount=1，2026-10-21 13:37:15Z 启动。历史 Docker events 无保留记录，当前 OOMKilled=false 不能排除上次 OOM；原因未确认。

## 二、怎么验证的（命令与原文）

```bash
bash evidence/1.1.1/tools/build_final.sh
bash evidence/1.1.1/tools/verify_final.sh
node scripts/compat/sighup_reload_e2e.mjs --runner docker --image md-server:probe-final-a06fe20 --out evidence/1.1.1/final-artifact/sighup-final
node scripts/compat/sighup_reload_e2e.mjs --runner docker --image md-server:probe-control-1.1.0 --out evidence/1.1.1/final-artifact/sighup-control-1.1.0 --report-only
python3 evidence/1.1.1/tools/inspect_198.py
python3 evidence/1.1.1/tools/final_canary.py
# 两小时完成后执行；未完成会拒绝通过：
python3 evidence/1.1.1/tools/analyze_final_memory.py
```

已完成输出：

```text
commit: a06fe2095e8fc58bff1e038a594b25e7002aff1b dirty=0
features=md-connector/vendored-tls,md-server/jemalloc-stats
cmp: identical; dirty=0 after both builds
fmt: exit 0, 0 bytes output
clippy: exit 0, 13 warning lines（基线 18）
workspace: 248 passed, 0 failed
md-tests: 11 passed; compat_1_0_0: 5 passed
SELFTEST OK: 25/25 个已知样本均得到预期结论
Docker final: 汇总 129/129 PASS, 0 FAIL
Docker 1.1.0 control: 汇总 70/129 PASS, 59 FAIL
240s Gateway tick/kline（11 主题）: RESULT: PASS
240s Legacy tick/kline（11 主题）: RESULT: PASS
Legacy: 9793 条数据消息，quote 0
subs: old=13, new=13, 缺失=0, 非预期多出=0
metrics: 新增=11, 删除=0, TYPE/HELP 变更=0
quote Binance: 8/1447 对应条数量不同，按主审裁定不计失败
```

原文全部在本目录与 `../build-logs/`，详见 README。对比器 selftest 会故意改价格、字段顺序、删除记录/指标、混入 quote 等，均得到预期失败；旧版 Docker E2E 同方法检出 59 条失败，而最终产物全部通过。两小时采样从 2026-10-23 17:34:34Z（北京时间 10-24 01:34:34）开始，预计 03:36 后才可验收。

## 三、没做什么与原因

- 尚未换现网：240s 已通过，等待实际 ≥2h 的内存门禁通过；不主动重启旧 1.1.0，自行重启只记时间。
- 24h 观察尚未开始，不能提前证明 RSS 收敛、丢弃不增长或消费者连续正常。未合并 PR、未打标签、未改 Cargo 版本号。
- 无历史 OOM 事件或内核日志证据，未把“疑 OOM”升级为定论。未对 NAS 增加特权诊断容器或挂宿主机卷。
- 未扩 quote 品种、改 tick buffer、关闭 THP、实现订阅持久化，均不在本次范围。

## 四、兼容性声明（docs/24 §三 8 条）

| # | 结论与证据 |
|---|---|
| 1 | 固定输入兼容回放 5/5；最终镜像 tick/kline 11 主题同窗口 240s 在 Gateway/Legacy 均 PASS，原文 canary/compare-push-*.log。Binance quote 配对数量噪声按主审裁定独立留证，不计入 tick/kline 字节门禁。 |
| 2 | REST/gRPC 行情数据路径本次未改；md-tests 11/11 实际执行。最终两镜像订阅集合 13=13，无缺失或多出；不以不同时间的最新价相等作为断言。 |
| 3 | Legacy 最终镜像抓取 9793 条，quote 0，canary/compare-noquote-legacy.log，compare selftest 已证明混入 quote 会失败。 |
| 4 | a06fe20 源码不变；proto 相对 1.1.0 无改动、生成编译通过。 |
| 5 | 所有新增配置仍可选；本次以实际运行配置只改 log_level 构建，Docker E2E 129/129。 |
| 6 | md-tests 11/11，原文 verify/md-tests.log，存在真实快照断言。 |
| 7 | 最终镜像 metrics 新增 11、删除 0，TYPE/HELP 无变更，canary/compare-metrics.log；selftest 的删样本/改类型正对照已通过。 |
| 8 | 本次不改变已经主审验收的 quote 关闭兼容结论；新分配器的最终镜像资源以本次两小时对照为准，未以历史数据替代。 |

## 五、需要对方做的事

PR #2 说明评论已发布：<https://github.com/DFBTCice/market-data-server-rust/pull/2#issuecomment-6371055873>，之后只更新这条评论。
初次证据提交 `64e1268` 已 push 到现有分支，二进制对应源码仍为 `a06fe20`。

业务方在换版窗口确定后转告 1stbotv3 主审，主审核对换版后的 reconnects、duplicates_dropped 与 K 线来源。已授权复验完成后直接换版，无须再次选择候选或日志优先级。
业务方亦明确同意由本聊天后台执行该流程，特别要求确保订阅恢复；跟进自动化 `1-1-1` 已创建为 ACTIVE。
自动审批最初因“缺少调度器执行高影响动作的明确授权”拒绝，业务方补充同意后创建成功。当前不再受此授权缺口阻塞。
24h 观察完成后由主审复核并决定标签/PR 合并；阶段 3 币对由业务方决定。没有完整证据前保持阶段 3 暂停。
