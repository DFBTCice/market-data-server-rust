# SIGHUP 重载端到端结果

runner=docker image=md-server-probe:1.1.0-prod  时间=2026-10-16T22:18:55.925Z

共 92 条断言，通过 59，失败 33。

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
| s9-log-level-not-claimed-applied | 点名 log_level | PASS |
| s9-log-level-not-claimed-applied | 不得写"(applied)" | **FAIL** |
| s9-log-level-not-claimed-applied | 不报成功 | **FAIL** |
| s9-log-level-not-claimed-applied | 实际日志级别未变（第二次 SIGHUP 的 info 日志仍可见）——仅作事实记录 | PASS |
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
2026-10-16T22:17:33.714415Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:33.715272Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxPublic / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:33.714415Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:33.715272Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxPublic / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:33.714415Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:33.715272Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：检测到 OKX Business 地址变更并点名

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:17:38.560509Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:38.562433Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:17:38.560509Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:38.562433Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:17:38.560509Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:38.562433Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：检测到 OKX Business 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:39.213690Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:39.214610Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:39.213690Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:39.214610Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:39.213690Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:39.214610Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：检测到 Binance Market(tick/kline) 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:44.599529Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:44.600425Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:44.599529Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:44.600425Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:44.599529Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:44.600425Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：检测到 Binance Public(quote) 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:50.058283Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:50.059537Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:50.058283Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:50.059537Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:50.058283Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:17:50.059537Z  INFO md_server: config reloaded and applied successfully
```

### s4-subs-hot-update-with-pending-url / 地址变更仍被点名为待重启，不报成功

```
2026-10-16T22:18:13.102847Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T22:18:13.103758Z  INFO md_server:   binance: adding 1 subscriptions
2026-10-16T22:18:13.104695Z  INFO md_server:   okx: adding 1 tick subscriptions
2026-10-16T22:18:13.104803Z  INFO md_server:   okx: removing 1 tick subscriptions
2026-10-16T22:18:13.106543Z  INFO md_server:   okx-kline: adding 1 kline subscriptions
2026-10-16T22:18:13.107330Z  INFO md_server: config reloaded and applied successfully
```

### s4-subs-hot-update-with-pending-url / 第二次 SIGHUP 仍提示待重启，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:18:13.743108Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:13.744135Z  INFO md_server: config reloaded and applied successfully
```

### s6-port-offset-no-false-positive / 不误报 listen_address 变更

```
[harness] ---- SIGHUP  ----
2026-10-16T22:18:25.689635Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:25.690629Z  WARN md_server:   grpc_server.listen_address changed: 127.0.0.1:30364 -> 127.0.0.1:30354 (requires restart)
2026-10-16T22:18:25.690665Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:22290 -> 127.0.0.1:22280 (requires restart)
2026-10-16T22:18:25.691171Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s6-port-offset-no-false-positive / 报成功

```
[harness] ---- SIGHUP  ----
2026-10-16T22:18:25.689635Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:25.690629Z  WARN md_server:   grpc_server.listen_address changed: 127.0.0.1:30364 -> 127.0.0.1:30354 (requires restart)
2026-10-16T22:18:25.690665Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:22290 -> 127.0.0.1:22280 (requires restart)
2026-10-16T22:18:25.691171Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s7-env-override-no-false-positive / 不误报 api_gateway.listen_address 变更

```
[harness] ---- SIGHUP  ----
2026-10-16T22:18:30.475542Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:30.476493Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:53760 -> 127.0.0.1:53759 (requires restart)
2026-10-16T22:18:30.477320Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s7-env-override-no-false-positive / 报成功

```
[harness] ---- SIGHUP  ----
2026-10-16T22:18:30.475542Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:30.476493Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:53760 -> 127.0.0.1:53759 (requires restart)
2026-10-16T22:18:30.477320Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s8-absent-connectors / 第1次：点名 okx.subscribe_klines 未能应用

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:18:35.270914Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:35.272702Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T22:18:35.273844Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T22:18:35.273931Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第1次：点名 okx.subscribe_quotes 未能应用

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:18:35.270914Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:35.272702Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T22:18:35.273844Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T22:18:35.273931Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第1次：给出"需要重启"，不报成功

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:18:35.270914Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:35.272702Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T22:18:35.273844Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T22:18:35.273931Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：点名 okx.subscribe_klines 未能应用

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:18:35.919634Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:35.920374Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：点名 okx.subscribe_quotes 未能应用

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:18:35.919634Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:35.920374Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：给出"需要重启"，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:18:35.919634Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:35.920374Z  INFO md_server: config reloaded and applied successfully
```

### s9-log-level-not-claimed-applied / 不得写"(applied)"

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:18:40.677197Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:40.679363Z  INFO md_server:   log_level: info -> error (applied)
2026-10-16T22:18:40.679918Z  INFO md_server: config reloaded and applied successfully
```

### s9-log-level-not-claimed-applied / 不报成功

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:18:40.677197Z  INFO md_server: received SIGHUP, reloading config from /cfg/config.yaml
2026-10-16T22:18:40.679363Z  INFO md_server:   log_level: info -> error (applied)
2026-10-16T22:18:40.679918Z  INFO md_server: config reloaded and applied successfully
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
