# SIGHUP 重载端到端结果

runner=docker image=md-server:probe-final-a06fe20  时间=2026-10-23T17:35:11.097Z

共 129 条断言，通过 129，失败 0。

| 场景 | 断言 | 结果 |
|---|---|---|
| s00-noop | 报告成功 | PASS |
| s00-noop | 无重启提示 | PASS |
| s1-url-okxPublic | 第1次：检测到 OKX Public 地址变更并点名 | PASS |
| s1-url-okxPublic | 第1次：给出"需要重启"的结论 | PASS |
| s1-url-okxPublic | 第1次：不得报告"已成功应用" | PASS |
| s1-url-okxPublic | 第2次：检测到 OKX Public 地址变更并点名 | PASS |
| s1-url-okxPublic | 第2次：给出"需要重启"的结论 | PASS |
| s1-url-okxPublic | 第2次：不得报告"已成功应用" | PASS |
| s1-url-okxPublic | 旧上游连接保持、新上游无连接（地址未被热切换） | PASS |
| s1-url-okxBusiness | 第1次：检测到 OKX Business 地址变更并点名 | PASS |
| s1-url-okxBusiness | 第1次：给出"需要重启"的结论 | PASS |
| s1-url-okxBusiness | 第1次：不得报告"已成功应用" | PASS |
| s1-url-okxBusiness | 第2次：检测到 OKX Business 地址变更并点名 | PASS |
| s1-url-okxBusiness | 第2次：给出"需要重启"的结论 | PASS |
| s1-url-okxBusiness | 第2次：不得报告"已成功应用" | PASS |
| s1-url-okxBusiness | 旧上游连接保持、新上游无连接（地址未被热切换） | PASS |
| s1-url-binanceMarket | 第1次：检测到 Binance Market(tick/kline) 地址变更并点名 | PASS |
| s1-url-binanceMarket | 第1次：给出"需要重启"的结论 | PASS |
| s1-url-binanceMarket | 第1次：不得报告"已成功应用" | PASS |
| s1-url-binanceMarket | 第2次：检测到 Binance Market(tick/kline) 地址变更并点名 | PASS |
| s1-url-binanceMarket | 第2次：给出"需要重启"的结论 | PASS |
| s1-url-binanceMarket | 第2次：不得报告"已成功应用" | PASS |
| s1-url-binanceMarket | 旧上游连接保持、新上游无连接（地址未被热切换） | PASS |
| s1-url-binancePublic | 第1次：检测到 Binance Public(quote) 地址变更并点名 | PASS |
| s1-url-binancePublic | 第1次：给出"需要重启"的结论 | PASS |
| s1-url-binancePublic | 第1次：不得报告"已成功应用" | PASS |
| s1-url-binancePublic | 第2次：检测到 Binance Public(quote) 地址变更并点名 | PASS |
| s1-url-binancePublic | 第2次：给出"需要重启"的结论 | PASS |
| s1-url-binancePublic | 第2次：不得报告"已成功应用" | PASS |
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
| s4-subs-hot-update-with-pending-url | 地址变更仍被点名为待重启，不报成功 | PASS |
| s4-subs-hot-update-with-pending-url | 第二次 SIGHUP 不重复下发订阅帧 | PASS |
| s4-subs-hot-update-with-pending-url | 第二次 SIGHUP 仍提示待重启，不报成功 | PASS |
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
| s6-port-offset-no-false-positive | 不误报 listen_address 变更 | PASS |
| s6-port-offset-no-false-positive | 报成功 | PASS |
| s7-env-override-no-false-positive | 不误报 api_gateway.listen_address 变更 | PASS |
| s7-env-override-no-false-positive | 报成功 | PASS |
| s8-absent-connectors | 第1次：点名 okx.subscribe_klines 未能应用 | PASS |
| s8-absent-connectors | 第1次：点名 okx.subscribe_quotes 未能应用 | PASS |
| s8-absent-connectors | 第1次：给出"需要重启"，不报成功 | PASS |
| s8-absent-connectors | 第2次：点名 okx.subscribe_klines 未能应用 | PASS |
| s8-absent-connectors | 第2次：点名 okx.subscribe_quotes 未能应用 | PASS |
| s8-absent-connectors | 第2次：给出"需要重启"，不报成功 | PASS |
| s9a-log-level-hot-apply | 启动日志说明过滤器来自配置的 log_level | PASS |
| s9a-log-level-hot-apply | info→warn：点名 log_level 变更并开始应用 | PASS |
| s9a-log-level-hot-apply | warn 级别：无变更 SIGHUP 没有任何 INFO 输出（INFO 已被过滤） | PASS |
| s9a-log-level-hot-apply | warn 级别：WARN 仍可见（待重启提示照常输出） | PASS |
| s9a-log-level-hot-apply | error 级别：同样的待重启情形不再输出 WARN（对照上一步 warn 级别可见） | PASS |
| s9a-log-level-hot-apply | error 级别：ERROR 仍可见（YAML 解析失败照常报错） | PASS |
| s9a-log-level-hot-apply | error→info：确认已应用 | PASS |
| s9a-log-level-hot-apply | error→info：此后报成功（地址已改回，无待处理项） | PASS |
| s9a-log-level-hot-apply | info 恢复：无变更 SIGHUP 的 INFO 日志重新可见 | PASS |
| s9a-log-level-hot-apply | 全程没有重连 / 断线（热更，不重启） | PASS |
| s9b-log-level-from-config-at-startup | 启动阶段没有任何 INFO 日志（旧版本这里会输出全部 INFO） | PASS |
| s9b-log-level-from-config-at-startup | 连接器照常连上（只是日志安静） | PASS |
| s9b-log-level-from-config-at-startup | 无变更 SIGHUP：安静 | PASS |
| s9b-log-level-from-config-at-startup | warn→info：确认已应用 | PASS |
| s9b-log-level-from-config-at-startup | warn→info：此后 INFO 可见（结论行报成功） | PASS |
| s9c-rust-log-takes-precedence | 启动：WARN 点名 YAML 的 log_level 被 RUST_LOG 覆盖 | PASS |
| s9c-rust-log-takes-precedence | 启动：实际生效的是 RUST_LOG（有 INFO、没有 DEBUG） | PASS |
| s9c-rust-log-takes-precedence | 第1次：点名 log_level 未生效，并说明被 RUST_LOG 覆盖 | PASS |
| s9c-rust-log-takes-precedence | 第1次：给出"需要重启"的结论，不报成功 | PASS |
| s9c-rust-log-takes-precedence | 第1次：过滤器没有被改动（INFO 仍可见，也没有"开始应用"） | PASS |
| s9c-rust-log-takes-precedence | 第2次：点名 log_level 未生效，并说明被 RUST_LOG 覆盖 | PASS |
| s9c-rust-log-takes-precedence | 第2次：给出"需要重启"的结论，不报成功 | PASS |
| s9c-rust-log-takes-precedence | 第2次：过滤器没有被改动（INFO 仍可见，也没有"开始应用"） | PASS |
| s9c-rust-log-takes-precedence | 改回启动时的值：待处理项消失，报成功 | PASS |
| s9d-invalid-log-level-rejected | 启动：进程自己退出，退出码为 1（不是跑到超时被杀） | PASS |
| s9d-invalid-log-level-rejected | 启动：报错点名 log_level 和错误值 | PASS |
| s9d-invalid-log-level-rejected | 重载：明确报错并点名 log_level | PASS |
| s9d-invalid-log-level-rejected | 重载：声明保留运行配置 | PASS |
| s9d-invalid-log-level-rejected | 重载：不报成功 | PASS |
| s9d-invalid-log-level-rejected | 重载：进程仍在运行 | PASS |
| s9d-invalid-log-level-rejected | 修好后：报成功，且 INFO 仍可见（运行中的日志级别没有被坏配置改动） | PASS |
| s9e-log-level-env-override | 启动：按 LOG_LEVEL=warn 生效（没有 INFO 日志） | PASS |
| s9e-log-level-env-override | 无变更 SIGHUP：安静，不误报 log_level 变更 | PASS |
| s9e-log-level-env-override | YAML 改动被环境变量覆盖：生效配置不变，不输出任何 INFO/WARN/ERROR | PASS |
| s9f-prod-198-combination | 启动：生效的仍是 RUST_LOG=warn（没有 INFO 日志，行为与升级前一致） | PASS |
| s9f-prod-198-combination | 启动：WARN 明确说明 YAML 的 log_level 被 RUST_LOG 覆盖 | PASS |
| s9f-prod-198-combination | 无变更 SIGHUP：安静 | PASS |
| s9f-prod-198-combination | 第1次：WARN 级别可见，点名 log_level 未生效并说明 RUST_LOG=warn 优先 | PASS |
| s9f-prod-198-combination | 第1次：给出"需要重启"的结论 | PASS |
| s9f-prod-198-combination | 第2次：WARN 级别可见，点名 log_level 未生效并说明 RUST_LOG=warn 优先 | PASS |
| s9f-prod-198-combination | 第2次：给出"需要重启"的结论 | PASS |
| s10-prod-log-level-warn | 无变更：没有 WARN/ERROR 输出 | PASS |
| s10-prod-log-level-warn | 仅改 OKX Business 地址，第1次：WARN 级别可见并点名 | PASS |
| s10-prod-log-level-warn | 仅改 OKX Business 地址，第1次：WARN 级别给出需要重启的结论 | PASS |
| s10-prod-log-level-warn | 仅改 OKX Business 地址，第2次：WARN 级别可见并点名 | PASS |
| s10-prod-log-level-warn | 仅改 OKX Business 地址，第2次：WARN 级别给出需要重启的结论 | PASS |
| s10-prod-log-level-warn | YAML 解析失败：WARN/ERROR 级别可见 | PASS |
