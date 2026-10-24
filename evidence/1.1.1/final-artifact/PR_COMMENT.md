1.1.1最终产物、两小时门禁、换版、十分钟验收已通过主审复核；**完整24h采集现已结束，整体验收未通过**。掉tick增长2460，1次写超时缺少充分客户端归因，不能以采集完成代替准入通过。不打标签、不合并PR，阶段3暂停。

## 一、做了什么

固定源码a06fe2095e8fc58bff1e038a594b25e7002aff1b干净树、SOURCE_DATE_EPOCH1792193713，同路径两次从零构建vendored-tls + jemalloc，字节相同。

- binary SHA256：3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d。
- md-server:1.1.1镜像ID：sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010。
- 生产容器：5d6996ca9dc000c162c02ea3a4d5dca7baf29fd1a9cc98601c72a9552cecd812。

北京时间10-24 03:53:15在提前告知窗口内换版，中断7.1秒，立刻补两条BNB TICK后订阅12=12、缺失0多出0；04:03:29十分钟验收通过。仅YAML info→warn，RUST_LOG=warn优先，512MiB/swap共1GiB/2CPU不变，latest未改。后续提交为工具/证据/文档，产物源码仍a06fe20。

## 二、怎么验证的

完整窗口UTC10-23 19:54:13至10-24 19:54:19（北京时间10-24 03:54:13至10-25 03:54:19）：1441/1441有效点，86406秒，间隔59–61秒，字段无缺失。采样exit0、状态finished；process_start始终1792785202、uptime无回退、最终RestartCount0。

| SWAP_PLAN §6项目 | 实测 | 判定 |
|---|---|---|
| 连续完整24h/无重启 | 1441点、86406秒、maxgap61秒 | 通过 |
| 预热1h后RSS≤1MiB/h | 全1381点、82806秒，OLS **+0.970764MiB/h** | 数值门槛通过 |
| allocated持平 | 4.189–4.361MiB，OLS−0.000655MiB/h | 通过 |
| tick丢弃不增长 | **0→1327→2460** | **未通过** |
| 写超时0或可验证解释 | **0→1**，具体测试/客户端未确认 | **未通过：归因不足** |
| 订阅/消费者 | 最终12=12、缺失0多出0、WS2、md-server/OMS healthy；17个孤立WS1点来源未知 | 最终正常，全天行为有证据限制 |

RSS预热后56.035→78.277MiB，范围55.309–79.129，台阶全部保留，不能宣称RSS完全不增长或全天收敛。CPU单核等效全窗平均约1.777%，不排除瞬时调度问题。

tick两次增长由WARN证实1000/1000入口队列满、try_send真实丢弃。限频日志since-last-log可能含先前抑制累计，不能逐行相加代替增量。事后61个约1秒点、CPU限流累计0，不能证明事发微突发是唯一根因；旧版beaac94同机制，但无今天同窗口旧版正对照，不构成豁免。

写超时UTC16:34:08.768，发送等待超过10秒；WS2→3→2、接收端12→24→12，REST订阅仍12。固定“client is not reading”不能证明客户端进程停顿。业务方只说明近期有其他测试、“应该是”，不清楚具体哪项；已备注可能关联，未升级为确认，不因后续保持1次就自动通过。

正常BNBUSDT 3m浏览导致UTC20:40/20:42 SUB+、20:41/20:43 SUB-，12↔13后回12；主审提供的17:55–17:59时段属换版前，不计入本窗。WS4的4点为兼容抓取，WS3的1点关联上述额外连接；另17个孤立WS1点均下一点回2，不能当成精确17分钟断线。上游重连OKX tick/kline/quote=5/2/3，Binance tick/quote=1/1，不能与botd内部reconnects混用。

主审docs/34早先独立核对：botd146→147仅+1，同样K线订退duplicates_dropped130522→130522增量0（旧版+38）；50条K线49条market_stream、1条重订间隙按契约tick_agg补位后恢复；BNB两币对报价正常。该结论只覆盖当时测试窗口，不扩展为全天内部计数不变。

## 三、没做什么与原因

掉tick后评估暂不回滚：旧版同机制、无已证实改善；这不是豁免。单次写超时关联额外连接且已释放，但客户端归因不足，SWAP_PLAN §5回滚评估未关闭，不能把未回滚写成触发条件已经排除。当前仍1.1.1，失败项与限制交业务方/主审决定后续处置。未改缓冲区/资源/源码，未清零/重启或延长采样以凑PASS。

原三个停止备份曾被人工清理，业务方已确认，具体时间/规则未知；原镜像仍在。UTC17:28:57用精确旧镜像a46c88d0…重建同名停止备份md-server-1.1.0-quote-bnb-bak，新ID9dbf93da4951…、created、restart=no，最终仍在。只恢复当前1.1.0回滚准备；两个更早备份仍缺失，原容器日志不可恢复。未启动备份或重启生产。回滚前另存即时非空订阅，切回后立即补回核验。

10-21历史重启原因未确认，不写成OOM；等待期间未主动重启旧版。未写153、OMS、策略、1stbotv3或latest，OMS只读。报告交付后删除本观察自动化。

## 四、兼容性声明

fmt无输出、clippy13行warning（基线18）、workspace248、md-tests11、compat5、compare selftest25均通过。最终Docker E2E129/129，旧版正对照70/129。换前240s Gateway/Legacy11主题字节一致、Legacy9793条quote0；换后240s10主题通过、Legacy6683条quote0、订阅12=12。metrics只增11指标族，无删除或TYPE/HELP变化；35个新增标签样本不是35指标族。REST/gRPC、proto字段未改，旧配置兼容。

两小时实际7261秒、每侧122点，持续两连接预热后1h new RSS+0.0724、old+10.0034MiB/h，allocated持平；不能代替今天丢tick时段的旧版对照。jemalloc必须、SIGHUP/log_level归属1.1.1、RUST_LOG优先、文案接受等裁定维持。Binance quote数量配对噪声不计，不豁免本次真实入口丢弃。

历史三点审计：17:51旁路只证明发送阻塞，客户端进程停顿未证实；原粗键漏quote数量/kline is_closed，new churn2→8为闭K误计；330s严格原文探针new control/churn均0、old churn3715，未发现退订修复残留。约100个静态粗键命中无逐条原文，无法全量追溯。

## 五、需要对方做的事

业务方转主审复核完整报告并明确后续处置或任何例外；本报告不是准入批准。**不打标签、不合并PR、不扩quote，阶段3暂停。**

完整五段式报告：`evidence/1.1.1/nas-198/post-swap-24h/REPORT.md`。原文prod.csv、sampler-state.json，分析analysis.json、完整PNG/SVG曲线、final-state/与两个独立anomalies目录均保留。复跑：`python3 evidence/1.1.1/tools/analyze_post_swap_24h.py`；exit0是分析完成，`criteria.overall_pass=false`是验收未通过。
