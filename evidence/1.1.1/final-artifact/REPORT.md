# 最终产物交付记录（2026-10-24，已换版，24h观察中）

对照 docs/33 §5：第 1、3、4 条已完成，第 2 条 Docker E2E、240s 同窗口与真实两小时内存门禁全部通过，第 5 条确认发生过重启但原因证据不足；第 6 条说明评论见下方 PR 记录。第 7 条已按业务方授权于北京时间03:53:15执行，容器切换7.1秒，即时订阅12=12已恢复；换后240秒兼容与十分钟验收均已通过（04:03:29），每分钟24h采样继续；第 8 条仍须完整 24h、主审复核和业务决定。
最终 SHA-256：`3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d`。镜像 `md-server:1.1.1` = `sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010`。
两小时已实际完成；24小时仍在进行，未提前写成通过。当前不需要重复决定分配器、日志优先级或版本归属。换后记录见 [`../nas-198/post-swap-24h/DEPLOYMENT_RECORD.md`](../nas-198/post-swap-24h/DEPLOYMENT_RECORD.md)。

2026-10-24 14:27/15:16Z出现tick入口真实丢弃，累计0→1327→2460；当前SWAP_PLAN §6“掉tick不再增长”未通过，不能宣布24h全项达标。该轮健康/订阅/写超时及内存指标正常，当时暂不回滚而继续原采样的依据与限制见[异常记录](../nas-198/post-swap-24h/anomalies/tick-drop-20261024T1427Z/REPORT.md)。不以旧版已知机制作为自动豁免。

新增16:34:08Z写超时0→1：与额外第三连接及12个接收端释放相关，发送超时机制已证实，客户端来源仍待确认，未判通过、§5回滚评估尚未关闭。原三个停止备份已不在，17:28Z用保留的原镜像重建同名停止回滚备份`9dbf93da4951…`，未启动或重启现网；其余两个更早备份仍缺失。见[独立记录](../nas-198/post-swap-24h/anomalies/write-timeout-20261024T163408Z/REPORT.md)。

业务方补充（北京时间2026-10-25）：近期有其他测试，认为这次写超时可能相关，但不清楚具体发起方；已备注为可能关联，仍无客户端独立证据，写超时项未判通过。停止备份已确认有人清理过，具体时间/规则未知；17:53Z核对重建停止备份仍在。原采样1320/1320有效点、span79145s、maxgap61s、无重启，RSS预热后+0.8968MiB/h、allocated持平，write1/tick2460均未再增长，连接2、订阅12=12、healthy。 [回复原文与解释边界](../nas-198/post-swap-24h/anomalies/write-timeout-20261024T163408Z/business-clarification.json)。

## 一、做了什么

- 固定源码 `a06fe20`、干净 worktree、`SOURCE_DATE_EPOCH=1792193713`、相同 target 路径，两次从零构建 vendored-tls + jemalloc 静态 x86_64 musl 二进制，SHA 完全一致。
- 基于 `md-server:1.0.0-rollback` 构建最终镜像。现网阶段 2 配置仅改 YAML log_level 为 warn，容器 RUST_LOG=warn 保留，换版当时`latest`和回滚资源保留；当前备份重建状态见上方新增记录。
- CHANGELOG 未发布内容合并入 1.1.1；修正 OPS_GUIDE 中“只热更订阅列表”的旧句，保留 RSS ≥400MiB 告警，并记录本次等待期间不主动重启的授权。
- 验证工具纠正负载顺序：初始化订阅全部 K 线，按约定退订→等待20s→重订，日志明确记录每轮操作。此修改只影响本次复验，不改写历史实验或行情二进制。
- 换前现网只读基线：旧容器 RestartCount=1，2026-10-21 13:37:15Z 启动。历史 Docker events 无保留记录，当时 OOMKilled=false 不能排除上次 OOM；原因未确认。
- 换版后核对新容器镜像、二进制SHA、运行配置和资源；保留旧版备份，立即补订阅。十分钟验收38次主动查询、10个分钟样本通过，业务连接2、订阅12=12、掉tick0、写超时0；当时RSS55.86MiB、CPU累计11.12秒/uptime606秒。

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

原文全部在本目录与 `../build-logs/`，详见 README。对比器 selftest 会故意改价格、字段顺序、删除记录/指标、混入 quote 等，均得到预期失败；旧版 Docker E2E 同方法检出 59 条失败，而最终产物全部通过。两小时采样从 2026-10-23 17:34:34Z 至19:35:35Z（北京时间01:34:34–03:35:35），每侧122/122有效样本、跨度7261秒、最大间隔61秒、无进程重启。`analyze_final_memory.py` exit 0、全部门禁true。
预热后全样本RSS斜率：new +0.0395、old +9.8207 MiB/h。最终采样点发生在负载消费者退出后，allocated回落；为防止退出点美化趋势，另看 `memory-2h/steady-load-analysis.json`：仅保留两个负载消费者仍在线的完整1小时（61点/3600秒），new RSS +0.0724、old +10.0034 MiB/h；new allocated 3.724–4.243 MiB、斜率−0.0762 MiB/h，同样满足门禁。两只旁路已自动清理。

**用户追加的三项异常审计已完成**，见 [`anomalies/REPORT.md`](anomalies/REPORT.md)：
写超时服务端原文在17:51:10.648Z，客户端共同关闭在50.089Z；已证实连接未及时排空，但缺少历史客户端延迟/pcap，**不能证实是客户端进程停顿**。
原dups是粗键命中：quote漏数量、K线漏is_closed，历史约100条未留逐条原文，不能逐条追溯。new churn的2→8与两个OKX 5m闭K相符。
追加330秒原文探针：最终镜像control/churn严格同入站事件重复均为0；旧版churn为3715（静态control为0），正对照有效，未发现退订修复残留。
追加连接仅出现在预热期，两小时原采样未重启；后续24h采样已补逐分钟写超时/lagged/进程启动时间三列。

## 三、没做什么与原因

- 已于告知窗口换版：即时快照12条，新进程初始10条，立刻补回Binance BNBUSDT/OKX BNB-USDT-SWAP两条TICK后12=12、缺失0、多出0。换版前19:35Z时ETHUSDT 1h已从现网动态集合移除，因此以即时12条为恢复目标。等待期未主动重启旧1.1.0，也未观察到自行重启。
- 24h采样已于2026-10-23 19:54:13Z（北京时间03:54:13）启动，预计次日03:54:13后才可验收，不能提前证明完整24h达标。未合并 PR、未打标签、未改 Cargo 版本号。
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
| 8 | 本次不改变已经主审验收的 quote 关闭兼容结论；新分配器已完成7261秒对照，持续负载下预热后RSS +0.0724 MiB/h，旧版正对照+10.0034；未以历史数据替代。 |

## 五、需要对方做的事

PR #2 说明评论已发布：<https://github.com/DFBTCice/market-data-server-rust/pull/2#issuecomment-6371055873>，之后只更新这条评论。
初次证据提交 `64e1268` 已 push 到现有分支，二进制对应源码仍为 `a06fe20`。

2026-10-24业务方转达主审复核通过，并已只读核对1stbotv3仓库docs/34：最终产物、两小时门禁、换版、十分钟验收与三点异常说明接受；botd重连仅+1，同样K线订退测试重复帧增量0（旧版+38），BNB两币对报价正常。K线实时来自market_stream，重订间隙一条tick_agg临时补位符合既有契约并已由主审接受。正常BNBUSDT 3m订阅时段与服务端SUB+/SUB-原文已注释在换版记录，不误判为泄漏。该消费者结论来自主审独立证据。
业务方亦明确同意由本聊天后台执行该流程，特别要求确保订阅恢复；跟进自动化 `1-1-1` 已创建为 ACTIVE。
自动审批最初因“缺少调度器执行高影响动作的明确授权”拒绝，业务方补充同意后创建成功。当前不再受此授权缺口阻塞。
24h 观察完成后由主审复核并决定标签/PR 合并；阶段 3 币对由业务方决定。没有完整证据前保持阶段 3 暂停。
