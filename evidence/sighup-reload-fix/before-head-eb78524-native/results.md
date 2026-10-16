# SIGHUP 重载端到端结果

runner=native bin=/tmp/md-server-prefix-head  时间=2026-10-16T22:17:26.550Z

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
2026-10-16T22:16:22.585001Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxPublic-Xo1uS8/config.yaml
2026-10-16T22:16:22.585522Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxPublic / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:22.585001Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxPublic-Xo1uS8/config.yaml
2026-10-16T22:16:22.585522Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxPublic / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:22.585001Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxPublic-Xo1uS8/config.yaml
2026-10-16T22:16:22.585522Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：检测到 OKX Business 地址变更并点名

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:16:26.203242Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-nhFCNl/config.yaml
2026-10-16T22:16:26.204461Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:16:26.203242Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-nhFCNl/config.yaml
2026-10-16T22:16:26.204461Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:16:26.203242Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-nhFCNl/config.yaml
2026-10-16T22:16:26.204461Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：检测到 OKX Business 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:26.708320Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-nhFCNl/config.yaml
2026-10-16T22:16:26.709558Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:26.708320Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-nhFCNl/config.yaml
2026-10-16T22:16:26.709558Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:26.708320Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-nhFCNl/config.yaml
2026-10-16T22:16:26.709558Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：检测到 Binance Market(tick/kline) 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:30.828061Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binanceMarket-uszK73/config.yaml
2026-10-16T22:16:30.829016Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:30.828061Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binanceMarket-uszK73/config.yaml
2026-10-16T22:16:30.829016Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:30.828061Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binanceMarket-uszK73/config.yaml
2026-10-16T22:16:30.829016Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：检测到 Binance Public(quote) 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:34.950444Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binancePublic-IIf6uv/config.yaml
2026-10-16T22:16:34.951300Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:34.950444Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binancePublic-IIf6uv/config.yaml
2026-10-16T22:16:34.951300Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:34.950444Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binancePublic-IIf6uv/config.yaml
2026-10-16T22:16:34.951300Z  INFO md_server: config reloaded and applied successfully
```

### s4-subs-hot-update-with-pending-url / 地址变更仍被点名为待重启，不报成功

```
2026-10-16T22:16:53.397630Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T22:16:53.397916Z  INFO md_server:   binance: adding 1 subscriptions
2026-10-16T22:16:53.397994Z  INFO md_server:   okx: adding 1 tick subscriptions
2026-10-16T22:16:53.398029Z  INFO md_server:   okx: removing 1 tick subscriptions
2026-10-16T22:16:53.398065Z  INFO md_server:   okx-kline: adding 1 kline subscriptions
2026-10-16T22:16:53.398093Z  INFO md_server: config reloaded and applied successfully
```

### s4-subs-hot-update-with-pending-url / 第二次 SIGHUP 仍提示待重启，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:16:53.898024Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s4-subs-hot-update-with-pending-url-vRMRlU/config.yaml
2026-10-16T22:16:53.898623Z  INFO md_server: config reloaded and applied successfully
```

### s6-port-offset-no-false-positive / 不误报 listen_address 变更

```
[harness] ---- SIGHUP  ----
2026-10-16T22:17:02.653774Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s6-port-offset-no-false-positive-emEzOr/config.yaml
2026-10-16T22:17:02.665082Z  WARN md_server:   grpc_server.listen_address changed: 127.0.0.1:24906 -> 127.0.0.1:24896 (requires restart)
2026-10-16T22:17:02.665148Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:27277 -> 127.0.0.1:27267 (requires restart)
2026-10-16T22:17:02.665177Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s6-port-offset-no-false-positive / 报成功

```
[harness] ---- SIGHUP  ----
2026-10-16T22:17:02.653774Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s6-port-offset-no-false-positive-emEzOr/config.yaml
2026-10-16T22:17:02.665082Z  WARN md_server:   grpc_server.listen_address changed: 127.0.0.1:24906 -> 127.0.0.1:24896 (requires restart)
2026-10-16T22:17:02.665148Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:27277 -> 127.0.0.1:27267 (requires restart)
2026-10-16T22:17:02.665177Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s7-env-override-no-false-positive / 不误报 api_gateway.listen_address 变更

```
[harness] ---- SIGHUP  ----
2026-10-16T22:17:06.267316Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s7-env-override-no-false-positive-tiCxjG/config.yaml
2026-10-16T22:17:06.267915Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:53362 -> 127.0.0.1:53361 (requires restart)
2026-10-16T22:17:06.267931Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s7-env-override-no-false-positive / 报成功

```
[harness] ---- SIGHUP  ----
2026-10-16T22:17:06.267316Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s7-env-override-no-false-positive-tiCxjG/config.yaml
2026-10-16T22:17:06.267915Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:53362 -> 127.0.0.1:53361 (requires restart)
2026-10-16T22:17:06.267931Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s8-absent-connectors / 第1次：点名 okx.subscribe_klines 未能应用

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:17:09.881007Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-kVd8An/config.yaml
2026-10-16T22:17:09.881840Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T22:17:09.881871Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T22:17:09.881886Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第1次：点名 okx.subscribe_quotes 未能应用

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:17:09.881007Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-kVd8An/config.yaml
2026-10-16T22:17:09.881840Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T22:17:09.881871Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T22:17:09.881886Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第1次：给出"需要重启"，不报成功

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:17:09.881007Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-kVd8An/config.yaml
2026-10-16T22:17:09.881840Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T22:17:09.881871Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T22:17:09.881886Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：点名 okx.subscribe_klines 未能应用

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:10.382661Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-kVd8An/config.yaml
2026-10-16T22:17:10.383350Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：点名 okx.subscribe_quotes 未能应用

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:10.382661Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-kVd8An/config.yaml
2026-10-16T22:17:10.383350Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：给出"需要重启"，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-16T22:17:10.382661Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-kVd8An/config.yaml
2026-10-16T22:17:10.383350Z  INFO md_server: config reloaded and applied successfully
```

### s9-log-level-not-claimed-applied / 不得写"(applied)"

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:17:14.001281Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9-log-level-not-claimed-applied-f0Alll/config.yaml
2026-10-16T22:17:14.001943Z  INFO md_server:   log_level: info -> error (applied)
2026-10-16T22:17:14.001969Z  INFO md_server: config reloaded and applied successfully
```

### s9-log-level-not-claimed-applied / 不报成功

```
[harness] ---- SIGHUP #1 ----
2026-10-16T22:17:14.001281Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9-log-level-not-claimed-applied-f0Alll/config.yaml
2026-10-16T22:17:14.001943Z  INFO md_server:   log_level: info -> error (applied)
2026-10-16T22:17:14.001969Z  INFO md_server: config reloaded and applied successfully
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
