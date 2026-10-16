# 变异验证记录
生成时间：2026-10-17T06:24:35  命令：`cargo test -p md-server`（每次只改一处，跑完立即还原）
## 对照：未改动

- 退出码 0；[('ok', '21', '0')]
## M1 把文件配置整体记为已应用（即原 `current_cfg = new_cfg;`）

- 编译：通过；退出码 101；测试结果 [('FAILED', '16', '5')]
- 失败的测试（5 个）：
  - `reload::tests::every_url_stays_pending_across_consecutive_reloads_and_clears_on_revert`
  - `reload::tests::failed_apply_keeps_old_list_and_is_retried_on_next_reload`
  - `reload::tests::missing_connector_keeps_group_pending_until_reverted`
  - `reload::tests::pending_restart_items_accumulate_and_stay_visible`
  - `reload::tests::subscription_hot_update_coexists_with_pending_url_change`

## M2 漏掉 OKX Business 地址检测

- 编译：通过；退出码 101；测试结果 [('FAILED', '16', '5')]
- 失败的测试（5 个）：
  - `reload::tests::every_restart_only_field_is_detected_individually`
  - `reload::tests::every_url_stays_pending_across_consecutive_reloads_and_clears_on_revert`
  - `reload::tests::okx_business_url_only_change_requires_restart_and_is_not_success`
  - `reload::tests::subscription_hot_update_coexists_with_pending_url_change`
  - `reload::tests::url_changes_explain_that_reconnect_keeps_the_startup_address`

## M3 连接器缺失时仍视为已应用（静默吞掉）

- 编译：通过；退出码 101；测试结果 [('FAILED', '19', '2')]
- 失败的测试（2 个）：
  - `reload::tests::missing_connector_keeps_group_pending_until_reverted`
  - `reload::tests::quote_group_noop_when_lists_equal_or_connector_missing`

## M4 订阅下发失败仍视为已应用（报成功、不重试）

- 编译：通过；退出码 101；测试结果 [('FAILED', '20', '1')]
- 失败的测试（1 个）：
  - `reload::tests::failed_apply_keeps_old_list_and_is_retried_on_next_reload`

## M5 订阅热更成功后不推进已应用列表（每次 SIGHUP 重复下发）

- 编译：通过；退出码 101；测试结果 [('FAILED', '18', '3')]
- 失败的测试（3 个）：
  - `reload::tests::failed_apply_keeps_old_list_and_is_retried_on_next_reload`
  - `reload::tests::subscription_hot_update_applies_once_and_updates_applied_lists`
  - `reload::tests::subscription_hot_update_coexists_with_pending_url_change`

## M6 启动接线错误：OKX Business 连接器被接到 Public 地址

- 编译：通过；退出码 101；测试结果 [('FAILED', '20', '1')]
- 失败的测试（1 个）：
  - `tests::start_connectors_connects_each_connector_to_its_configured_url`

