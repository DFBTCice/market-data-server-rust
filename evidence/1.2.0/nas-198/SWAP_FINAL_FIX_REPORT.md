# 行情服务 1.2.0 换版前最后一项：报告（产物仍为 `cbc6b053…`）

依据：主审复核意见「1.2.0 换版前最后一项」（2026-11-07，`1stbotv3/docs/复核报告_行情服务1.2.0_金丝雀与换版脚本_2026-11-07.md` 第八节）。
分支 `feat/runtime-sub-idle-v1.2.0`；工具提交 `a08f8f3`；未打标签、未合并、未换版；现网 198 只读。
演练时间 2026-11-06 17:59:41–18:02:01Z（北京时间 11-07 01:59–02:02）。

## 首段

- **仍使用产物 `cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf`**。产品代码与产物都没变：`git diff 67abacc a08f8f3 -- crates Cargo.toml Cargo.lock config.yaml` 为空。
  本轮只改换版 / 回退工具、SWAP_PLAN 和证据；脚本 diff 见 `nas-198/swap-tools-since-67abacc.diff`（`git diff 67abacc a08f8f3 -- evidence/1.2.0/tools`）。
- **第二节必须修（已改）**：
  - `check_after_swap.py` 的 `judge_subscriptions()`：换前有、现在没有的运行期订阅分三类：
    - 有闲置退订日志的，算已解释；
    - 没有日志的 TICK / QUOTE，判失败；
    - 没有日志的**运行期 KLINE**，**不判失败**，单列 `needs_botd_confirmation`。
  - 配置订阅缺失仍判失败，配置里的 K 线也一样。退订计数改为「不小于已解释数」。
  - 输出 `verdict`：
    - `pass`，退出码 0；
    - `pass_needs_botd_confirmation`，退出码 0，最后一行写明「不回退，清单交业务方转主审对照 botd 日志」；
    - `fail`，退出码 1。
  - SWAP_PLAN §3 加了三种结论的处理表；§4 的回退触发条件改为只有 `verdict` 为 `fail` 才回退，`pass_needs_botd_confirmation` 不回退。
- **第三节建议（已改，换版前一起做了）**：
  - 建议 2：回退失败、原容器被放回后，`rollback_to_111.py` 照样补订阅、写 `rollback-result.json`（带 `status`）；`execute_swap.py` 的自动回退本身失败时，也照样补订阅、写 `swap-result.json`。
  - 建议 3：`rollback()` 动了当前容器后任何一步失败（包括停容器的调用报错），都执行「放回」：恢复原重启策略，没在运行就启动。
  - 建议 4：重建出的回退容器核对不通过就删掉；已存在的回退容器多核对 Cmd、User、Env、端口。
  - 建议 5：SWAP_PLAN 新增 §4.3 人工兜底，用 Portainer 界面一步步操作，附等价 docker 命令。
  - 建议 6：演练脚本开跑前断言工具已提交、工作区干净，并记录提交号；本次演练用的就是 `a08f8f3`。
- **顺带修了一个潜在问题**：67abacc 的 `_put_back` 无条件停「占着名字的容器」。如果回退容器启动这一步报错，它根本没在运行，Docker 停它会返回 304，而 `pt_exp.call` 默认把 304 当错误。结果是放回中途中断，现网可能空着。
  现在放回时停 / 启动都接受 304，启动前先看是否已在运行。离线测试 `start_of_111_fails_puts_previous_back` 覆盖这个情况，正对照 O7（把 304 容忍去掉）能被抓到。
- **演练结果**（`python3 evidence/1.2.0/tools/drill_198.py`，工具 `a08f8f3`、工作区干净，证据 `nas-198/drill/`）：
  - 换版 3 次，中断 5.4 / 5.2 / 5.2 秒，订阅 14 → 14，读回 sha 均为 `cbc6b053…`。
  - **一级回退** `idle_unsubscribe` 1→0→1、`on_demand_quote` 0→1→0：4 次都只改一行，开关指标 0 / 1 与目标一致，进程没有重启。
  - **二级回退正常路径**：日志 `present, image sha256:3250e021d152, stopped, cmd/user/env/ports match`，中断 5.2 秒，5 项核对全部 true。
  - **「回退容器不存在 → 先重建再切换」**：日志 `was missing; recreated 4667483687a5 … (verified image, cmd, user, env, ports, memory, cpu, network, logging)`，中断 4.1 秒，5 项核对全部 true。
  - **新增「1.1.1 起不来 → 放回 1.2.0」**（真实 Docker）：
    - 做法：把回退容器换成参数一致、但内存只有 6 MiB 的 1.1.1。
    - 工具日志：`1.1.1 container not healthy within 60s; putting the previous container back`。
    - 结果：`status: rollback_failed_previous_restored`，`restore_subs_exit 0`，退出码 1（符合预期）。
    - 核对 7 项全部 true：1.2.0 在运行且健康、进程重启过、订阅 14 条补回、6 MiB 容器回到 `-bak` 名下且已停止。
    - 这条路径服务空了约 66 秒（等满 60 秒健康超时才放回），这是 60 秒健康等待本身的代价。
  - **10 分钟判据逻辑**：`verdict pass`。
  - **现网只读**：演练前后进程启动时间都是 `1792785202`，镜像 1.1.1，重启 0 次，消费者 2 个。
  - **清理**：`md-exp-120-drill*` 四个容器和演练镜像已删，无残留。
- **需要业务方做的事**：
  1. 把本报告与 diff 转主审。
  2. 主审核对通过后定开始时间（不限时段），**避开 OMS 部署窗口**：开始前确认 OMS 和 botd 都稳定连上，`md_ws_active_clients` 为 2。
  3. 到时打开 Portainer，botd 保持运行，换后在场约 15 分钟；跑采样的 MacBook Pro 接电源、不合盖。
  4. 24 小时观察期内保留 `md-server-*-bak` 容器、`md-server:1.1.1` 镜像和临时账户。
  5. 10 分钟验收若出现 `pass_needs_botd_confirmation`，不回退，把清单转主审。

## 一、做了什么

| 文件 | 改动 |
|---|---|
| `tools/check_after_swap.py` | 判据按上面三类拆分；`needs_botd_confirmation`、`verdict`、退出码；`missing_runtime_explained` 单独列出 |
| `tools/swaplib.py` | `RollbackFailed(SystemExit)`：`restored=True` 表示已放回，`False` 表示须人工兜底；`rollback()` 动了当前容器后任何失败都 `_put_back`；`_put_back` 停 / 启动接受 304、启动前先查状态；`ensure_bak` 已存在路径核对 Cmd / User / Env / 端口，重建路径多核对 Cmd / User，核对失败删除刚建的容器 |
| `tools/rollback_to_111.py` | 区分 `rolled_back` / `refused_before_touching_current` / `rollback_failed_previous_restored` / `rollback_failed_manual_recovery_needed`，每种都写结果文件；放回后用存档补订阅 |
| `tools/execute_swap.py` | 自动回退本身失败时照样补订阅、写 `swap-result.json`（`revert_failed_*`）；补订阅超时也不影响写结果 |
| `tools/drill_198.py` | 开跑前断言工具已提交并记录提交号；新增第 7 步：真实 Docker 上的「1.1.1 起不来 → 放回」 |
| `tools/test_swaplib_offline.py` | 重写：假 Docker 模拟 304 和注入失败（报错前生效 / 报错后生效）、支持删除；每个用例单独捕获异常；23 项 |
| `tools/run_offline_mutations.sh`（新） | 离线测试的正对照，可复跑：14 种改坏 + 原样代码 |
| `nas-198/SWAP_PLAN_1.2.0.md` | 开头写明换版时段与 OMS 窗口；§3 判据与结论处理表；§4.2 失败分支与结果状态；§4 触发条件；§4.3 人工兜底 |
| `nas-198/drill-67abacc/` | 上一轮的演练证据挪到这里（`drill/` 现在是本次最终版本的演练）；`SWAP_TOOLS_REPORT.md` 开头加了说明 |

## 二、怎么验证的（原文）

### 2.1 离线测试（`python3 evidence/1.2.0/tools/test_swaplib_offline.py`，23 项全部 true）

```text
bak_present_normal_path, bak_wrong_image_aborts_before_touching_current, bak_running_aborts_before_touching_current,
bak_wrong_cmd_aborts_before_touching_current, bak_missing_recreated_then_switched, recreated_bak_mismatch_is_deleted_and_current_untouched,
unhealthy_111_puts_previous_back, start_of_111_fails_puts_previous_back, stop_call_errors_after_stopping_restarts_current,
stop_call_errors_before_stopping_restores_restart_policy, put_back_failure_reports_manual_recovery, drill_mode_uses_only_drill_names,
prod_refuses_drill_names, drill_refuses_prod_names, prod_allows_its_names, strip_image_labels, expected_sha_matches_dist_record,
ten_minute_real_canary_data_passes, ten_minute_unexplained_tick_fails, ten_minute_unexplained_quote_fails,
ten_minute_unexplained_runtime_kline_needs_botd_not_fail, ten_minute_counter_below_explained_fails, ten_minute_missing_config_kline_fails
```

10 分钟判据用的仍是 198 金丝雀复跑的真实数据（5 条运行期订阅缺失、闲置退订计数 6），在上面做了这些变化：

| 做法 | 预期 | 结果 |
|---|---|---|
| 去掉 TICK SNDKUSDT 那条闲置退订日志 | 判失败 | 判失败 |
| 加一条换前有、现在没有、也没有日志的 QUOTE | 判失败 | 判失败 |
| 去掉 KLINE BNBUSDT 5m 那条闲置退订日志 | 不判失败，单列 `needs_botd_confirmation` | 不判失败，单列 |
| 把闲置退订计数改为 0 | 判失败 | 判失败 |
| 拿掉一条配置里的 K 线 | 判失败，且不进入待确认清单 | 判失败，不在清单里 |

### 2.2 离线正对照（`bash evidence/1.2.0/tools/run_offline_mutations.sh`，`positive-controls/offline-tools/summary.txt`）

```text
tools_commit=a08f8f3fffcfbc212a2848a1bb7210c9bcee67f8 tools_dirty=0
O1-no-bak-image-check: CAUGHT; exit=1; false=[bak_wrong_image_aborts_before_touching_current]
O2-no-bak-stopped-check: CAUGHT; exit=1; false=[bak_running_aborts_before_touching_current]
O3-no-bak-params-check: CAUGHT; exit=1; false=[bak_wrong_cmd_aborts_before_touching_current]
O4-mismatched-recreated-bak-kept: CAUGHT; exit=1; false=[recreated_bak_mismatch_is_deleted_and_current_untouched]
O5-ensure-bak-after-stop: CAUGHT; exit=1; false=[bak_wrong_image_… bak_running_… bak_wrong_cmd_… bak_missing_recreated_then_switched recreated_bak_mismatch_…]
O6-no-put-back: CAUGHT; exit=1; false=[unhealthy_111_… start_of_111_fails_… stop_call_errors_after_… stop_call_errors_before_… put_back_failure_…]
O7-put-back-stop-rejects-304: CAUGHT; exit=1; false=[start_of_111_fails_puts_previous_back]
O8-put-back-keeps-restart-policy-no: CAUGHT; exit=1; false=[unhealthy_111_… start_of_111_fails_… stop_call_errors_after_… stop_call_errors_before_…]
O9-put-back-never-starts: CAUGHT; exit=1; false=[unhealthy_111_… start_of_111_fails_… stop_call_errors_after_… put_back_failure_…]
O10-failed-put-back-reported-as-restored: CAUGHT; exit=1; false=[put_back_failure_reports_manual_recovery]
C1-unexplained-tick-quote-excused: CAUGHT; exit=1; false=[ten_minute_unexplained_tick_fails ten_minute_unexplained_quote_fails]
C2-unexplained-kline-counted-as-failure: CAUGHT; exit=1; false=[ten_minute_unexplained_runtime_kline_needs_botd_not_fail]
C3-idle-counter-not-checked: CAUGHT; exit=1; false=[ten_minute_counter_below_explained_fails]
C4-config-kline-excused: CAUGHT; exit=1; false=[ten_minute_missing_config_kline_fails]
unmutated: exit=0; false=[]
overall=PASS
```

### 2.3 198 演练（`nas-198/drill/drill.log` 节选，时间只留时分秒、路径缩短；`drill-summary.json` 记录 `tools_commit a08f8f3…`）

```text
17:59:41 prod (read-only) before: process_start_time_seconds 1792785202.0, md_ws_active_clients 2.0, image sha256:3250e021…, restart_count 0, healthy
17:59:45 step 0-build-drill-image: exit=0
17:59:47 step 1-restore-into-drill-111: exit=0          drill 1.1.1 container 094d646558cd up on :28125
17:59:59 step 2-swap-1: exit=0
18:00:03 step 3-toggle-1-idle_unsubscribe-false: exit=0
18:00:07 step 3-toggle-2-idle_unsubscribe-true: exit=0
18:00:11 step 3-toggle-3-on_demand_quote-true: exit=0
18:00:14 step 3-toggle-4-on_demand_quote-false: exit=0
18:00:15 step 4-ten-minute-check-logic: exit=0           (verdict pass)
18:00:20 step 5-rollback-normal: exit=0
18:00:33 step 6a-swap-2: exit=0
18:00:33 step 6b: deleted md-exp-120-drill-bak 094d646558cd to simulate the backup container being cleaned up
18:00:38 step 6c-rollback-recreate: exit=0
18:00:51 step 7a-swap-3: exit=0
18:00:52 step 7b: replaced md-exp-120-drill-bak with e4e59bc9bbfa (md-server:1.1.1, same cmd/user/env/ports, memory 6 MiB) so the 1.1.1 container cannot become healthy
18:01:59 step 7c-rollback-put-back: exit=1 (expected 1)
18:01:59 step 7 check: {'status_previous_restored': True, 'restore_subs_ok': True, 'current_is_new_image': True, 'current_running_healthy': True,
                        'process_restarted_by_put_back': True, 'subscriptions_restored': True, 'weak_bak_back_under_bak_name_and_stopped': True}
18:02:01 cleanup: removed ['md-exp-120-drill-bak', 'md-exp-120-drill', 'md-exp-120-drill-1793988034-failed', 'md-exp-120-drill-1793988015-failed'];
         leftovers containers=[] images=[]; prod after: process_start_time_seconds 1792785202.0, md_ws_active_clients 2.0, image sha256:3250e021…, restart_count 0, healthy
```

放回路径（`drill/rollback-put-back/rollback.log` 与 `rollback-result.json`）：

```text
18:00:52.536 archived 14 subscriptions from md-exp-120-drill; process_start_time_seconds=1793988044.0
18:00:52.629 rollback pre-check: md-exp-120-drill-bak present, image sha256:3250e021d152, stopped, cmd/user/env/ports match
18:01:57.240 rollback: 1.1.1 container not healthy within 60s; putting the previous container back
18:01:59.357 rollback result: status rollback_failed_previous_restored,
             error "rollback failed (1.1.1 container not healthy within 60s); previous container restored, health 0.0",
             restore_subs_exit 0, process_start_time 1793988044 -> 1793988119, healthy_now true, pass false
```

其余各步（`drill/*/swap-result.json`、`rollback-*/rollback-result.json`、`toggle-*/toggle-result.json`）：

```text
swap-1 / swap-2 / swap-3: immediate_checks_passed, outage 5.4 / 5.2 / 5.2 s, subs 14 -> 14, binary_sha256 cbc6b053…
rollback-normal:   rolled_back, bak_status present,   outage 5.2 s, checks image_is_111 / process_start_time_changed / runtime_metrics_gone / restore_subs_ok / healthy_now all true
rollback-recreate: rolled_back, bak_status recreated, outage 4.1 s, checks all true
toggle-1..4: config_diff exactly one line each; md_runtime_subscriptions_enabled 0 / 1 / 1 / 1; restart_count 0
ten-minute (swap-1): verdict pass; checks all true (missing_tick_quote_explained_by_idle_unsubscribe true)
```

清理核对（`drill/cleanup-check.txt`，18:02:10Z）：`md-exp-*` 容器无；`exp-120` 镜像无；`md-server*` 容器和镜像与演练前一致
（`md-server` 1.1.1 运行中；`md-server-1.1.0-quote-bnb-bak` 是之前就有的，没动）；现网 `md-server` 重启策略 `unless-stopped`，重启 0 次，健康，消费者 2 个。

## 三、没做什么与原因

- **没有换版**，没有动现网 `md-server`、OMS、1stbotv3、153；对现网只做了 GET 和容器信息读取。临时账户的 JWT 用完已删除，密码没有落盘。
- **Rust 代码没改**（`git diff c790a68 a08f8f3 -- crates Cargo.toml Cargo.lock config.yaml` 为空），所以没有重跑 cargo 门禁。上一轮在 c790a68 跑的全量是 293 个，主审也已复跑通过。
- **`execute_swap.py` 自动回退本身失败的分支没有在 198 上演练**：要演练就得让一次换版故意失败，再让回退也失败。它和二级回退共用 `RollbackFailed` 和放回逻辑，而放回逻辑在第 7 步用真实 Docker 走过；补订阅和写结果这部分只做了代码改动。
- **人工兜底（§4.3）只写了文档、没有演练**：需要人在 Portainer 界面操作。
- **放回路径的服务中断**：放回要等满 60 秒健康超时，这条失败路径约 66 秒。只在 1.1.1 起不来时才会走到。

## 四、兼容性声明

产物、配置、产品代码都没变（`cbc6b053…`，`config.release.yaml` sha `44a5e0e6…`），第二轮的兼容性结论不变。

## 五、需要对方做的事

1. **业务方**：把本报告和 `nas-198/swap-tools-since-67abacc.diff` 转主审。
2. **主审**：核对第二节必须修的那一项、第三节建议的改动，以及最终版本的演练证据（`nas-198/drill/`）。通过后告知业务方。
3. **换版**（主审通过后，业务方决定时间，不限时段）：
   - 避开 OMS 部署窗口，开始前确认 OMS 和 botd 都已稳定连上（`md_ws_active_clients` 为 2）；
   - 我提前告知开始时间，业务方打开 Portainer、botd 保持运行、换后在场约 15 分钟；
   - 采样用的 MacBook Pro 接电源、不合盖 24 小时；
   - 观察期内保留 `md-server-*-bak` 容器、`md-server:1.1.1` 镜像和临时账户；
   - 10 分钟验收若是 `pass_needs_botd_confirmation`，**不回退**，把清单转主审对照 botd 日志。
