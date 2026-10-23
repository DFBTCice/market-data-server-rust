# SIGHUP 重载端到端结果

runner=native bin=/tmp/md-server-prefix-head  时间=2026-10-16T23:20:58.704Z

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
2026-10-16T23:19:08.098668Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxPublic-cgSoAe/config.yaml
2026-10-16T23:19:08.099166Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxPublic / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:08.098668Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxPublic-cgSoAe/config.yaml
2026-10-16T23:19:08.099166Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxPublic / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:08.098668Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxPublic-cgSoAe/config.yaml
2026-10-16T23:19:08.099166Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：检测到 OKX Business 地址变更并点名

```
[harness] ---- SIGHUP #1 ----
2026-10-16T23:19:11.713301Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-P9n50L/config.yaml
2026-10-16T23:19:11.713846Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #1 ----
2026-10-16T23:19:11.713301Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-P9n50L/config.yaml
2026-10-16T23:19:11.713846Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第1次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #1 ----
2026-10-16T23:19:11.713301Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-P9n50L/config.yaml
2026-10-16T23:19:11.713846Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：检测到 OKX Business 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:12.215503Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-P9n50L/config.yaml
2026-10-16T23:19:12.216048Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:12.215503Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-P9n50L/config.yaml
2026-10-16T23:19:12.216048Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-okxBusiness / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:12.215503Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-okxBusiness-P9n50L/config.yaml
2026-10-16T23:19:12.216048Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：检测到 Binance Market(tick/kline) 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:16.336302Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binanceMarket-y4DdJt/config.yaml
2026-10-16T23:19:16.337287Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:16.336302Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binanceMarket-y4DdJt/config.yaml
2026-10-16T23:19:16.337287Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binanceMarket / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:16.336302Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binanceMarket-y4DdJt/config.yaml
2026-10-16T23:19:16.337287Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：检测到 Binance Public(quote) 地址变更并点名

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:20.453942Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binancePublic-2jmUPC/config.yaml
2026-10-16T23:19:20.454525Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：给出"需要重启"的结论

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:20.453942Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binancePublic-2jmUPC/config.yaml
2026-10-16T23:19:20.454525Z  INFO md_server: config reloaded and applied successfully
```

### s1-url-binancePublic / 第2次：不得报告"已成功应用"

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:20.453942Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s1-url-binancePublic-2jmUPC/config.yaml
2026-10-16T23:19:20.454525Z  INFO md_server: config reloaded and applied successfully
```

### s4-subs-hot-update-with-pending-url / 地址变更仍被点名为待重启，不报成功

```
2026-10-16T23:19:38.948151Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T23:19:38.948420Z  INFO md_server:   binance: adding 1 subscriptions
2026-10-16T23:19:38.948527Z  INFO md_server:   okx: adding 1 tick subscriptions
2026-10-16T23:19:38.948565Z  INFO md_server:   okx: removing 1 tick subscriptions
2026-10-16T23:19:38.948599Z  INFO md_server:   okx-kline: adding 1 kline subscriptions
2026-10-16T23:19:38.948624Z  INFO md_server: config reloaded and applied successfully
```

### s4-subs-hot-update-with-pending-url / 第二次 SIGHUP 仍提示待重启，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:39.450044Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s4-subs-hot-update-with-pending-url-EL3gXW/config.yaml
2026-10-16T23:19:39.450727Z  INFO md_server: config reloaded and applied successfully
```

### s6-port-offset-no-false-positive / 不误报 listen_address 变更

```
[harness] ---- SIGHUP  ----
2026-10-16T23:19:48.190116Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s6-port-offset-no-false-positive-Xq1gJl/config.yaml
2026-10-16T23:19:48.190733Z  WARN md_server:   grpc_server.listen_address changed: 127.0.0.1:34847 -> 127.0.0.1:34837 (requires restart)
2026-10-16T23:19:48.190746Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:39177 -> 127.0.0.1:39167 (requires restart)
2026-10-16T23:19:48.190757Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s6-port-offset-no-false-positive / 报成功

```
[harness] ---- SIGHUP  ----
2026-10-16T23:19:48.190116Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s6-port-offset-no-false-positive-Xq1gJl/config.yaml
2026-10-16T23:19:48.190733Z  WARN md_server:   grpc_server.listen_address changed: 127.0.0.1:34847 -> 127.0.0.1:34837 (requires restart)
2026-10-16T23:19:48.190746Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:39177 -> 127.0.0.1:39167 (requires restart)
2026-10-16T23:19:48.190757Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s7-env-override-no-false-positive / 不误报 api_gateway.listen_address 变更

```
[harness] ---- SIGHUP  ----
2026-10-16T23:19:51.811203Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s7-env-override-no-false-positive-wzRf2M/config.yaml
2026-10-16T23:19:51.811775Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:64560 -> 127.0.0.1:64559 (requires restart)
2026-10-16T23:19:51.811794Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s7-env-override-no-false-positive / 报成功

```
[harness] ---- SIGHUP  ----
2026-10-16T23:19:51.811203Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s7-env-override-no-false-positive-wzRf2M/config.yaml
2026-10-16T23:19:51.811775Z  WARN md_server:   api_gateway.listen_address changed: 127.0.0.1:64560 -> 127.0.0.1:64559 (requires restart)
2026-10-16T23:19:51.811794Z  WARN md_server: some config changes require connector restart -- please restart the service
```

### s8-absent-connectors / 第1次：点名 okx.subscribe_klines 未能应用

```
[harness] ---- SIGHUP #1 ----
2026-10-16T23:19:55.427109Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-Wx4GoP/config.yaml
2026-10-16T23:19:55.427716Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T23:19:55.427760Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T23:19:55.427779Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第1次：点名 okx.subscribe_quotes 未能应用

```
[harness] ---- SIGHUP #1 ----
2026-10-16T23:19:55.427109Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-Wx4GoP/config.yaml
2026-10-16T23:19:55.427716Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T23:19:55.427760Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T23:19:55.427779Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第1次：给出"需要重启"，不报成功

```
[harness] ---- SIGHUP #1 ----
2026-10-16T23:19:55.427109Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-Wx4GoP/config.yaml
2026-10-16T23:19:55.427716Z  INFO md_server:   subscription lists changed, applying...
2026-10-16T23:19:55.427760Z  WARN md_server:   okx: subscribe_quotes changed but quote connector is not running (quotes_enabled=false or list was empty at startup) -- requires restart
2026-10-16T23:19:55.427779Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：点名 okx.subscribe_klines 未能应用

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:55.930024Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-Wx4GoP/config.yaml
2026-10-16T23:19:55.930524Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：点名 okx.subscribe_quotes 未能应用

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:55.930024Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-Wx4GoP/config.yaml
2026-10-16T23:19:55.930524Z  INFO md_server: config reloaded and applied successfully
```

### s8-absent-connectors / 第2次：给出"需要重启"，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:19:55.930024Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s8-absent-connectors-Wx4GoP/config.yaml
2026-10-16T23:19:55.930524Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / 启动日志说明过滤器来自配置的 log_level

```
2026-10-16T23:19:58.447540Z  INFO md_connector::base: [binance-quote] connected and synced
2026-10-16T23:19:58.447546Z  INFO md_connector::base: [okx-quote] subscription sync complete
2026-10-16T23:19:58.447552Z  INFO md_connector::base: [okx-quote] connected and synced
2026-10-16T23:19:58.447540Z  INFO md_connector::base: [okx] subscription sync complete
2026-10-16T23:19:58.447571Z  INFO md_connector::base: [okx] connected and synced
[harness] ---- process ready ----
```

### s9a-log-level-hot-apply / info→warn：点名 log_level 变更并开始应用

```
[harness] ---- SIGHUP info->warn ----
2026-10-16T23:19:59.542669Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9a-log-level-hot-apply-rfFnvT/config.yaml
2026-10-16T23:19:59.543300Z  INFO md_server:   log_level: info -> warn (applied)
2026-10-16T23:19:59.543326Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / warn 级别：无变更 SIGHUP 没有任何 INFO 输出（INFO 已被过滤）

```
[harness] ---- SIGHUP warn, no change ----
2026-10-16T23:20:01.145206Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9a-log-level-hot-apply-rfFnvT/config.yaml
2026-10-16T23:20:01.145913Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / warn 级别：WARN 仍可见（待重启提示照常输出）

```
[harness] ---- SIGHUP warn, url pending ----
2026-10-16T23:20:02.747581Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9a-log-level-hot-apply-rfFnvT/config.yaml
2026-10-16T23:20:02.748157Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / error 级别：同样的待重启情形不再输出 WARN（对照上一步 warn 级别可见）

```
[harness] ---- SIGHUP warn->error ----
2026-10-16T23:20:04.349476Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9a-log-level-hot-apply-rfFnvT/config.yaml
2026-10-16T23:20:04.349949Z  INFO md_server:   log_level: warn -> error (applied)
2026-10-16T23:20:04.349968Z  INFO md_server: config reloaded and applied successfully[harness] ---- SIGHUP error, same url pending ----
2026-10-16T23:20:05.950926Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9a-log-level-hot-apply-rfFnvT/config.yaml
2026-10-16T23:20:05.951587Z  INFO md_server: config reloaded and applied successfully
```

### s9a-log-level-hot-apply / error 级别：ERROR 仍可见（YAML 解析失败照常报错）

```
[harness] ---- SIGHUP error, bad yaml ----
2026-10-16T23:20:07.552926Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9a-log-level-hot-apply-rfFnvT/config.yaml
2026-10-16T23:20:07.553106Z  WARN md_server: failed to reload config: Parse error: did not find expected ',' or ']' at line 2 column 1, while parsing a flow sequence at line 1 column 13, keeping current config
```

### s9a-log-level-hot-apply / error→info：确认已应用

```
[harness] ---- SIGHUP error->info ----
2026-10-16T23:20:09.157191Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9a-log-level-hot-apply-rfFnvT/config.yaml
2026-10-16T23:20:09.157833Z  INFO md_server:   log_level: error -> info (applied)
2026-10-16T23:20:09.157851Z  INFO md_server: config reloaded and applied successfully
```

### s9b-log-level-from-config-at-startup / 启动阶段没有任何 INFO 日志（旧版本这里会输出全部 INFO）

```
2026-10-16T23:20:12.178001Z  INFO md_connector::base: [binance-quote] subscription sync complete
2026-10-16T23:20:12.178007Z  INFO md_connector::base: [binance-quote] connected and synced
2026-10-16T23:20:12.178008Z  INFO md_connector::base: [binance] subscription sync complete
2026-10-16T23:20:12.178011Z  INFO md_connector::base: [okx] subscription sync complete
2026-10-16T23:20:12.178026Z  INFO md_connector::base: [okx] connected and synced
2026-10-16T23:20:12.178015Z  INFO md_connector::base: [binance] connected and synced
```

### s9b-log-level-from-config-at-startup / 无变更 SIGHUP：安静

```
[harness] ---- SIGHUP no change ----
2026-10-16T23:20:13.271764Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9b-log-level-from-config-at-startup-iYlyWz/config.yaml
2026-10-16T23:20:13.272474Z  INFO md_server: config reloaded and applied successfully
```

### s9b-log-level-from-config-at-startup / warn→info：确认已应用

```
[harness] ---- SIGHUP warn->info ----
2026-10-16T23:20:14.874128Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9b-log-level-from-config-at-startup-iYlyWz/config.yaml
2026-10-16T23:20:14.875415Z  INFO md_server:   log_level: warn -> info (applied)
2026-10-16T23:20:14.875469Z  INFO md_server: config reloaded and applied successfully
```

### s9c-rust-log-takes-precedence / 启动：WARN 点名 YAML 的 log_level 被 RUST_LOG 覆盖

```
2026-10-16T23:20:17.407500Z  INFO md_connector::base: [okx-quote] connected and synced
2026-10-16T23:20:17.407502Z  INFO md_connector::base: [okx] connected and synced
2026-10-16T23:20:17.407535Z  INFO md_connector::base: [binance] subscribing to 2 streams
2026-10-16T23:20:17.407565Z  INFO md_connector::base: [binance] subscription sync complete
2026-10-16T23:20:17.407572Z  INFO md_connector::base: [binance] connected and synced
[harness] ---- process ready ----
```

### s9c-rust-log-takes-precedence / 第1次：点名 log_level 未生效，并说明被 RUST_LOG 覆盖

```
[harness] ---- SIGHUP #1 ----
2026-10-16T23:20:18.502600Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9c-rust-log-takes-precedence-NYil7Q/config.yaml
2026-10-16T23:20:18.503160Z  INFO md_server:   log_level: debug -> warn (applied)
2026-10-16T23:20:18.503174Z  INFO md_server: config reloaded and applied successfully
```

### s9c-rust-log-takes-precedence / 第1次：给出"需要重启"的结论，不报成功

```
[harness] ---- SIGHUP #1 ----
2026-10-16T23:20:18.502600Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9c-rust-log-takes-precedence-NYil7Q/config.yaml
2026-10-16T23:20:18.503160Z  INFO md_server:   log_level: debug -> warn (applied)
2026-10-16T23:20:18.503174Z  INFO md_server: config reloaded and applied successfully
```

### s9c-rust-log-takes-precedence / 第2次：点名 log_level 未生效，并说明被 RUST_LOG 覆盖

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:20:19.004408Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9c-rust-log-takes-precedence-NYil7Q/config.yaml
2026-10-16T23:20:19.005212Z  INFO md_server: config reloaded and applied successfully
```

### s9c-rust-log-takes-precedence / 第2次：给出"需要重启"的结论，不报成功

```
[harness] ---- SIGHUP #2 ----
2026-10-16T23:20:19.004408Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9c-rust-log-takes-precedence-NYil7Q/config.yaml
2026-10-16T23:20:19.005212Z  INFO md_server: config reloaded and applied successfully
```

### s9d-invalid-log-level-rejected / 启动：进程自己退出，退出码为 1（不是跑到超时被杀）

```
code=101 timedOut=true
```

### s9d-invalid-log-level-rejected / 启动：报错点名 log_level 和错误值

```
2026-10-16T23:20:22.026458Z  INFO md_server: loading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9d-invalid-log-level-rejected-v1zic8/config.yaml
2026-10-16T23:20:22.027051Z  INFO md_server: log_level: verbose
2026-10-16T23:20:22.027060Z  INFO md_server: gRPC: 127.0.0.1:64716
2026-10-16T23:20:22.027067Z  INFO md_server: API Gateway: 127.0.0.1:64718
2026-10-16T23:20:22.027146Z  INFO md_server: starting Binance connector
2026-10-16T23:20:22.027270Z  INFO md_server: starting OKX Public connector (trades)
2026-10-16T23:20:22.027302Z  INFO md_server: starting OKX Business connector (candles)
2026-10-16T23:20:22.027311Z  INFO md_connector::base: [binance] connecting to ws://127.0.0.1:64708/ws
2026-10-16T23:20:22.027326Z  INFO md_connector::base: [okx] connecting to ws://127.0.0.1:64712/ws
2026-10-16T23:20:22.027339Z  INFO md_server: starting Binance quote connector (bookTicker) -> ws://127.0.0.1:64710/ws
2026-10-16T23:20:22.027372Z  INFO md_server: starting OKX quote connector (tickers) -> ws://127.0.0.1:64712/ws
2026-10-16T23:20:22.027371Z  INFO md_connector::base: [okx] connecting to ws://127.0.0.1:64714/ws
2026-10-16T23:20:22.027421Z  INFO md_connector::base: [okx-quote] connecting to ws://127.0.0.1:64712/ws
2026-10-16T23:20:22.027418Z  INFO md_connector::base: [binance-quote] connecting to ws://127.0.0.1:64710/ws
2026-10-16T23:20:22.027514Z  INFO md_processor: processor dispatch loop started
2026-10-16T23:20:22.027589Z  INFO md_server: gRPC server listening on 127.0.0.1:64716
2026-10-16T23:20:22.027776Z  INFO md_server: API Gateway listening on 127.0.0.1:64718

```

### s9d-invalid-log-level-rejected / 重载：明确报错并点名 log_level

```
[harness] ---- SIGHUP bad level ----
2026-10-16T23:20:31.885547Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9d-invalid-log-level-rejected-v1zic8/config.yaml
2026-10-16T23:20:31.888319Z  INFO md_server:   log_level: info -> verbose (applied)
2026-10-16T23:20:31.888395Z  INFO md_server: config reloaded and applied successfully
```

### s9d-invalid-log-level-rejected / 重载：声明保留运行配置

```
[harness] ---- SIGHUP bad level ----
2026-10-16T23:20:31.885547Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9d-invalid-log-level-rejected-v1zic8/config.yaml
2026-10-16T23:20:31.888319Z  INFO md_server:   log_level: info -> verbose (applied)
2026-10-16T23:20:31.888395Z  INFO md_server: config reloaded and applied successfully
```

### s9d-invalid-log-level-rejected / 重载：不报成功

```
[harness] ---- SIGHUP bad level ----
2026-10-16T23:20:31.885547Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9d-invalid-log-level-rejected-v1zic8/config.yaml
2026-10-16T23:20:31.888319Z  INFO md_server:   log_level: info -> verbose (applied)
2026-10-16T23:20:31.888395Z  INFO md_server: config reloaded and applied successfully
```

### s9e-log-level-env-override / 启动：按 LOG_LEVEL=warn 生效（没有 INFO 日志）

```
2026-10-16T23:20:34.938234Z  INFO md_connector::base: [okx-quote] subscribing to 1 streams
2026-10-16T23:20:34.938254Z  INFO md_connector::base: [okx-quote] subscription sync complete
2026-10-16T23:20:34.938258Z  INFO md_connector::base: [okx-quote] connected and synced
2026-10-16T23:20:34.938265Z  INFO md_connector::base: [okx] subscribing to 1 streams
2026-10-16T23:20:34.938287Z  INFO md_connector::base: [okx] subscription sync complete
2026-10-16T23:20:34.938292Z  INFO md_connector::base: [okx] connected and synced
```

### s9e-log-level-env-override / 无变更 SIGHUP：安静，不误报 log_level 变更

```
[harness] ---- SIGHUP no change ----
2026-10-16T23:20:36.032359Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9e-log-level-env-override-UH3VyG/config.yaml
2026-10-16T23:20:36.032886Z  INFO md_server:   log_level: warn -> info (applied)
2026-10-16T23:20:36.032901Z  INFO md_server: config reloaded and applied successfully
```

### s9e-log-level-env-override / YAML 改动被环境变量覆盖：生效配置不变，不输出任何 INFO/WARN/ERROR

```
[harness] ---- SIGHUP yaml->debug ----
2026-10-16T23:20:37.637403Z  INFO md_server: received SIGHUP, reloading config from /var/folders/29/5kp2s02d5t3ft__lf9f_3qsh0000gn/T/sighup-s9e-log-level-env-override-UH3VyG/config.yaml
2026-10-16T23:20:37.637945Z  INFO md_server:   log_level: info -> debug (applied)
2026-10-16T23:20:37.637979Z  INFO md_server: config reloaded and applied successfully
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
