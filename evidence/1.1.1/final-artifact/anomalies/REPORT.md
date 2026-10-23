# 两小时门禁补充：断线与重复计数（2026-10-24）

本次只补充旁路证据，不改行情源码/最终二进制，不重启正在运行的两小时负载。对应用户要求的三点；本报告不将缺失的历史数据写成已证明。

## ① 17:51:50.089Z 同毫秒断线与写超时=1

| 证据时刻（UTC） | 原文与含义 |
|---|---|
| NAS 17:51:10.648661Z | `Gateway WebSocket client kicked: write blocked longer than ws_write_wait (10s); client is not reading (stalled or half-open)`。证明某连接的一次 send 未能在10秒内完成，写超时保护确实触发；不是 lagged 踢除。见 new.server-window.log。 |
| 采样 Mac 17:51:34Z | new.csv 中 ws_clients=1、ws_subscriptions_active=5；原 static 有8个主题、churn当时5个，符合 static 被释放。两侧连接器重连计数仍为0，服务端数据处理继续，churn计数继续增长。 |
| 采样 Mac 17:51:50.089Z | old/new 两份 consumers.log 同毫秒记录 `static: connection closed, reconnect in 3s (wanted=8)`。这不是服务端写超时发生的时刻。 |
| 17:52:34–35Z | 两侧 static reconnects=1、acks=16（原来8），ws_clients恢复2，自动重连和重订阅已发生。 |

`new.metrics-now.txt` 中 write_timeout_total=1、kicked_lagged_total=0。旧版同窗口WARN日志为空（它没有写超时保护），不能据此说它没有连接故障。NAS/Mac当前时钟差约5ms、请求RTT约22ms（clock-alignment.json）；本机该窗口无Sleep/Wake转换记录，且caffeinate在阻止系统休眠，限定网络日志查询也未给出链路变化记录。

**能确认**：这是一次 static 连接未及时排空、最终两侧共同断连的事件；新版本按既定10秒保护及时释放了该连接，其余消费者/上游仍工作。**不能从现存日志证实“客户端进程/事件循环主动停顿”**：原消费者没记事件循环延迟、close code、最后收包时刻，也没有当时pcap；通用日志中的 `client is not reading` 是写超时分支的固定文案，不是客户端侧测量。共同客户端或网络链路停顿是受证据支持的解释，具体在哪一段仍未确认，不写成已证实客户端进程卡住。

没有把同毫秒本身当成因果证明，也没有把这次事件解释成退订泄漏。该事件与解释限制会一并进入两小时报告；换版后若写超时增加，仍须按24h门禁单独解释，不能一概套用本次解释。

## ② static 的约100条“重复”：原去重键是什么

`load_consumers.mjs` 的 `dups` 是**粗键再次命中次数**，不是逐字节重复数：

| 数据 | 实际键 | 没进入键的关键字段 |
|---|---|---|
| tick | `topic|trade_id` | 其余字段、接收时间 |
| kline | `topic|open_time|volume|number_of_trades|close|exchange_event_ts` | `is_closed`、open/high/low、quote_asset_volume、接收时间等 |
| quote | `topic|timestamp|bid_price|ask_price` | **bid_qty、ask_qty、last_price**、接收时间 |

每个消费者有自己的Map，超过20,000个键时删最老10,000个；这不是按秒计的时间窗口，重连后也不会清空seen。static原负载只订6个tick和2个quote，从不做订退。quote在同一毫秒同价时数量仍可变化，因此粗键相同并不意味着重复投递；跨重连保留seen也使计数不能直接视为新连接重复。

已有240秒原文审计（captured-key-audit.json）：old/new各4个Binance quote粗键命中，全部是bid_qty/ask_qty变化，均不是相同入站事件重复。追加探针再次捕获数量碰撞，见下表。**历史~100条仅保存了累计数，没有各条原文，不能事后声称已逐条证明全部100条都是数量碰撞**；报告应写“粗键命中，已用样本和对照证明存在误计，不能作为100条重复投递的证据”。

## ③ new churn 的2→8与old的14161

new原日志在17:35/17:40/17:45/17:50分钟依次为2/4/6/8；17:35时churn_cycles=0，即第一组2发生在任何订退之前，之后每个5分钟收盘恰加2。对应两个OKX 5m主题的未收盘→已收盘消息，价格/成交量没变时，只有 `is_closed` 和 `connector_receive_ts` 改变，而原粗键漏了 `is_closed`。

240秒原文已保存两个主题各一对：`kline.5m.okx.BTC-USDT-SWAP`、`kline.5m.okx.ETH-USDT-SWAP`，前条无is_closed（false被omitempty省略），后条 `is_closed:true`，粗键相同但payload不同。追加330秒探针跨过18:10收盘，又捕获同样两对。

为排除“只是把真实重复改口径藏掉”，追加探针同时记粗键与**包含connector_receive_ts的完整原文字节**，并以原版1.1.0作正对照。对照连接与订退连接各订相同13主题；订退连接每20秒退一个K线、3秒后重订。没有改原负载计数，也没有改行情程序。

| 镜像/连接 | 消息数 | 粗键命中 | 同一入站事件逐字节重复 | 不同payload的键碰撞 |
|---|---:|---:|---:|---:|
| old/control | 17420 | 4 | 0 | 4 |
| old/churn | 21066 | 3719 | 3715 | 4 |
| new/control | 17425 | 5 | 0 | 5 |
| new/churn | 17356 | 5 | 0 | 5 |

new两连接各5个粗键命中均为3次数量变化+2次is_closed变化，严格重复均为0；old订退连接则3715个原文连connector_receive_ts都完全相同的重复，静态对照为0。每连接最多保留3000对原文，分类计数继续覆盖全程，因此旧版3715为全程分类计数，其中最多3000对原文可直接审阅；new的5对全部留存。
分类器还有“故意重放相同字节必须报重复、改is_closed必须报键碰撞”的正对照。探针期间没有断线，事件循环最大额外延迟约109.4ms；这只是该330秒窗口的观测，不能拿来证明17:51未留测量的历史状态。

**结论**：new的2→8与闭K粗键碰撞相符；严格同事件重复探针在最终镜像为0、旧版订退正对照为3715，未发现退订修复残留。不能仅按原始dups总数将new的8与old的14161等同为同类故障。这个结论限于已留原文和上述对照，不声称逐条还原了未留原文的历史消息。

## 复跑、范围与后续

```bash
python3 evidence/1.1.1/tools/collect_anomaly_logs.py
python3 evidence/1.1.1/tools/audit_captured_duplicates.py
node evidence/1.1.1/tools/anomaly_probe.mjs evidence/1.1.1/final-artifact/canary/subscriptions.before.json /tmp/md-anomaly-replay 330
```

原文：server-window.log、captured-key-audit.json、live-probe/duplicate-pairs.jsonl、live-probe/events.jsonl、live-probe/summary.json。运行中的原两小时负载未改；追加连接仅在预热期18:05:59–18:11:29Z出现，两侧相同，预热截止18:34:34Z之前已关闭。没有把本次短探针替代两小时内存门禁。

采样器原先CSV没有逐分钟保存写超时计数（仅原始小时快照里有）；已为后续新启动的24h采样追加 `ws_kicked_write_timeout`、`ws_kicked_lagged`、`process_start_time_seconds` 三列，并用只读1秒采样核对两行均为1/0/非空。正在运行的旧采样保持原样，不补造历史值。

本补充不改既定换版准入：仍须真实两小时、预热后内存门禁及旧版增长正对照全部满足。断线的客户端进程/网络根因保持未证明，必须在报告中保留；没有发现由最终镜像引入的重复投递。换版后严格核对订阅恢复与24h写超时/丢弃计数。
