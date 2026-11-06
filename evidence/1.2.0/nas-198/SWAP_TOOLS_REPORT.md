# 行情服务 1.2.0 换版前修正报告：换版 / 回退工具与 198 旁路演练（产物仍为 `cbc6b053…`）

依据：主审复核意见「1.2.0 换版前修正」（2026-11-07，`1stbotv3/docs/复核报告_行情服务1.2.0_金丝雀与换版脚本_2026-11-07.md`）。
分支 `feat/runtime-sub-idle-v1.2.0`；未打标签、未合并、未换版；现网 198 只读。演练时间 2026-11-06 17:16–17:21Z（北京时间 11-07 01:16–01:21）。

> 2026-11-07 补记：本报告的演练证据已移到 `nas-198/drill-67abacc/`（为最终提交版本的第二次演练腾出 `drill/`），文中 `drill/` 均指该目录。
> 后续修正与第二次演练见 [`SWAP_FINAL_FIX_REPORT.md`](SWAP_FINAL_FIX_REPORT.md)。

## 首段

- **仍使用产物 `cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf`**（`dist/md-server-1.2.0-12192d9-linux-x86_64`，源码 `42bc897`）。
  本轮产品代码未改：`git diff 42bc897 HEAD -- crates` 只有测试文件 `crates/md-gateway/tests/runtime_subs_gateway.rs`（新增 3 个测试）；产物不重建。
- **第二节 6 项各改了什么**（脚本 diff：`nas-198/swap-tools-since-879cdd7.diff`，即 `git diff 879cdd7 HEAD -- evidence/1.2.0/tools`）：
  1. **金丝雀依据改读 `canary-rerun/`**：`execute_swap.py` 的 `check_canary()` 断言第 1 部分 `gate-results.json` 全 0（quote 另核）、
     `canary-rerun/{short,prod}/summary.json` 为 `pass: true`、第一次运行两份 summary 的失败项恰好是 `["config_subscriptions_kept"]`，
     并当场跑 `recheck_config_kept.mjs` 复核两份时间线（含正对照）。SWAP_PLAN §2 同步改写。
  2. **二级回退不让服务空着**：新增 `swaplib.py`。`ensure_bak()` 在动当前容器之前先确认 `md-server-1.1.1-bak` 存在、镜像 ID 是 1.1.1、处于停止状态；
     不存在则按 `old-container.inspect.json` 从 `md-server:1.1.1`（断言镜像 ID `3250e021…`）重建，并核对镜像、环境、端口、内存、CPU、网络、日志配置；
     任一检查不通过就报错退出，当前容器不动。`rollback()` 切换后 60 秒内 1.1.1 不健康，就把刚停的容器放回原位重启。`execute_swap.py` 的自动回退也走这套逻辑。
  3. **10 分钟验收**：`check_after_swap.py` 的订阅判据只要求配置订阅（从 `config.release.yaml` 独立解析）全部还在。
     换前有、现在没有的运行期订阅，必须每个都有 `idle-unsubscribed` 日志，且 `md_runtime_subscriptions_idle_unsubscribed_total` 不小于缺失数。
     写超时改为只记录（值加日志行），不判失败。
  4. **自动回退条件**：90 秒内健康，且 `md_ws_active_clients >= 2`；写超时只写进 `swap-result.json`。另加一条前置条件：换版前现网消费者 ≥ 2，否则不动容器（见第三节）。
  5. **固定期望 sha**：`swaplib.EXPECTED_SHA = cbc6b053…`，与 `build-logs/dist-sha256.txt` 记录交叉核对。`build_release_image.py` 不再接受产物或 sha 参数。
     `execute_swap.py` 断言下面各处都等于这个值：常量、记录、本地产物、镜像清单、镜像标签、容器标签、容器内读回的二进制。
  6. **两级回退工具**：
     - 一级 `runtime_toggle.py <idle_unsubscribe|on_demand_quote> <true|false>`：`guard()` → exec（root）`sed` 只改一个键 → 读回断言只改了这一行（不符则原样写回，不发 SIGHUP）→ `kill?signal=HUP` → 读回开关指标与 `md_runtime_subs` 日志，核对进程未重启。
     - 二级 `rollback_to_111.py`：存档当前订阅 → `swaplib.rollback`（含回退容器前置检查与重建）→ `restore_subs.mjs <存档> … --apply` → 核对镜像为 1.1.1、`process_start_time_seconds` 已变、`md_runtime_subscriptions*` 指标消失。
     - 演练脚本 `drill_198.py`：演练模式只允许 `md-exp-120-drill*` 名字，现网模式拒绝 `md-exp-` 名字（`Target.check_name`），两种模式不能混用。
     - SWAP_PLAN §4 写全两级步骤；DEPLOYMENT §12.3 与 OPS_GUIDE §8.6 改为指向新工具，不再用 `pt_swap.py rollback`。
- **演练结果**（198 旁路，端口 28125 / 50125，256 MiB、2 核、带标签，`python3 evidence/1.2.0/tools/drill_198.py`，证据 `nas-198/drill/`）：
  - **一级回退** `runtime_toggle.py --drill`：
    - `idle_unsubscribe` true→false→true：开关指标 0→1；日志 `DISABLED` → `ENABLED (… on_demand_quote=false)`。
    - `on_demand_quote` false→true→false：日志 `settings changed (… on_demand_quote=true)` → `(…=false)`。
    - 4 次都是配置只变 1 行、进程启动时间与 RestartCount 不变，全部 `pass: true`。
  - **二级回退正常路径** `rollback_to_111.py --drill`：`rollback pre-check: md-exp-120-drill-bak present, image sha256:3250e021d152, stopped`。
    - 服务中断 6.4 秒，补回 14 条订阅。
    - 检查项全部 true：镜像为 1.1.1、进程启动时间 1793985476→1793985505、运行期指标消失、restore 成功、健康。
  - **「回退容器不存在 → 先重建再切换」**：再换一次版（swap-2）后删掉回退容器，然后 `rollback_to_111.py --drill`。
    - 日志：`… was missing; recreated 01cf523b5d37 from md-server:1.1.1 with the old container's parameters (verified image, env, ports, memory, cpu, network, logging)`。
    - `bak_status: recreated`，中断 5.6 秒，检查项全部 true。
  - **换版演练**两次（`execute_swap.py --drill`）均通过即时检查：
    - 中断 5.7 秒 / 7.9 秒，订阅 14→14。
    - 容器标签与读回的二进制 sha 都是 `cbc6b053…`，所以复制标签时去掉镜像来源标签在真实 Docker 上生效。
  - 10 分钟验收逻辑 `check_after_swap.py --drill` 通过。
  - **现网只读**：进程启动时间 `1792785202` 前后不变，镜像仍为 1.1.1，RestartCount 0，healthy。
  - **清理**：`md-exp-120-drill*` 三个容器与镜像 `md-server:exp-120-release` 已删，无残留（`drill/cleanup-check.txt`）。
  - **第一次演练没跑完**：第 3 个开关失败，是工具 bug，不是服务问题，修复后整套重跑通过。原因是预期 diff 的正则在文件最后一行多带了换行。
    失败时容器内配置已被改、未发 SIGHUP，演练容器随即被清理。已修复，并加上失败即原样写回配置。现场保留在 `nas-198/drill-attempt1-toggle-bug/`。
- **演练期间现网消费者从 2 个变成 1 个，需要业务方查看**：
  - 时间在 17:17:40–17:18:53Z 之间，到 17:21Z 仍是 1 个。`md_ws_subscriptions_active` 从今天各次快照的 13–20 降到 2，说明订阅较多的那个消费者断开了。
  - 现网 md-server 未重启，无任何日志（写超时会打 WARN）；`md_ws_kicked_write_timeout_total` 88 未变、`md_ws_kicked_lagged_total` 0，所以是消费者自己断开的，不是被服务踢掉。
  - 同一时段 198 上出现了一个不是我建的容器 `oms-tmp-restore-ac1863b`（17:18:26Z 启动）；演练只操作 `md-exp-120-drill*`。
  - 从 1.1.1 的指标无法判断断开的是哪个消费者，请业务方查看 botd / OMS 的连接。
  - 换版脚本已加前置条件：现网消费者少于 2 个时拒绝换版，不动容器。否则换版后等不到 2 个消费者会触发自动回退。
- **需要业务方做的事**：
  1. 把本报告转主审复核。
  2. 查看并恢复上面断开的消费者（botd / OMS）。
  3. 主审通过后约定开始时间，我会提前告知。到时：
     - 打开 Portainer，临时账户保留到 24 小时观察结束；
     - botd 保持运行；
     - 换版后在场约 15 分钟；
     - 采样用的这台 MacBook Pro 接电源、不合盖 24 小时。
  4. 观察期内保留 `md-server-*-bak` 容器与 `md-server:1.1.1` 镜像。

## 一、做了什么

| 文件 | 改动 |
|---|---|
| `tools/swaplib.py`（新） | 换版 / 回退共用逻辑：两种模式的名字校验（`Target`）、固定 sha、去镜像来源标签、`ensure_bak` / `rollback` / `_put_back` / `revert_after_failed_swap` |
| `tools/execute_swap.py`（重写） | 金丝雀依据改读 `canary-rerun/` 并断言第一次只失败配置订阅判定；固定 sha 全链核对；换版前消费者 ≥ 2；自动回退条件只看健康与消费者；写超时只记录；`--drill` 演练模式 |
| `tools/build_release_image.py`（重写） | 去掉产物 / sha 参数，固定 `cbc6b053…`；`--drill` 构建 `md-server:exp-120-release` |
| `tools/check_after_swap.py`（重写） | 订阅判据抽成 `judge_subscriptions()`：配置订阅全在、缺失的运行期订阅都要能对上闲置退订日志与计数；写超时、交易所重连、前 10 分钟消费者数只记录；`--drill` |
| `tools/runtime_toggle.py`（新） | 一级回退（Portainer API 版 OPS_GUIDE §8.6） |
| `tools/rollback_to_111.py`（新） | 二级回退全步骤 |
| `tools/drill_198.py`（新） | 198 旁路演练（第 0–7 步，`finally` 里清理并核对现网） |
| `tools/test_swaplib_offline.py`（新） | 假 Portainer 的离线测试 16 项，覆盖 198 上不便制造的失败分支 + 10 分钟判据（用 198 金丝雀复跑的真实数据） |
| `tools/observe_24h.sh` | `caffeinate -i -s`；记录采样机、电源与 caffeinate 断言到 `host.txt`；不在电源上告警；防重复启动 |
| `tools/analyze_24h.py`（新） | 24 小时采样分析：覆盖率、缺口（起止与缺失点数）、失败采样、是否重启、前 10 分钟消费者数、RSS、计数器增量 |
| `tools/run_mutations.sh` | 新增 M17、M18 两个正对照；`ONLY=` 只跑指定项 |
| `crates/md-gateway/tests/runtime_subs_gateway.rs` | 第三节建议的 3 个测试（只改测试） |
| `nas-198/SWAP_PLAN_1.2.0.md` | §2 前置条件、§3 命令与 10 分钟判据、§4 两级回退全步骤、§5 采样机与防休眠、缺口标注 |
| `DEPLOYMENT.md` §12.3、`OPS_GUIDE.md` §8.6 | 指向新工具 |

主审第三节建议的处理：

- 消费者侧重连风暴检查：`check_after_swap.py` 记录 `post-swap-24h/prod.csv` 前 10 个采样点的消费者数，`analyze_24h.py` 输出换版后前 10 分钟每分钟消费者数、最小值与变化次数。
- 复制标签去掉镜像来源标签：`swaplib.strip_image_labels()`，演练中容器标签 sha 断言通过。
- 3 个测试：已加，并配了正对照 M17 / M18（原来抓不住的两种改坏方式现在抓得住）。
- 24 小时采样的机器与防休眠：写进 SWAP_PLAN §5 与 `observe_24h.sh`，缺口用 `analyze_24h.py` 标注。

## 二、怎么验证的（原文）

### 2.1 离线测试与正对照（不联网、不需要 JWT）

```text
$ python3 evidence/1.2.0/tools/test_swaplib_offline.py
{"real_canary_missing_runtime": [["binance", 1, "SNDKUSDT", ""], ["binance", 2, "BNBUSDT", "5m"], ["binance", 2, "BTCUSDT", "30m"], ["binance", 2, "SNDKUSDT", "30m"], ["binance", 2, "SNDKUSDT", "5m"]], "idle_unsubscribed_total": 6.0}
{
 "bak_present_normal_path": true,
 "bak_wrong_image_aborts_before_touching_current": true,
 "bak_running_aborts_before_touching_current": true,
 "bak_missing_recreated_then_switched": true,
 "recreate_happens_before_current_is_stopped": true,
 "unhealthy_111_puts_previous_back": true,
 "drill_mode_uses_only_drill_names": true,
 "prod_refuses_drill_names": true,
 "drill_refuses_prod_names": true,
 "prod_allows_its_names": true,
 "strip_image_labels": true,
 "expected_sha_matches_dist_record": true,
 "ten_minute_real_canary_data_passes": true,
 "ten_minute_missing_log_line_fails": true,
 "ten_minute_counter_below_missing_fails": true,
 "ten_minute_missing_config_subscription_fails": true
}
```

离线测试本身的正对照（在临时副本里把 `swaplib.py` 改坏，测试必须失败；日志 `positive-controls/swaplib/`）：

```text
ensure-bak-after-stop: exit=1   （回退容器检查挪到停当前容器之后 → 失败）
no-bak-image-check:    exit=1 failed=[bak_wrong_image_aborts_before_touching_current]
no-put-back:           exit=1 failed=[unhealthy_111_puts_previous_back]
```

产品测试与正对照（只跑新增两项 + 原样全量；`positive-controls/r4-c790a68/summary.txt`）：

```text
$ ONLY="M17|M18" bash evidence/1.2.0/tools/run_mutations.sh c790a68 evidence/1.2.0/positive-controls/r4-c790a68
commit=c790a6884501f4ea1990bd3767890f48c8ff5e98
M17-quote-skip-after-taking-a-slot: CAUGHT; exit=101; failed=[skipped_quote_does_not_count_against_the_limit ]
M18-no-nudge-on-settings-change: CAUGHT; exit=101; failed=[settings_change_subscribes_immediately_under_long_check_interval ]
M0-unmutated: exit=0; passed=293 failed=0
overall=PASS
```

`cargo fmt --all -- --check` 无输出；`cargo clippy --workspace --all-targets` 13 条（与基线相同）。

24 小时分析脚本：用 1.1.1 的 24 小时数据（副本）跑出覆盖率 100%、无缺口、前 10 分钟消费者 `[2,4,4,4,4,2,2,2,2,2]`；删掉 30 行后报
`97.92 [{'from': '2026-10-23T21:31:14Z', 'to': '2026-10-23T22:02:14Z', 'seconds': 1860, 'missing_samples': 30}]`。

### 2.2 198 旁路演练（`nas-198/drill/drill.log` 节选，时间只留时分秒、路径缩短）

```text
17:17:40 prod (read-only) before: process_start_time_seconds 1792785202.0, md_ws_active_clients 2.0, image sha256:3250e021…, restart_count 0, healthy
17:17:48 step 0-build-drill-image: exit=0 :: build_release_image.py --drill
17:17:51 step 1-restore-into-drill-111: exit=0 :: restore_subs.mjs drill/prod.subscriptions.json http://192.168.1.198:28125 --apply
17:17:51 drill 1.1.1 container a52e81ad7b5d up on http://192.168.1.198:28125
17:18:03 step 2-swap-1: exit=0 :: execute_swap.py --drill drill/swap-1
17:18:07 step 3-toggle-1-idle_unsubscribe-false: exit=0
17:18:11 step 3-toggle-2-idle_unsubscribe-true: exit=0
17:18:15 step 3-toggle-3-on_demand_quote-true: exit=0
17:18:18 step 3-toggle-4-on_demand_quote-false: exit=0
17:18:19 step 4-ten-minute-check-logic: exit=0 :: check_after_swap.py --drill drill/swap-1
17:18:26 step 5-rollback-normal: exit=0 :: rollback_to_111.py --drill drill/rollback-normal drill/swap-1/old-container.inspect.json
17:18:41 step 6a-swap-2: exit=0
17:18:41 step 6b: deleted md-exp-120-drill-bak a52e81ad7b5d to simulate the backup container being cleaned up
17:18:49 step 6c-rollback-recreate: exit=0 :: rollback_to_111.py --drill drill/rollback-recreate drill/swap-2/old-container.inspect.json
17:18:53 cleanup: removed ['md-exp-120-drill', 'md-exp-120-drill-1793985523-failed', 'md-exp-120-drill-1793985499-failed']; leftovers containers=[] images=[];
         prod after: process_start_time_seconds 1792785202.0, md_ws_active_clients 1.0, image sha256:3250e021…, restart_count 0, healthy
```

一级回退（`drill/toggle-*/toggle-result.json`，日志行原文节选）：

```text
toggle-1 idle_unsubscribe false  diff ['-  idle_unsubscribe: true', '+  idle_unsubscribe: false']  enabled 0.0 restart 0 pass
  INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe DISABLED (current subscriptions are kept as they are)
toggle-2 idle_unsubscribe true   diff ['-  idle_unsubscribe: false', '+  idle_unsubscribe: true']  enabled 1.0 restart 0 pass
  INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED (idle_minutes=10, check_interval_seconds=60, startup_grace_minutes=10, max_r…
toggle-3 on_demand_quote true    diff ['-  on_demand_quote: false', '+  on_demand_quote: true']  enabled 1.0 restart 0 pass
  INFO md_runtime_subs: runtime subscriptions: settings changed (idle_minutes=10, …, max_runtime_subscriptions=200, on_demand_quote=true)
toggle-4 on_demand_quote false   diff ['-  on_demand_quote: true', '+  on_demand_quote: false']  enabled 1.0 restart 0 pass
```

二级回退（`drill/rollback-*/rollback-result.json`）：

```text
rollback-normal:   bak_status present,   outage 6.4s, start 1793985476 -> 1793985505, subscriptions_after 14,
                   checks {image_is_111, process_start_time_changed, runtime_metrics_gone, restore_subs_ok, healthy_now} all true
rollback-recreate: bak_status recreated, outage 5.6s, start 1793985514 -> 1793985528, subscriptions_after 14, checks all true
  rollback pre-check: md-exp-120-drill-bak was missing; recreated 01cf523b5d37 from md-server:1.1.1 with the old container's parameters
  (verified image, env, ports, memory, cpu, network, logging)
```

换版演练（`drill/swap-*/swap-result.json`）：两次 `immediate_checks_passed`，`binary_sha256` 均为 `cbc6b053…`，中断 5.7 / 7.9 秒，订阅 14 → 14，`ws_kicked_write_timeout_total` 0；
两次前置检查都跑了 `recheck_config_kept.mjs`（`config_subscriptions_kept: true`，正对照 `caught: true`）。
10 分钟判据（`drill/swap-1/ten-minute.json`）：8 项全 true，配置订阅 10 条全在，缺失运行期订阅 0。

清理核对（`drill/cleanup-check.txt`，17:20:26Z）：`md-exp-*` 容器无；`exp-120` 镜像无；`md-server*` 容器与镜像与演练前一致（`md-server` 1.1.1 运行中，
另有演练前就存在的 `md-server-1.1.0-quote-bnb-bak`，未动）；现网 `md-server` 镜像 `3250e021d152`、重启 0、启动于 2026-10-23T19:53:21、healthy。

### 2.3 现网消费者变化（只读）

| 时间（UTC） | 来源 | `md_ws_active_clients` | `md_ws_subscriptions_active` | 写超时累计 |
|---|---|---|---|---|
| 金丝雀前 / 后（16:06–16:45） | `canary*/prod.metrics.*.txt` | 2 | 17–20 | 88 |
| 17:16:43 / 17:17:18 | 第一次演练前 / 后 | 2 | — | — |
| 17:17:40 | 第二次演练前 | 2 | — | — |
| 17:18:53 起 | 第二次演练后、`drill/prod.metrics.after-drill.txt`、之后 6 次每 10 秒复查 | 1 | 2 | 88 |

现网 md-server 自 16:10Z 起没有任何日志行（`RUST_LOG=warn`，写超时与踢出都会打 WARN）。17:18:26Z 198 上启动了 `oms-tmp-restore-ac1863b`（不是本次操作）。

## 三、没做什么与原因

- **没有换版**，也没有动现网 `md-server`、OMS、1stbotv3、153；现网只做 GET 与容器信息读取。
- **「消费者 ≥ 2 才换版」这条前置条件是演练之后加的**：演练里消费者门槛是 0（演练容器没有业务消费者），所以这条只在现网模式生效，演练里没走到。
  改动是一条断言，加在动容器之前（`execute_swap.py`，读 `metrics.before`）。
- **失败即原样写回配置**（`runtime_toggle.py` 的 `put_config`）是在第一次演练暴露问题后加的；第二次演练 4 次开关都成功，没有走到写回分支。
- 10 分钟判据里「缺失的运行期订阅对上闲置退订日志」在演练中缺失数为 0（演练只有几十秒，保护期 10 分钟）。这条判据用 198 金丝雀复跑的真实数据离线验证（5 条缺失全部对上，删一条日志或把计数改小都判失败）。
- 现网断开的是哪个消费者无法判断（1.1.1 指标不区分），没有去碰 OMS / botd。

## 四、兼容性声明

产物与配置都没变（`cbc6b053…`、`config.release.yaml` sha `44a5e0e6…`），第二轮的兼容性结论不变。本轮只改换版 / 回退工具、文档与 3 个测试。

## 五、需要对方做的事

1. **业务方**：把本报告转主审；查看演练期间断开的现网消费者（botd / OMS，17:18Z 前后），换版前恢复到 2 个。
2. **主审**：复核第二节 6 项改动、演练证据（`nas-198/drill/`）与离线测试；通过后告知业务方。
3. **换版当天**：我提前告知开始时间；业务方打开 Portainer（临时账户保留到 24 小时观察结束）、botd 保持运行、换版后在场约 15 分钟；
   采样机（这台 MacBook Pro）接电源、不合盖 24 小时。观察期内保留 `md-server-*-bak` 容器与 `md-server:1.1.1` 镜像。
