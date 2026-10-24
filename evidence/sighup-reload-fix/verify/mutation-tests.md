# 变异验证记录

生成时间：2026-10-17T07:23:11  脚本：`evidence/sighup-reload-fix/verify/run_mutations.py`（每次只改一处，跑完立即还原）

- A 类：`cargo test -p <crate>`，期望至少一个测试失败。
- B 类：日志过滤器接线类变异，单元测试覆盖不到（全局 subscriber 只能安装一次），期望端到端场景（真实进程 + 真实 SIGHUP）失败；同时如实记录单元测试结果。

## 对照：未改动

- `cargo test -p md-server`：退出码 0；通过 42，失败 0
- `cargo test -p md-config`：退出码 0；通过 14，失败 0

## M1 把文件配置整体记为已应用（即原 `current_cfg = new_cfg;`）

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-         self.applied = next;
-         report
+         self.applied = file;
+         report
```

- 编译：通过；退出码 101；通过 35，失败 7；**已被测试抓住**
- 失败的测试（7 个）：
  - `reload::tests::every_url_stays_pending_across_consecutive_reloads_and_clears_on_revert`
  - `reload::tests::failed_apply_keeps_old_list_and_is_retried_on_next_reload`
  - `reload::tests::log_level_failure_keeps_old_level_and_is_retried_on_next_reload`
  - `reload::tests::missing_connector_keeps_group_pending_until_reverted`
  - `reload::tests::pending_restart_items_accumulate_and_stay_visible`
  - `reload::tests::rust_log_override_keeps_log_level_pending_across_reloads_and_clears_on_revert`
  - `reload::tests::subscription_hot_update_coexists_with_pending_url_change`

## M2 漏掉 OKX Business 地址检测

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-         (
-             "connectors.okx.stream_base_url_business",
-             show(okx_stream_url_business),
-         ),
+ （删除）
```

- 编译：通过；退出码 101；通过 36，失败 6；**已被测试抓住**
- 失败的测试（6 个）：
  - `reload::tests::every_restart_only_field_is_detected_individually`
  - `reload::tests::every_url_stays_pending_across_consecutive_reloads_and_clears_on_revert`
  - `reload::tests::log_level_hot_update_coexists_with_url_pending_and_subscription_update`
  - `reload::tests::okx_business_url_only_change_requires_restart_and_is_not_success`
  - `reload::tests::subscription_hot_update_coexists_with_pending_url_change`
  - `reload::tests::url_changes_explain_that_reconnect_keeps_the_startup_address`

## M3 连接器缺失时仍视为已应用（静默吞掉）

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-         return GroupOutcome::NotRunning;
+         return GroupOutcome::Applied;
```

- 编译：通过；退出码 101；通过 40，失败 2；**已被测试抓住**
- 失败的测试（2 个）：
  - `reload::tests::missing_connector_keeps_group_pending_until_reverted`
  - `reload::tests::quote_group_noop_when_lists_equal_or_connector_missing`

## M4 订阅下发失败仍视为已应用（报成功、不重试）

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-         Some(e) => GroupOutcome::Failed(e),
+         Some(_e) => GroupOutcome::Applied,
```

- 编译：通过；退出码 101；通过 41，失败 1；**已被测试抓住**
- 失败的测试（1 个）：
  - `reload::tests::failed_apply_keeps_old_list_and_is_retried_on_next_reload`

## M5 订阅热更成功后不推进已应用列表（每次 SIGHUP 重复下发）

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
- GroupOutcome::Applied => group.adopt(&mut next, &file),
+ GroupOutcome::Applied => {}
```

- 编译：通过；退出码 101；通过 39，失败 3；**已被测试抓住**
- 失败的测试（3 个）：
  - `reload::tests::failed_apply_keeps_old_list_and_is_retried_on_next_reload`
  - `reload::tests::subscription_hot_update_applies_once_and_updates_applied_lists`
  - `reload::tests::subscription_hot_update_coexists_with_pending_url_change`

## M6 启动接线错误：OKX Business 连接器被接到 Public 地址

文件：`crates/md-server/src/main.rs`（被测：`-p md-server`）

```diff
-             stream_base_url_business: cfg.connectors.okx.stream_base_url_business.clone(),
+             stream_base_url_business: cfg.connectors.okx.stream_base_url_public.clone(),
```

- 编译：通过；退出码 101；通过 41，失败 1；**已被测试抓住**
- 失败的测试（1 个）：
  - `tests::start_connectors_connects_each_connector_to_its_configured_url`

## L1 RUST_LOG 覆盖时仍把文件里的 log_level 记为已应用（提示一次就消失）

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-                 Some(rust_log) => {
-                     log_pending = Some(PendingChange {
+                 Some(rust_log) => {
+                     next.log_level = file.log_level.clone();
+                     log_pending = Some(PendingChange {
```

- 编译：通过；退出码 101；通过 41，失败 1；**已被测试抓住**
- 失败的测试（1 个）：
  - `reload::tests::rust_log_override_keeps_log_level_pending_across_reloads_and_clears_on_revert`

## L2 log_level 变更时不真正切换过滤器，却记为已应用

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-                     match self.log.apply(&file.log_level) {
+                     match LevelOutcome::Applied {
```

- 编译：通过；退出码 101；通过 38，失败 4；**已被测试抓住**
- 失败的测试（4 个）：
  - `reload::tests::log_level_change_is_hot_applied_once_and_adopted`
  - `reload::tests::log_level_failure_keeps_old_level_and_is_retried_on_next_reload`
  - `reload::tests::log_level_hot_update_coexists_with_url_pending_and_subscription_update`
  - `reload::tests::sighup_applies_log_level_written_in_any_accepted_spelling`

## L3 过滤器切换失败仍记为已应用（不再重试）

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-                         LevelOutcome::Rejected(error) => {
-                             log_pending = Some(PendingChange {
+                         LevelOutcome::Rejected(error) => {
+                             next.log_level = file.log_level.clone();
+                             log_pending = Some(PendingChange {
```

- 编译：通过；退出码 101；通过 41，失败 1；**已被测试抓住**
- 失败的测试（1 个）：
  - `reload::tests::log_level_failure_keeps_old_level_and_is_retried_on_next_reload`

## L4 log_level 没变也每次 SIGHUP 都去切换过滤器

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-         if file.log_level != self.applied.log_level {
+         if true {
```

- 编译：通过；退出码 101；通过 39，失败 3；**已被测试抓住**
- 失败的测试（3 个）：
  - `reload::tests::log_level_change_is_hot_applied_once_and_adopted`
  - `reload::tests::rust_log_override_keeps_log_level_pending_across_reloads_and_clears_on_revert`
  - `reload::tests::unchanged_log_level_never_touches_the_filter`

## L5 不检查 RUST_LOG 是否覆盖，直接尝试切换过滤器

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-             match self.log.overridden_by() {
+             match None::<String> {
```

- 编译：通过；退出码 101；通过 41，失败 1；**已被测试抓住**
- 失败的测试（1 个）：
  - `reload::tests::rust_log_override_keeps_log_level_pending_across_reloads_and_clears_on_revert`

## L6 生效配置里的 log_level 不做规范化（INFO / warning 等写法与运行值比较出假变更）

文件：`crates/md-server/src/reload.rs`（被测：`-p md-server`）

```diff
-     canonicalize_log_level(&mut cfg)?;
-     Ok(cfg)
+     Ok(cfg)
```

- 编译：通过；退出码 101；通过 40，失败 2；**已被测试抓住**
- 失败的测试（2 个）：
  - `reload::tests::effective_config_canonicalizes_log_level_spellings`
  - `reload::tests::sighup_applies_log_level_written_in_any_accepted_spelling`

## L7 启动时忽略 RUST_LOG（环境变量不再优先）

文件：`crates/md-server/src/logging.rs`（被测：`-p md-server`）

```diff
-     let Some(raw) = rust_log.map(str::trim).filter(|s| !s.is_empty()) else {
+     let Some(raw) = None::<&str> else {
```

- 编译：通过；退出码 101；通过 38，失败 4；**已被测试抓住**
- 失败的测试（4 个）：
  - `logging::tests::invalid_rust_log_falls_back_to_config_and_says_so`
  - `logging::tests::plain_rust_log_that_agrees_with_config_is_quiet`
  - `logging::tests::plain_rust_log_that_disagrees_with_config_warns_loudly`
  - `logging::tests::rust_log_takes_precedence_over_config`

## L8 启动时忽略 YAML 的 log_level（永远用 info）

文件：`crates/md-server/src/logging.rs`（被测：`-p md-server`）

```diff
-         directives: config_level.to_string(),
-         source: FilterSource::Config,
+         directives: "info".to_string(),
+         source: FilterSource::Config,
```

- 编译：通过；退出码 101；通过 40，失败 2；**已被测试抓住**
- 失败的测试（2 个）：
  - `logging::tests::config_level_is_used_when_rust_log_is_unset_or_blank`
  - `logging::tests::invalid_rust_log_falls_back_to_config_and_says_so`

## L9 RUST_LOG 与 YAML 的 log_level 不一致时不再告警

文件：`crates/md-server/src/logging.rs`（被测：`-p md-server`）

```diff
-         if plain != config_level {
+         if false {
```

- 编译：通过；退出码 101；通过 41，失败 1；**已被测试抓住**
- 失败的测试（1 个）：
  - `logging::tests::plain_rust_log_that_disagrees_with_config_warns_loudly`

## L10 热更时不检查 RUST_LOG 覆盖（会改动用户用环境变量固定的过滤器）

文件：`crates/md-server/src/logging.rs`（被测：`-p md-server`）

```diff
-         if let Some(raw) = self.overridden_by() {
-             return LevelOutcome::Rejected(
+         if let Some(raw) = None::<String> {
+             return LevelOutcome::Rejected(
```

- 编译：通过；退出码 101；通过 41，失败 1；**已被测试抓住**
- 失败的测试（1 个）：
  - `logging::tests::rust_log_overrides_and_is_never_touched_by_apply`

## L11 热更时只返回成功、不真正重载过滤器

文件：`crates/md-server/src/logging.rs`（被测：`-p md-server`）

```diff
-         match self.handle.reload(filter) {
+         match { let _ = filter; Ok::<(), reload::Error>(()) } {
```

- 编译：通过；退出码 101；通过 39，失败 3；**已被测试抓住**
- 失败的测试（3 个）：
  - `logging::tests::apply_after_subscriber_is_gone_is_reported_not_panicked`
  - `logging::tests::current_filter_reflects_last_applied_level`
  - `logging::tests::hot_apply_changes_which_events_pass_without_reinstalling_the_subscriber`

## L12 YAML 里的非法 log_level 不再被校验拒绝

文件：`crates/md-config/src/lib.rs`（被测：`-p md-config`）

```diff
-     if normalize_log_level(&cfg.log_level).is_none() {
+     if false && normalize_log_level(&cfg.log_level).is_none() {
```

- 编译：通过；退出码 101；通过 13，失败 1；**已被测试抓住**
- 失败的测试（1 个）：
  - `tests::invalid_log_level_in_yaml_is_a_validation_error`

## L13 任何拼写都被当成合法级别（`fatal`、`verbose` 悄悄变成 info）

文件：`crates/md-config/src/lib.rs`（被测：`-p md-config`）

```diff
-         "error" => Some("error"),
-         _ => None,
+         "error" => Some("error"),
+         _ => Some("info"),
```

- 编译：通过；退出码 101；通过 12，失败 2；**已被测试抓住**
- 失败的测试（2 个）：
  - `tests::invalid_log_level_in_yaml_is_a_validation_error`
  - `tests::normalize_log_level_rejects_everything_else`

## 对照：未改动（端到端）

- 场景 s9a-log-level-hot-apply, s9b-log-level-from-config-at-startup, s9c-rust-log-takes-precedence, s9d-invalid-log-level-rejected, s9e-log-level-env-override, s9f-prod-198-combination：41 条断言，失败 0 条

## B1 `logging::init` 不使用启动方案，过滤器永远是 info（等于旧版本行为）

文件：`crates/md-server/src/logging.rs`

```diff
-     let filter = EnvFilter::try_new(&plan.directives).unwrap_or_else(|_| EnvFilter::new("info"));
+     let filter = EnvFilter::new("info");
```

- 编译：通过
- 单元测试（`cargo test -p md-server`）：通过 42，失败 0（单元测试抓不住这类接线变异，符合预期）
- 端到端：41 条断言，失败 7 条；**已被端到端抓住**
  失败的断言：
  - s9b-log-level-from-config-at-startup：启动阶段没有任何 INFO 日志（旧版本这里会输出全部 INFO）
  - s9b-log-level-from-config-at-startup：无变更 SIGHUP：安静
  - s9e-log-level-env-override：启动：按 LOG_LEVEL=warn 生效（没有 INFO 日志）
  - s9e-log-level-env-override：无变更 SIGHUP：安静，不误报 log_level 变更
  - s9e-log-level-env-override：YAML 改动被环境变量覆盖：生效配置不变，不输出任何 INFO/WARN/ERROR
  - s9f-prod-198-combination：启动：生效的仍是 RUST_LOG=warn（没有 INFO 日志，行为与升级前一致）
  - s9f-prod-198-combination：无变更 SIGHUP：安静

## B2 `main` 初始化日志时传错级别（不用配置里的 log_level）

文件：`crates/md-server/src/main.rs`

```diff
- logging::init(&cfg.log_level)
+ logging::init("info")
```

- 编译：通过
- 单元测试（`cargo test -p md-server`）：通过 42，失败 0（单元测试抓不住这类接线变异，符合预期）
- 端到端：41 条断言，失败 6 条；**已被端到端抓住**
  失败的断言：
  - s9b-log-level-from-config-at-startup：启动阶段没有任何 INFO 日志（旧版本这里会输出全部 INFO）
  - s9b-log-level-from-config-at-startup：无变更 SIGHUP：安静
  - s9c-rust-log-takes-precedence：启动：WARN 点名 YAML 的 log_level 被 RUST_LOG 覆盖
  - s9e-log-level-env-override：启动：按 LOG_LEVEL=warn 生效（没有 INFO 日志）
  - s9e-log-level-env-override：无变更 SIGHUP：安静，不误报 log_level 变更
  - s9e-log-level-env-override：YAML 改动被环境变量覆盖：生效配置不变，不输出任何 INFO/WARN/ERROR

## 还原核对

- 与备份逐字节一致：是
- `git status --porcelain`（这几个源文件）为空：是

## 结论

共 21 个变异，抓住 21 个，全部被抓住。
