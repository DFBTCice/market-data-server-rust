# 198 换版计划：1.1.1 → 1.2.0（开关打开、报价补订关闭）

依据：主审复核意见（2026-11-06）§四、§五与业务方决定；换版前修正（2026-11-07）§二、§四；换版前最后一项（2026-11-07）§二–§四。
**主审核对本轮修正通过后，业务方批准即可执行（不限时段）**；执行前把开始时间告诉业务方（业务方开 Portainer、botd 保持运行、换版后在场约 15 分钟）。
**避开 OMS 部署窗口**：OMS 是 198 行情的另一个消费者，开始前请业务方确认 OMS 已稳定连上（`md_ws_active_clients` 为 2，主审可只读核对）；
消费者不足 2 个时脚本会拒绝换版、不动容器，换后 90 秒内等不到 2 个会自动回退。

## 1. 产物与镜像

| 项 | 值 |
|---|---|
| 产物 | `dist/md-server-1.2.0-12192d9-linux-x86_64`，sha256 固定为 `cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf`（`swaplib.EXPECTED_SHA`，与 `build-logs/dist-sha256.txt` 交叉核对；脚本不接受外部传入的产物或 sha） |
| 源码 | `42bc897`（产物从 `12192d9` 构建，源码相同）；之后只有测试与工具提交，产品代码未改 |
| 构建特性 | `--features md-connector/vendored-tls,md-server/jemalloc-stats`，两次从零构建 sha 一致 |
| 镜像 | `md-server:1.2.0` = `md-server:1.0.0-rollback`（`06907bdf7fd8`）+ 产物 + `config.release.yaml`；`build_release_image.py`（无参数）构建并写 `image-manifest.json` |
| 配置 | `config.release.yaml` = 现网 `config.final.yaml` + `runtime_subscriptions`（开关打开、闲置 10 分钟、保护 10 分钟、检查 60 秒、上限 200、`on_demand_quote: false`）；diff 见 `config.release.diff` |
| 容器环境 | `RUST_LOG=warn,md_runtime_subs=info`、`TZ=Asia/Shanghai`；端口、内存 512 MiB、CPU、重启策略、日志配置沿用旧容器；复制旧容器标签时去掉镜像来源标签（`md-server.binary.sha256`、`md-server.config.sha256`、`org.opencontainers.image.*`） |

## 2. 前置条件（`execute_swap.py` 在动任何容器之前逐条断言）

1. 金丝雀第 1 部分：`canary/gate-results.json` 里 push / subs / metrics / noquote / selftest 全部 0（quote 只允许已裁定的 BNB 数量噪声）。
2. 金丝雀第 2 部分以 `canary-rerun/` 为准：`canary-rerun/short/summary.json`、`canary-rerun/prod/summary.json` 都是 `pass: true`（含报价话题未被补订）。
3. 第一次运行 `canary/{short,prod}/summary.json` 的失败项恰好只有 `config_subscriptions_kept`，并当场用 `recheck_config_kept.mjs`（按 `source=config` 判，带正对照）复核两份时间线通过。
4. 固定期望 sha：常量 = `dist-sha256.txt` 记录 = 本地产物文件 = `image-manifest.json` = 镜像标签。
5. 现网容器 `md-server` 镜像 ID = `sha256:3250e021…f010`（1.1.1）且在运行；`md-server-1.1.1-bak` 不存在。
6. 现网 `md_ws_active_clients >= 2`（消费者不齐时换版必然触发自动回退，先确认 botd 等都已连上）。
7. 环境变量 `APPROVED` 写明业务方批准人与时间。

## 3. 步骤（不限时段；须等 OMS 部署结束、稳定连上，开始前确认 `md_ws_active_clients` 为 2，开始时间先告诉业务方；业务方开 Portainer 后，用临时账户登录换 JWT 写 `/tmp/pt_jwt`，权限 600，用完删除）

```bash
python3 evidence/1.2.0/tools/build_release_image.py                    # 构建 md-server:1.2.0，写 nas-198/image-manifest.json
APPROVED="<批准人 时间>" python3 evidence/1.2.0/tools/execute_swap.py
#   前置条件 → 记录换前订阅 / 指标 / 旧容器 inspect → 旧容器改名 md-server-1.1.1-bak 并停止（保留作回退）
#   → 创建新 md-server → 健康后立刻 restore_subs.mjs → 等健康且消费者回到 >= 2（90 秒）
#   → 即时检查：镜像、环境、资源参数、容器标签 sha = cbc6b053…、容器内二进制与配置逐字节、开关 = 1、jemalloc 指标；写超时只记录
#   任何一步失败 → 自动回退（走下面第 4 节的二级回退逻辑，含回退容器前置检查）→ restore_subs
bash evidence/1.2.0/tools/observe_24h.sh                                # 立即开始 24 小时每分钟采样（见第 5 节）
sleep 600 && python3 evidence/1.2.0/tools/check_after_swap.py          # 10 分钟验收
```

换版后交付：`swap/swap-result.json` 与 `swap/ten-minute.json`（含 `verdict`）交业务方转主审；`verdict` 为 `pass_needs_botd_confirmation` 时不回退，`needs_botd_confirmation` 清单一并转去。

10 分钟验收判据（`check_after_swap.py`）：同一容器、健康、RestartCount = 0、**配置订阅（从 `config.release.yaml` 独立解析）全部还在**；
换前有、现在没有的运行期订阅中，**TICK / QUOTE** 必须每个都有 `runtime subscription idle-unsubscribed:` 日志，
且 `md_runtime_subscriptions_idle_unsubscribed_total` 不小于有日志的缺失数；`md_ws_active_clients >= 2`、开关为 1、jemalloc 指标在。
**运行期 KLINE** 缺失又对不上闲置退订日志时不判失败，单列 `needs_botd_confirmation`：REST DELETE 不打日志，
业务方关掉 1stbotv3 的 K 线图约 60 秒后，botd 会先退 WS 话题再 DELETE 自己登记的 K 线，这是正常释放。
**只记录不判失败**：写超时（为 0，或附日志说明原因）、交易所重连增量、`post-swap-24h/prod.csv` 前 10 个采样点的消费者数最小值与变化次数（消费者侧重连风暴检查）。

结论 `verdict` 三种（同时写在 `swap/ten-minute.json` 和输出最后一行）：

| verdict | 退出码 | 怎么办 |
|---|---|---|
| `pass` | 0 | 通过 |
| `pass_needs_botd_confirmation` | 0 | **不回退**。把 `needs_botd_confirmation` 清单（如 `KLINE binance BTCUSDT 30m`）交业务方转主审，由主审对照 botd 日志「已释放行情服务端 K 线订阅 <交易所> <币对> <级别>」核对 |
| `fail` | 1 | 按第 4 节回退（配置订阅缺失、TICK / QUOTE 缺失对不上日志、退订计数不足、消费者不足、健康 / 重启 / 开关 / jemalloc 任一不符） |

## 4. 回退（两级，均有现成工具，已用最终提交 `a08f8f3` 在 198 旁路演练，见 `drill/` 与 `SWAP_FINAL_FIX_REPORT.md`）

### 4.1 一级：不重启，改开关 + SIGHUP

```bash
APPROVED="<批准人 时间>" python3 evidence/1.2.0/tools/runtime_toggle.py idle_unsubscribe false   # 关闭闲置退订与按需补订
APPROVED="<批准人 时间>" python3 evidence/1.2.0/tools/runtime_toggle.py on_demand_quote false    # 只关报价补订（默认即关）
```

工具步骤：`guard()`（endpoint 3 = local）→ 读回容器内配置 → exec（root）`sed` 只改这一个键 → 读回断言只有这一行变化（不符则原样写回、不发 SIGHUP）
→ `kill?signal=HUP` → 读回 `md_runtime_subscriptions_enabled`、`md_runtime_subs` 日志（ENABLED / DISABLED / settings changed），
确认 `process_start_time_seconds` 与 RestartCount 不变。结果写 `swap/toggle-<键>-<值>/toggle-result.json`。

### 4.2 二级：切回 1.1.1

```bash
APPROVED="<批准人 时间>" python3 evidence/1.2.0/tools/rollback_to_111.py
```

工具步骤（全部自动）：

1. 存档当前订阅 `swap/rollback-<时间>/subscriptions.archived.json` 与当前指标。
2. **回退容器前置检查（不通过则不动当前容器）**：`md-server-1.1.1-bak` 存在 → 断言镜像 ID 是 1.1.1、处于停止状态、Cmd / User / Env / 端口与旧容器一致；
   不存在 → 按 `swap/old-container.inspect.json` 从 `md-server:1.1.1`（断言镜像 ID `3250e021…`）重建，核对镜像、Cmd、User、环境、端口、内存、CPU、网络、日志配置后再切换；
   重建出来的容器核对不通过就删掉（不留下可能被下次误用的回退容器）。
3. 当前容器重启策略改 no → 停止 → 改名 `md-server-<时间戳>-failed`（保留现场）→ 回退容器改名 `md-server`、恢复原重启策略、启动 → 60 秒内健康。
   动了当前容器之后**任何一步**失败（包括停容器的调用报错、回退容器启动报错、60 秒内不健康），都把当前容器放回原位：
   占着名字的回退容器停掉并改回 `-bak`，当前容器改回原名、恢复原重启策略、没在运行就启动（服务不留空），然后报错。
4. `restore_subs.mjs <存档订阅> http://192.168.1.198:8081 --apply`（回退失败、原容器被放回时也照样补，因为它被停过一次，运行期订阅已丢）。
5. 核对：镜像为 1.1.1、`process_start_time_seconds` 已变化、`md_runtime_subscriptions*` 指标消失、restore 成功、健康。结果写 `rollback-result.json`，`status` 为：
   `rolled_back`（成功）、`refused_before_touching_current`（前置检查不过，现网没动）、`rollback_failed_previous_restored`（已放回 1.2.0 并补了订阅）、
   `rollback_failed_manual_recovery_needed`（放回也失败 → 立即按 4.3 人工兜底）。

`execute_swap.py` 的自动回退走同一套逻辑（`swaplib.revert_after_failed_swap`：新容器已建 → 上面的二级回退；新容器没建成 → 把旧容器放回原位启动）；
自动回退本身失败时也照样补订阅并写 `swap-result.json`（`status` 为 `revert_failed_new_container_restored` 或 `revert_failed_manual_recovery_needed`）。

触发条件：健康 60 秒不恢复（自动）；消费者 90 秒未回到 2 个（自动）；10 分钟验收 `verdict` 为 `fail`（`pass_needs_botd_confirmation` **不回退**，见第 3 节）；
24 小时内出现 OMS / botd 数据中断、重连风暴、RSS 异常增长或配置订阅被退。
观察期内保留 `md-server-*-bak` 容器与 `md-server:1.1.1` 镜像（镜像不在就无法二级回退：脚本会拒绝回退、不停现网）。

### 4.3 人工兜底（Portainer 中途不可用、脚本报 `manual_recovery_needed` 或脚本中断时）

目标只有一个：**让一个名叫 `md-server` 的容器跑起来并健康**，优先 1.1.1。业务方在 Portainer 界面操作
（`http://192.168.1.198:9000` → 环境 `local` → Containers；改名在容器详情页名字旁的编辑按钮，重启策略在详情页下方 Restart policies → Update）：

**一律按镜像认容器，不按名字认**：容器列表的 Image 列，`md-server:1.1.1` 是 1.1.1，`md-server:1.2.0` 是 1.2.0。
名字里的时间戳不能用来判断新旧：脚本改名用的是 Unix 秒数，人工改名用的是当前时间，两种格式没法比较；刚改名的 1.1.1 也可能是「最新」的。
`md-server-1.1.0-quote-bnb-bak`（1.1.0 时代的旧回退容器）与本次无关，任何情况下都不要用。

1. **先看现状**：容器列表里搜 `md-server`，记下每个容器的名字、镜像（Image 列）、状态（running / exited / restarting）。
   名叫 `md-server` 的容器在运行，且 `curl -s http://192.168.1.198:8081/health` 返回 `{"status":"ok"…}` → 服务没空着，不用动，把现状告诉我。
2. **按下面的顺序处理，直到有一个名叫 `md-server` 的容器在运行且健康**：
   - (a) **腾出名字**：若名叫 `md-server` 的容器没在运行（exited / created），先把它改名，不要删：
     镜像是 `md-server:1.1.1` → 改名为 `md-server-1.1.1-bak`（该名已被占用则用 `md-server-1.1.1-bak-<当前时间>`）；
     镜像是 `md-server:1.2.0` → 改名为 `md-server-1.2.0-<当前时间>-failed`。
   - (b) **先试 1.1.1**：若有镜像为 `md-server:1.1.1` 的停止容器（正常名叫 `md-server-1.1.1-bak`）→ 改名为 `md-server` → 重启策略改为 `Unless stopped` → Start → 等健康（第 3 步）。
   - (c) **1.1.1 起不来**（Start 后状态是 exited / restarting，或 60 秒内 `/health` 不是 ok）→ 它的重启策略改为 `No` → Stop
     → 改回 `md-server-1.1.1-bak`（该名已被占用则用 `md-server-1.1.1-bak-<当前时间>`）→ 接着做 (d)。
   - (d) **放回 1.2.0**：找**镜像为 `md-server:1.2.0`** 的容器（不管它叫什么名字；正常只有一个，名字形如 `md-server-<数字>-failed`）
     → 改名为 `md-server` → 重启策略改为 `Unless stopped` → Start，先保证服务不空着；之后由我用 `rollback_to_111.py` 重建回退容器再切 1.1.1。
     镜像为 `md-server:1.2.0` 的容器有不止一个时，不要猜，先告诉我。
   - (e) 1.1.1 和 1.2.0 的容器都没有 → 告诉我，由我按 `swap/old-container.inspect.json` 用 `rollback_to_111.py` 重建（Portainer 恢复后）。
3. **等健康**：`curl -s http://192.168.1.198:8081/health`，30 秒内应为 ok。
4. **补订阅**（这台 MacBook 上执行，不需要 Portainer）：
   `node scripts/compat/restore_subs.mjs evidence/1.2.0/nas-198/swap/subscriptions.before.json http://192.168.1.198:8081 --apply`
   （回退过程中存过档的话，用更新的 `swap/rollback-*/subscriptions.archived.json`）。
5. **核对**：`curl -s http://192.168.1.198:8081/metrics | grep -E '^(md_ws_active_clients|process_start_time_seconds|md_runtime_subscriptions_enabled) '`；
   消费者回到 2 个；1.1.1 没有 `md_runtime_subscriptions_enabled` 这一行。

有 198 的 shell 时，等价命令：`docker ps -a --filter name=md-server --format '{{.Names}} {{.Image}} {{.Status}}'`（按镜像认容器）、
`docker ps -a --filter ancestor=md-server:1.2.0`、`docker update --restart no <名字>`、`docker stop <名字>`、`docker rename <旧名> <新名>`、
`docker update --restart unless-stopped md-server`、`docker start md-server`。人工兜底后，把操作过程与时间告诉我，我补记到 `swap/` 并通知主审。

## 5. 24 小时采样（`post-swap-24h/prod.csv`，每分钟）

- **采样机**：业务方这台 MacBook Pro（lihao的MacBook Pro，MacBookPro18,3，与 198 同在 192.168.1.x 内网）。
- **防休眠**：`observe_24h.sh` 用 `caffeinate -i -s` 启动采样（阻止空闲休眠与系统休眠；`-s` 只在接电源时有效）。采样期间**必须接电源、不合盖**
  （合盖且无外接显示器会强制休眠）。启动时把机型、电源状态、caffeinate 断言写入 `post-swap-24h/host.txt`，不在电源上会告警。
- **缺口标注**：结束后 `python3 evidence/1.2.0/tools/analyze_24h.py`，输出 `analysis.json`：覆盖率、缺口（相邻成功采样间隔 > 2 分钟，列起止时间与缺失点数）、
  失败采样、进程是否重启、换版后前 10 分钟每分钟消费者数、RSS 首 / 峰 / 末值、各计数器增量。报告里逐条列出缺口，不插值。
- 采样内容：RSS（1 小时预热后斜率）、`md_runtime_subscriptions` 曲线、闲置退订 / 按需补订 / 失败 / 上限拒绝累计、各 quote 入站速率（`quote_counts` 相邻行相减）、
  `md_ticks_dropped` 与 `md_quotes_dropped`、OKX / 币安各连接器重连次数；订阅增减事件写 `prod.events.log`。botd 侧行情年龄与重连计数由主审核对。
