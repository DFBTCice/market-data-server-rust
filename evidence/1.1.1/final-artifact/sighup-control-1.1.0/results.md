# SIGHUP 重载端到端结果

runner=docker image=md-server:probe-control-1.1.0  时间=2026-10-23T17:38:21.240Z

共 129 条断言，通过 70，失败 59。

| 场景 | 断言 | 结果 |
|---|---|---|
| s00-noop | 报告成功 | PASS |
| s00-noop | 无重启提示 | PASS |
| s1-url-okxPublic | 第1次：检测到 OKX Public 地址变更并点名 | PASS |
| s1-url-okxPublic | 第1次：给出"需要重启"的结论 | PASS |
| s1-url-okxPublic | 第1次：不得报告"已成功应用" | PASS |
| s1-url-okxPublic | 第2次：检测到 OKX Public 地址变更并点名 | **FAIL** |
| s1-url-okxPublic | 第2次：给出"需要重启"的结论 | **FAIL** |
| s1-url-okxPublic | 第2次：不得报告"已成功应用" | **FAIL** |
| s1-url-okxPublic | 旧上游连接保持、新上游无连接（地址未被热切换） | PASS |
| s1-url-okxBusiness | 第1次：检测到 OKX Business 地址变更并点名 | **FAIL** |
| s1-url-okxBusiness | 第1次：给出"需要重启"的结论 | **FAIL** |
| s1-url-okxBusiness | 第1次：不得报告"已成功应用" | **FAIL** |
| s1-url-okxBusiness | 第2次：检测到 OKX Business 地址变更并点名 | **FAIL** |
| s1-url-okxBusiness | 第2次：给出"需要重启"的结论 | **FAIL** |
| s1-url-okxBusiness | 第2次：不得报告"已成功应用" | **FAIL** |
| s1-url-okxBusiness | 旧上游连接保持、新上游无连接（地址未被热切换） | PASS |
| s1-url-binanceMarket | 第1次：检测到 Binance Market(tick/kline) 地址变更并点名 | PASS |
| s1-url-binanceMarket | 第1次：给出"需要重启"的结论 | PASS |
| s1-url-binanceMarket | 第1次：不得报告"已成功应用" | PASS |
| s1-url-binanceMarket | 第2次：检测到 Binance Market(tick/kline) 地址变更并点名 | **FAIL** |
| s1-url-binanceMarket | 第2次：给出"需要重启"的结论 | **FAIL** |
| s1-url-binanceMarket | 第2次：不得报告"已成功应用" | **FAIL** |
| s1-url-binanceMarket | 旧上游连接保持、新上游无连接（地址未被热切换） | PASS |
| s1-url-binancePublic | 第1次：检测到 Binance Public(quote) 地址变更并点名 | PASS |
| s1-url-binancePublic | 第1次：给出"需要重启"的结论 | PASS |
| s1-url-binancePublic | 第1次：不得报告"已成功应用" | PASS |
| s1-url-binancePublic | 第2次：检测到 Binance Public(quote) 地址变更并点名 | **FAIL** |
| s1-url-binancePublic | 第2次：给出"需要重启"的结论 | **FAIL** |
| s1-url-binancePublic | 第2次：不得报告"已成功应用" | **FAIL** |
| s1-url-binancePublic | 旧上游连接保持、新上游无连接（地址未被热切换） | PASS |
| s2-reconnect-keeps-startup-urls | SIGHUP 给出"需要重启"，且不报成功 | PASS |
| s2-reconnect-keeps-startup-urls | OKX Public：重连回旧地址 | PASS |
| s2-reconnect-keeps-startup-urls | OKX Public：新地址仍无连接 | PASS |
| s2-reconnect-keeps-startup-urls | OKX Business：重连回旧地址 | PASS |
| s2-reconnect-keeps-startup-urls | OKX Business：新地址仍无连接 | PASS |
| s2-reconnect-keeps-startup-urls | Binance Market(tick/kline)：重连回旧地址 | PASS |
| s2-reconnect-keeps-startup-urls | Binance Market(tick/kline)：新地址仍无连接 | PASS |
| s2-reconnect-keeps-startup-urls | Binance Public(quote)：重连回旧地址 | PASS |
| s2-reconnect-keeps-startup-urls | Binance Public(quote)：新地址仍无连接 | PASS |
| s3-restart-picks-new-urls | OKX Public：新进程连到新地址 | PASS |
| s3-restart-picks-new-urls | OKX Public：新进程不再连旧地址 | PASS |
| s3-restart-picks-new-urls | OKX Business：新进程连到新地址 | PASS |
| s3-restart-picks-new-urls | OKX Business：新进程不再连旧地址 | PASS |
| s3-restart-picks-new-urls | Binance Market(tick/kline)：新进程连到新地址 | PASS |
| s3-restart-picks-new-urls | Binance Market(tick/kline)：新进程不再连旧地址 | PASS |
| s3-restart-picks-new-urls | Binance Public(quote)：新进程连到新地址 | PASS |
| s3-restart-picks-new-urls | Binance Public(quote)：新进程不再连旧地址 | PASS |
| s3-restart-picks-new-urls | 重启后文件与运行配置一致：SIGHUP 报成功 | PASS |
| s4-subs-hot-update-with-pending-url | OKX 逐笔：新增 ETH 订阅帧已发出 | PASS |
| s4-subs-hot-update-with-pending-url | OKX 逐笔：BTC 取消订阅帧已发出 | PASS |
| s4-subs-hot-update-with-pending-url | Binance：新增 ethusdt@aggTrade 订阅帧已发出 | PASS |
| s4-subs-hot-update-with-pending-url | OKX K 线：新增 ETH 订阅帧已发出 | PASS |
| s4-subs-hot-update-with-pending-url | 地址变更仍被点名为待重启，不报成功 | **FAIL** |
| s4-subs-hot-update-with-pending-url | 第二次 SIGHUP 不重复下发订阅帧 | PASS |
| s4-subs-hot-update-with-pending-url | 第二次 SIGHUP 仍提示待重启，不报成功 | **FAIL** |
| s5-invalid-yaml-keeps-running-config | 语法错误：明确报错 | PASS |
| s5-invalid-yaml-keeps-running-config | 语法错误：声明保留运行配置 | PASS |
| s5-invalid-yaml-keeps-running-config | 语法错误：不报成功 | PASS |
| s5-invalid-yaml-keeps-running-config | 语法错误：进程仍在运行 | PASS |
| s5-invalid-yaml-keeps-running-config | 语法错误：没有任何订阅动作 | PASS |
| s5-invalid-yaml-keeps-running-config | 校验失败(okx.enabled 但 business 地址为空)：明确报错 | PASS |
| s5-invalid-yaml-keeps-running-config | 校验失败(okx.enabled 但 business 地址为空)：声明保留运行配置 | PASS |
| s5-invalid-yaml-keeps-running-config | 校验失败(okx.enabled 但 business 地址为空)：不报成功 | PASS |
| s5-invalid-yaml-keeps-running-config | 校验失败(okx.enabled 但 business 地址为空)：进程仍在运行 | PASS |
| s5-invalid-yaml-keeps-running-config | 校验失败(okx.enabled 但 business 地址为空)：没有任何订阅动作 | PASS |
| s5-invalid-yaml-keeps-running-config | 文件不存在：明确报错 | PASS |
| s5-invalid-yaml-keeps-running-config | 文件不存在：声明保留运行配置 | PASS |
| s5-invalid-yaml-keeps-running-config | 文件不存在：不报成功 | PASS |
| s5-invalid-yaml-keeps-running-config | 文件不存在：进程仍在运行 | PASS |
| s5-invalid-yaml-keeps-running-config | 文件不存在：没有任何订阅动作 | PASS |
| s5-invalid-yaml-keeps-running-config | 恢复后：订阅热更新生效 | PASS |
| s5-invalid-yaml-keeps-running-config | 恢复后：报成功 | PASS |
| s6-port-offset-no-false-positive | 不误报 listen_address 变更 | **FAIL** |
| s6-port-offset-no-false-positive | 报成功 | **FAIL** |
| s7-env-override-no-false-positive | 不误报 api_gateway.listen_address 变更 | **FAIL** |
| s7-env-override-no-false-positive | 报成功 | **FAIL** |
| s8-absent-connectors | 第1次：点名 okx.subscribe_klines 未能应用 | **FAIL** |
| s8-absent-connectors | 第1次：点名 okx.subscribe_quotes 未能应用 | **FAIL** |
| s8-absent-connectors | 第1次：给出"需要重启"，不报成功 | **FAIL** |
| s8-absent-connectors | 第2次：点名 okx.subscribe_klines 未能应用 | **FAIL** |
| s8-absent-connectors | 第2次：点名 okx.subscribe_quotes 未能应用 | **FAIL** |
| s8-absent-connectors | 第2次：给出"需要重启"，不报成功 | **FAIL** |
| s9a-log-level-hot-apply | 启动日志说明过滤器来自配置的 log_level | **FAIL** |
| s9a-log-level-hot-apply | info→warn：点名 log_level 变更并开始应用 | **FAIL** |
| s9a-log-level-hot-apply | warn 级别：无变更 SIGHUP 没有任何 INFO 输出（INFO 已被过滤） | **FAIL** |
| s9a-log-level-hot-apply | warn 级别：WARN 仍可见（待重启提示照常输出） | **FAIL** |
| s9a-log-level-hot-apply | error 级别：同样的待重启情形不再输出 WARN（对照上一步 warn 级别可见） | **FAIL** |
| s9a-log-level-hot-apply | error 级别：ERROR 仍可见（YAML 解析失败照常报错） | **FAIL** |
| s9a-log-level-hot-apply | error→info：确认已应用 | **FAIL** |
| s9a-log-level-hot-apply | error→info：此后报成功（地址已改回，无待处理项） | PASS |
| s9a-log-level-hot-apply | info 恢复：无变更 SIGHUP 的 INFO 日志重新可见 | PASS |
| s9a-log-level-hot-apply | 全程没有重连 / 断线（热更，不重启） | PASS |
| s9b-log-level-from-config-at-startup | 启动阶段没有任何 INFO 日志（旧版本这里会输出全部 INFO） | **FAIL** |
| s9b-log-level-from-config-at-startup | 连接器照常连上（只是日志安静） | PASS |
| s9b-log-level-from-config-at-startup | 无变更 SIGHUP：安静 | **FAIL** |
| s9b-log-level-from-config-at-startup | warn→info：确认已应用 | **FAIL** |
| s9b-log-level-from-config-at-startup | warn→info：此后 INFO 可见（结论行报成功） | PASS |
| s9c-rust-log-takes-precedence | 启动：WARN 点名 YAML 的 log_level 被 RUST_LOG 覆盖 | **FAIL** |
| s9c-rust-log-takes-precedence | 启动：实际生效的是 RUST_LOG（有 INFO、没有 DEBUG） | PASS |
| s9c-rust-log-takes-precedence | 第1次：点名 log_level 未生效，并说明被 RUST_LOG 覆盖 | **FAIL** |
| s9c-rust-log-takes-precedence | 第1次：给出"需要重启"的结论，不报成功 | **FAIL** |
| s9c-rust-log-takes-precedence | 第1次：过滤器没有被改动（INFO 仍可见，也没有"开始应用"） | PASS |
| s9c-rust-log-takes-precedence | 第2次：点名 log_level 未生效，并说明被 RUST_LOG 覆盖 | **FAIL** |
| s9c-rust-log-takes-precedence | 第2次：给出"需要重启"的结论，不报成功 | **FAIL** |
| s9c-rust-log-takes-precedence | 第2次：过滤器没有被改动（INFO 仍可见，也没有"开始应用"） | PASS |
| s9c-rust-log-takes-precedence | 改回启动时的值：待处理项消失，报成功 | PASS |
| s9d-invalid-log-level-rejected | 启动：进程自己退出，退出码为 1（不是跑到超时被杀） | **FAIL** |
| s9d-invalid-log-level-rejected | 启动：报错点名 log_level 和错误值 | **FAIL** |
| s9d-invalid-log-level-rejected | 重载：明确报错并点名 log_level | **FAIL** |
| s9d-invalid-log-level-rejected | 重载：声明保留运行配置 | **FAIL** |
| s9d-invalid-log-level-rejected | 重载：不报成功 | **FAIL** |
| s9d-invalid-log-level-rejected | 重载：进程仍在运行 | PASS |
| s9d-invalid-log-level-rejected | 修好后：报成功，且 INFO 仍可见（运行中的日志级别没有被坏配置改动） | PASS |
| s9e-log-level-env-override | 启动：按 LOG_LEVEL=warn 生效（没有 INFO 日志） | **FAIL** |
| s9e-log-level-env-override | 无变更 SIGHUP：安静，不误报 log_level 变更 | **FAIL** |
| s9e-log-level-env-override | YAML 改动被环境变量覆盖：生效配置不变，不输出任何 INFO/WARN/ERROR | **FAIL** |
| s9f-prod-198-combination | 启动：生效的仍是 RUST_LOG=warn（没有 INFO 日志，行为与升级前一致） | PASS |
| s9f-prod-198-combination | 启动：WARN 明确说明 YAML 的 log_level 被 RUST_LOG 覆盖 | **FAIL** |
| s9f-prod-198-combination | 无变更 SIGHUP：安静 | PASS |
| s9f-prod-198-combination | 第1次：WARN 级别可见，点名 log_level 未生效并说明 RUST_LOG=warn 优先 | **FAIL** |
| s9f-prod-198-combination | 第1次：给出"需要重启"的结论 | **FAIL** |
| s9f-prod-198-combination | 第2次：WARN 级别可见，点名 log_level 未生效并说明 RUST_LOG=warn 优先 | **FAIL** |
| s9f-prod-198-combination | 第2次：给出"需要重启"的结论 | **FAIL** |
| s10-prod-log-level-warn | 无变更：没有 WARN/ERROR 输出 | PASS |
| s10-prod-log-level-warn | 仅改 OKX Business 地址，第1次：WARN 级别可见并点名 | **FAIL** |
| s10-prod-log-level-warn | 仅改 OKX Business 地址，第1次：WARN 级别给出需要重启的结论 | **FAIL** |
| s10-prod-log-level-warn | 仅改 OKX Business 地址，第2次：WARN 级别可见并点名 | **FAIL** |
| s10-prod-log-level-warn | 仅改 OKX Business 地址，第2次：WARN 级别给出需要重启的结论 | **FAIL** |
| s10-prod-log-level-warn | YAML 解析失败：WARN/ERROR 级别可见 | PASS |

## 失败断言的证据（日志/观察摘录）

### s1-url-okxPublic / 第2次：检测到 OKX Public 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:35:50.644336Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:50.645287Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxPublic / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:35:50.644336Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:50.645287Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxPublic / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:35:50.644336Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:50.645287Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：检测到 OKX Business 地址变更并点名

```
[harness] ---- SIGHUP #1 ----
2026-10-23T17:35:55.526536Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:55.528673Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #1 ----
2026-10-23T17:35:55.526536Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:55.528673Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #1 ----
2026-10-23T17:35:55.526536Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:55.528673Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：检测到 OKX Business 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:35:56.172195Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:56.173246Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:35:56.172195Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:56.173246Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:35:56.172195Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:35:56.173246Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：检测到 Binance Market(tick/kline) 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:01.690656Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:01.691295Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:01.690656Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:01.691295Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:01.690656Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:01.691295Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：检测到 Binance Public(quote) 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:07.132453Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:07.133092Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:07.132453Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:07.133092Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:07.132453Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:07.133092Z  INFO md_server: config reloaded and applied successfully
```

### s4-subs-hot-update-with-pending-url / 地址变更仍被点名为待重启，不报成功

```
2026-10-23T17:36:30.390977Z  INFO md_server:   subscription lists changed, applying...
2026-10-23T17:36:30.392224Z  INFO md_server:   binance: adding 1 subscriptions
2026-10-23T17:36:30.393641Z  INFO md_server:   okx: adding 1 tick subscriptions
2026-10-23T17:36:30.394170Z  INFO md_server:   okx: removing 1 tick subscriptions
2026-10-23T17:36:30.395660Z  INFO md_server:   okx-kline: adding 1 kline subscriptions
2026-10-23T17:36:30.396288Z  INFO md_server: config reloaded and applied successfully
```

### s4-subs-hot-update-with-pending-url / 第二次 SIGHUP 仍提示待重启，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:31.167499Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:31.169309Z  INFO md_server: config reloaded and applied successfully
```

### s6-port-offset-no-false-positive / 不误报 listen_address 变更

```
[harness] ---- SIGHUP  ----
2026-10-23T17:36:43.196832Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:43.198355Z  WARN md_server:   grpc_server.listen_address changed: 127.0.0.1:26408 -> 127.0.0.1:26398 (requires restart)
2026-10-23T17:36:43.198402Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:24108 -> 127.0.0.1:24098 (requires restart)
2026-10-23T17:36:43.199133Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s6-port-offset-no-false-positive / 报成功

```
[harness] ---- SIGHUP  ----
2026-10-23T17:36:43.196832Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:43.198355Z  WARN md_server:   grpc_server.listen_address changed: 127.0.0.1:26408 -> 127.0.0.1:26398 (requires restart)
2026-10-23T17:36:43.198402Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:24108 -> 127.0.0.1:24098 (requires restart)
2026-10-23T17:36:43.199133Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s7-env-override-no-false-positive / 不误报 api_gateway.listen_address 变更

```
[harness] ---- SIGHUP  ----
2026-10-23T17:36:47.969408Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:47.970268Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:64415 -> 127.0.0.1:64414 (requires restart)
2026-10-23T17:36:47.971224Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s7-env-override-no-false-positive / 报成功

```
[harness] ---- SIGHUP  ----
2026-10-23T17:36:47.969408Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:47.970268Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:64415 -> 127.0.0.1:64414 (requires restart)
2026-10-23T17:36:47.971224Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s8-absent-connectors / 第1次：点名 okx.subscribe_klines 未能应用

```
[harness] ---- SIGHUP #1 ----
2026-10-23T17:36:52.970137Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:52.972292Z  INFO md_server:   subscription lists changed, applying...
2026-10-23T17:36:52.973281Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-23T17:36:52.973376Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第1次：点名 okx.subscribe_quotes 未能应用

```
[harness] ---- SIGHUP #1 ----
2026-10-23T17:36:52.970137Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:52.972292Z  INFO md_server:   subscription lists changed, applying...
2026-10-23T17:36:52.973281Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-23T17:36:52.973376Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第1次：给出"需要重启"，不报成功

```
[harness] ---- SIGHUP #1 ----
2026-10-23T17:36:52.970137Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:52.972292Z  INFO md_server:   subscription lists changed, applying...
2026-10-23T17:36:52.973281Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-23T17:36:52.973376Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：点名 okx.subscribe_klines 未能应用

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:53.617290Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:53.619425Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：点名 okx.subscribe_quotes 未能应用

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:53.617290Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:53.619425Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：给出"需要重启"，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:36:53.617290Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:53.619425Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / 启动日志说明过滤器来自配置的 log_level

```
2026-10-23T17:36:57.347992Z  INFO md_connector::base: [binance] connected and synced
2026-10-23T17:36:57.348011Z  INFO md_connector::base: [okx] connected and synced
2026-10-23T17:36:57.348042Z  INFO md_connector::base: [binance-quote] connected and synced
2026-10-23T17:36:57.348073Z  INFO md_connector::base: [okx] connected and synced
2026-10-23T17:36:57.348097Z  INFO md_connector::base: [okx-quote] connected and synced
[harness] ---- process ready ----
```

### s9a-log-level-hot-apply / info→warn：点名 log_level 变更并开始应用

```
[harness] ---- SIGHUP info->warn ----
2026-10-23T17:36:58.380027Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:36:58.381055Z  INFO md_server:   log_level: info -> warn (applied)
2026-10-23T17:36:58.381672Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / warn 级别：无变更 SIGHUP 没有任何 INFO 输出（INFO 已被过滤）

```
[harness] ---- SIGHUP warn, no change ----
2026-10-23T17:37:00.115733Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:00.116649Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / warn 级别：WARN 仍可见（待重启提示照常输出）

```
[harness] ---- SIGHUP warn, url pending ----
2026-10-23T17:37:01.869781Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:01.871120Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / error 级别：同样的待重启情形不再输出 WARN（对照上一步 warn 级别可见）

```
[harness] ---- SIGHUP warn->error ----
2026-10-23T17:37:03.609244Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:03.610100Z  INFO md_server:   log_level: warn -> error (applied)
2026-10-23T17:37:03.610129Z  INFO md_server: config reloaded and applied successfully[harness] ---- SIGHUP error, same url pending ----
2026-10-23T17:37:05.354824Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:05.355740Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / error 级别：ERROR 仍可见（YAML 解析失败照常报错）

```
[harness] ---- SIGHUP error, bad yaml ----
2026-10-23T17:37:07.108999Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:07.111619Z  WARN md_server: failed to reload config: Parse error: did not find expected ',' or ']' at line 2 column 1, while parsing a flow sequence at line 1 column 13, keeping current config
```

### s9a-log-level-hot-apply / error→info：确认已应用

```
[harness] ---- SIGHUP error->info ----
2026-10-23T17:37:08.848475Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:08.849289Z  INFO md_server:   log_level: error -> info (applied)
2026-10-23T17:37:08.849335Z  INFO md_server: config reloaded and applied successfully
```

### s9b-log-level-from-config-at-startup / 启动阶段没有任何 INFO 日志（旧版本这里会输出全部 INFO）

```
2026-10-23T17:37:13.346217Z  INFO md_connector::base: [okx-quote] subscription sync complete
2026-10-23T17:37:13.346403Z  INFO md_connector::base: [binance] connected and synced
2026-10-23T17:37:13.346418Z  INFO md_connector::base: [okx] connected and synced
2026-10-23T17:37:13.346461Z  INFO md_connector::base: [binance-quote] connected and synced
2026-10-23T17:37:13.346506Z  INFO md_connector::base: [okx] connected and synced
2026-10-23T17:37:13.346551Z  INFO md_connector::base: [okx-quote] connected and synced
```

### s9b-log-level-from-config-at-startup / 无变更 SIGHUP：安静

```
[harness] ---- SIGHUP no change ----
2026-10-23T17:37:14.390046Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:14.391709Z  INFO md_server: config reloaded and applied successfully
```

### s9b-log-level-from-config-at-startup / warn→info：确认已应用

```
[harness] ---- SIGHUP warn->info ----
2026-10-23T17:37:16.141190Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:16.142384Z  INFO md_server:   log_level: warn -> info (applied)
2026-10-23T17:37:16.142407Z  INFO md_server: config reloaded and applied successfully
```

### s9c-rust-log-takes-precedence / 启动：WARN 点名 YAML 的 log_level 被 RUST_LOG 覆盖

```
2026-10-23T17:37:19.845494Z  INFO md_connector::base: [okx] connected and synced
2026-10-23T17:37:19.845515Z  INFO md_connector::base: [binance-quote] connected and synced
2026-10-23T17:37:19.845544Z  INFO md_connector::base: [binance] connected and synced
2026-10-23T17:37:19.845594Z  INFO md_connector::base: [okx-quote] connected and synced
2026-10-23T17:37:19.845633Z  INFO md_connector::base: [okx] connected and synced
[harness] ---- process ready ----
```

### s9c-rust-log-takes-precedence / 第1次：点名 log_level 未生效，并说明被 RUST_LOG 覆盖

```
[harness] ---- SIGHUP #1 ----
2026-10-23T17:37:20.904448Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:20.905806Z  INFO md_server:   log_level: debug -> warn (applied)
2026-10-23T17:37:20.906773Z  INFO md_server: config reloaded and applied successfully
```

### s9c-rust-log-takes-precedence / 第1次：给出"需要重启"的结论，不报成功

```
[harness] ---- SIGHUP #1 ----
2026-10-23T17:37:20.904448Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:20.905806Z  INFO md_server:   log_level: debug -> warn (applied)
2026-10-23T17:37:20.906773Z  INFO md_server: config reloaded and applied successfully
```

### s9c-rust-log-takes-precedence / 第2次：点名 log_level 未生效，并说明被 RUST_LOG 覆盖

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:37:21.543673Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:21.544276Z  INFO md_server: config reloaded and applied successfully
```

### s9c-rust-log-takes-precedence / 第2次：给出"需要重启"的结论，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-23T17:37:21.543673Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:21.544276Z  INFO md_server: config reloaded and applied successfully
```

### s9d-invalid-log-level-rejected / 启动：进程自己退出，退出码为 1（不是跑到超时被杀）

```
code=null timedOut=true
```

### s9d-invalid-log-level-rejected / 启动：报错点名 log_level 和错误值

```
2026-10-23T17:37:25.784146Z  INFO md_server: loading config from /cfg/config.yaml
2026-10-23T17:37:25.800301Z  INFO md_server: log_level: verbose
2026-10-23T17:37:25.800326Z  INFO md_server: gRPC: 127.0.0.1:64547
2026-10-23T17:37:25.800543Z  INFO md_server: API Gateway: 127.0.0.1:64549
2026-10-23T17:37:25.801699Z  INFO md_server: starting Binance connector
2026-10-23T17:37:25.805065Z  INFO md_server: starting OKX Public connector (trades)
2026-10-23T17:37:25.806815Z  INFO md_server: starting OKX Business connector (candles)
2026-10-23T17:37:25.806680Z  INFO md_connector::base: [binance] connecting to ws://host.docker.internal:64539/ws
2026-10-23T17:37:25.808347Z  INFO md_connector::base: [okx] connecting to ws://host.docker.internal:64543/ws
2026-10-23T17:37:25.808988Z  INFO md_server: starting Binance quote connector (bookTicker) -> ws://host.docker.internal:64541/ws
2026-10-23T17:37:25.809689Z  INFO md_connector::base: [okx] connecting to ws://host.docker.internal:64545/ws
2026-10-23T17:37:25.812370Z  INFO md_server: starting OKX quote connector (tickers) -> ws://host.docker.internal:64543/ws
2026-10-23T17:37:25.813183Z  INFO md_connector::base: [binance-quote] connecting to ws://host.docker.internal:64541/ws
2026-10-23T17:37:25.817343Z  INFO md_connector::base: [okx-quote] connecting to ws://host.docker.internal:64543/ws
2026-10-23T17:37:25.823419Z  INFO md_processor: processor dispatch loop started
2026-10-23T17:37:25.824650Z  INFO md_server: gRPC server listening on 127.0.0.1:64547
2026-10-23T17:37:25.917318Z  INFO md_server: API Gateway listening on 127.0.0.1:64549
2026-10-23T17:37:40.825570Z  WARN md_connector::base: [binance-quote] connection timeout (15s), retrying in 1s
2026-10-23T17:37:40.826530Z  WARN md_connector::base: [okx-quote] connection timeout (15s), retrying in 1s
2026-10-23T17:37:40.826455Z  WARN md_connector::base: [okx] connection timeout (15s), retrying in 1s
2026-10-23T17:37:40.826573Z  WARN md_connector::base: [okx] connection timeout (15s), retrying in 1s
2026-10-23T17:37:40.826595Z  WARN md_connector::base: [binance] connection timeout (15s), retrying in 1s
2026-10-23T17:37:41.834898Z  INFO md_connector::base: [okx] connecting to ws://host.docker.internal:64543/ws
2026-10-23T17:37:41.835305Z  INFO md_connector::base: [okx-quote] connecting to ws://host.docker.internal:64543/ws
2026-10-23T17:37:41.834861Z  INFO md_connector::base: [okx] connecting to ws://host.docker.internal:64545/ws
2026-10-23T17:37:41.837900Z  INFO md_connector::base: [binance] connecting to ws://host.docker.internal:64539/ws
2026-10-23T17:37:41.838248Z  INFO md_connector::base: [binance-quote] connecting to ws://host.docker.internal:64541/ws

```

### s9d-invalid-log-level-rejected / 重载：明确报错并点名 log_level

```
[harness] ---- SIGHUP bad level ----
2026-10-23T17:37:47.463100Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:47.464687Z  INFO md_server:   log_level: info -> verbose (applied)
2026-10-23T17:37:47.465265Z  INFO md_server: config reloaded and applied successfully
```

### s9d-invalid-log-level-rejected / 重载：声明保留运行配置

```
[harness] ---- SIGHUP bad level ----
2026-10-23T17:37:47.463100Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:47.464687Z  INFO md_server:   log_level: info -> verbose (applied)
2026-10-23T17:37:47.465265Z  INFO md_server: config reloaded and applied successfully
```

### s9d-invalid-log-level-rejected / 重载：不报成功

```
[harness] ---- SIGHUP bad level ----
2026-10-23T17:37:47.463100Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:47.464687Z  INFO md_server:   log_level: info -> verbose (applied)
2026-10-23T17:37:47.465265Z  INFO md_server: config reloaded and applied successfully
```

### s9e-log-level-env-override / 启动：按 LOG_LEVEL=warn 生效（没有 INFO 日志）

```
2026-10-23T17:37:52.864286Z  INFO md_connector::base: [okx-quote] subscription sync complete
2026-10-23T17:37:52.864469Z  INFO md_connector::base: [binance-quote] connected and synced
2026-10-23T17:37:52.864497Z  INFO md_connector::base: [binance] connected and synced
2026-10-23T17:37:52.864523Z  INFO md_connector::base: [okx] connected and synced
2026-10-23T17:37:52.864558Z  INFO md_connector::base: [okx] connected and synced
2026-10-23T17:37:52.864593Z  INFO md_connector::base: [okx-quote] connected and synced
```

### s9e-log-level-env-override / 无变更 SIGHUP：安静，不误报 log_level 变更

```
[harness] ---- SIGHUP no change ----
2026-10-23T17:37:53.912602Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:53.913960Z  INFO md_server:   log_level: warn -> info (applied)
2026-10-23T17:37:53.914635Z  INFO md_server: config reloaded and applied successfully
```

### s9e-log-level-env-override / YAML 改动被环境变量覆盖：生效配置不变，不输出任何 INFO/WARN/ERROR

```
[harness] ---- SIGHUP yaml->debug ----
2026-10-23T17:37:55.656190Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-23T17:37:55.657608Z  INFO md_server:   log_level: info -> debug (applied)
2026-10-23T17:37:55.657654Z  INFO md_server: config reloaded and applied successfully
```

### s9f-prod-198-combination / 启动：WARN 明确说明 YAML 的 log_level 被 RUST_LOG 覆盖

```
(无)
```

### s9f-prod-198-combination / 第1次：WARN 级别可见，点名 log_level 未生效并说明 RUST_LOG=warn 优先

```
[harness] ---- SIGHUP yaml->debug #1 ----
```

### s9f-prod-198-combination / 第1次：给出"需要重启"的结论

```
[harness] ---- SIGHUP yaml->debug #1 ----
```

### s9f-prod-198-combination / 第2次：WARN 级别可见，点名 log_level 未生效并说明 RUST_LOG=warn 优先

```
[harness] ---- SIGHUP yaml->debug #2 ----
```

### s9f-prod-198-combination / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP yaml->debug #2 ----
```

### s10-prod-log-level-warn / 仅改 OKX Business 地址，第1次：WARN 级别可见并点名

```
[harness] ---- SIGHUP business url #1 ----
```

### s10-prod-log-level-warn / 仅改 OKX Business 地址，第1次：WARN 级别给出需要重启的结论

```
[harness] ---- SIGHUP business url #1 ----
```

### s10-prod-log-level-warn / 仅改 OKX Business 地址，第2次：WARN 级别可见并点名

```
[harness] ---- SIGHUP business url #2 ----
```

### s10-prod-log-level-warn / 仅改 OKX Business 地址，第2次：WARN 级别给出需要重启的结论

```
[harness] ---- SIGHUP business url #2 ----
```
