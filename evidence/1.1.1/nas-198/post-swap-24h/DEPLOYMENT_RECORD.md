# NAS 198 1.1.1 换版记录（2026-10-24，24小时观察中）

已按业务方对本聊天后台流程的明确授权，在全部换版前门禁通过后执行。提前告知北京时间03:53–04:05窗口；03:53:15开始，健康恢复中断约7.1秒。换版、即时订阅恢复及镜像身份检查通过，04:03:29十分钟验收通过；完整24小时尚未完成。未打标签、未合并PR、未扩quote。

## 一、做了什么

- 仅操作Portainer endpoint3的md-server。旧容器1de7da0c7d2c改名md-server-1.1.0-quote-bnb-bak并停止、restart=no，保留旧镜像和原两个备份；latest仍指1.0.0。
- 新容器5d6996ca9dc0，md-server:1.1.1镜像ID sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010。
- 实际运行容器中读回二进制SHA-256 3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d，与a06fe20干净树两次从零构建相同。运行配置逐字节等于final-artifact/config.final.yaml：阶段2配置仅log_level info→warn；容器RUST_LOG=warn保留。512MiB、swap共1GiB、2CPU、端口和unless-stopped均核对不变。
- 执行前即时订阅12条，新进程初始10条；立刻补回binance BNBUSDT和okx BNB-USDT-SWAP两条TICK，恢复为12=12、缺失0、多出0。两小时旁路开始时13条中的ETHUSDT 1h在19:35Z已于换版前被移除（pre-swap-observe事件原文），因此恢复目标以即时12条为准。
- 健康恢复后两个消费者已恢复，新容器healthy、RestartCount=0；OMS只读检查healthy。主审侧1stbotv3状态须由主审确认，不将服务端WS=2等同于已检查其内部去重/来源。
- 24小时每分钟只读采样从19:54:13Z（北京时间03:54:13）开始，预计2026-10-25 03:54:13以后才可验收。采样进程PID40311，防空闲休眠；按真实样本跨度和连续性判断，不仅凭运行时长。

## 二、怎么验证的（命令与原文）

换前：固定a06fe20，248个工作区测试、md-tests11、compat_1_0_0 5、compare selftest25；最终Docker E2E129/129，旧版正对照59条FAIL；240秒同窗口通过；两小时7261秒通过。持续两消费者在线的预热后完整1h，new RSS +0.0724MiB/h，old正对照+10.0034；allocated 3.724–4.243MiB持平。详见../../final-artifact/memory-2h/REPORT.md。

```bash
python3 evidence/1.1.1/tools/execute_final_swap.py
python3 evidence/1.1.1/tools/observe_post_swap.py
python3 evidence/1.1.1/tools/post_swap_compat.py
python3 evidence/1.1.1/tools/accept_post_swap.py
```

```text
从发出停止旧容器到新容器健康共7.1s
baseline=12 current=10 missing(TICK/KLINE)=2
200 binance TICK BNBUSDT
200 okx TICK BNB-USDT-SWAP
after: total=12 stillMissing=0 extraVsBaseline=0
old=12 条, new=12 条, 缺失=0, 非预期多出=0
RESULT: PASS
ws_active_clients=2, docker_health=healthy
```

原文：swap.log、restore.log、compare-subs.log、swap-result.json、container-check.json。六项jemalloc指标出现、写超时0。生产首次RSS约57.25MiB，不能以旁路约22MiB替代生产观测；是否收敛仍看完整24h。换后同窗口240s已PASS：Gateway/Legacy的10个tick/kline主题逐字节一致，Legacy6683条quote为0，订阅12=12，指标族新增11、无删除或TYPE/HELP变更。抓取期间临时增加两个抓取连接，时段记录在compat/capture-started.json与capture-finished.json；它们结束后必须回到2个业务连接，不能误报为业务重连风暴。

十分钟实际截止20:03:28.972Z（北京时间04:03:29），距即时检查600.068秒。38次主动检查及10个分钟样本通过；业务连接2、订阅仍12=12、缺失0、多出0、Docker healthy、RestartCount0、掉tick0、写超时0。RSS55.863MiB，CPU累计11.12秒/uptime606秒（单核等效平均约1.84%）。原文acceptance-10min.json、acceptance-checks.jsonl、subscriptions.after-10min.json。之后后台跟进改每小时，原每分钟采样继续。

## 三、没做什么与原因

- 完整24小时尚未覆盖，未宣称观察达标；不得据十分钟健康直接打标签、合并PR或扩品种。
- 2026-10-21重启原因仍未确证；等待最终产物期间未主动重启旧版，执行前RestartCount仍为1。未把疑OOM写成定论。
- 用户追问的旁路写超时只能确认连接未排空，不能证实历史客户端进程卡顿；静态约100条粗键命中未留每条原文，不能逐条回溯。严格探针new重复0、old订退3715，细节与限制见../../final-artifact/anomalies/REPORT.md。
- 未修改153、OMS、策略系统、1stbotv3；OMS仅做一次只读健康查询。未做会增加一次业务中断的生产回滚演练，保留可执行回滚路径。

## 四、兼容性声明（docs/24 §三）

1. 换前最终镜像tick/kline同窗口逐字节通过；换后与原版旁路同窗口240s，10主题Gateway/Legacy均PASS，原文compat/compare-push-*.log。
2. REST/gRPC行情源码未改；换版即时订阅12=12，缺失0、多出0。
3. Legacy永不含quote；换前通过，换后6683条quote为0，再次PASS。
4. proto/既有RPC与字段号未改；构建与兼容回放通过。
5. 旧配置兼容，本次仅日志级别对齐warn，无新增必填项。
6. md-tests11/11实际执行，原文见final-artifact/verify/。
7. 换前metrics只增11项；换后旧→新无删除或TYPE/HELP变化、新增11指标族。新增35个样本中包含只有现网业务消费者订阅quote才有的观测标签，不能称作35个新指标族。
8. 资源需实测：生产RSS初始约57MiB，完整24h仍待完成；旁路完整1h持续负载new+0.0724、old+10.0034MiB/h是有效正对照。

## 五、需要对方做的事

业务方转交本记录给主审。主审核对1stbotv3 reconnects仅增加1、duplicates_dropped保持0、K线仍为market_stream；OMS健康已只读核对。24h报告实际完成后，再由主审与业务方决定标签/合并与阶段3币对。

回滚命令（异常触发时按已授权流程执行并补订阅）：

```bash
python3 evidence/1.1.0/nas-198/tools/pt_swap.py rollback md-server-1.1.0-quote-bnb-bak
node scripts/compat/restore_subs.mjs evidence/1.1.1/nas-198/post-swap-24h/subscriptions.before.json http://192.168.1.198:8081 --apply
```
