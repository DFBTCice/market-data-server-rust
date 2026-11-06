#!/usr/bin/env bash
# 正对照（任务书 §四）：把关键判定逐个故意改坏，相应测试必须失败；最后原样代码必须全过。
# 在临时 worktree（指定提交、干净）里改，不碰工作区。
# 用法：bash evidence/1.2.0/tools/run_mutations.sh [提交，默认 HEAD] [输出目录]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
REV="${1:-HEAD}"
OUT="${2:-$ROOT/evidence/1.2.0/positive-controls}"
WT="/tmp/md120-mutation-wt"
export CARGO_TARGET_DIR="/tmp/md120-mutation-target"
mkdir -p "$OUT"
cd "$ROOT"
git worktree remove --force "$WT" >/dev/null 2>&1 || true
git worktree add --detach "$WT" "$REV" >/dev/null
COMMIT="$(git -C "$WT" rev-parse HEAD)"
SUMMARY="$OUT/summary.txt"
: > "$SUMMARY"
echo "commit=$COMMIT" | tee -a "$SUMMARY"

RS="crates/md-gateway/src/runtime_subs.rs"
GW_TEST=(-p md-gateway --test runtime_subs_gateway)

# 名称 | 文件 | perl 替换（-0 整文件） | cargo test 参数（多次调用用 ; 分隔） | 必须失败的测试（空格分隔）
MUTATIONS=(
  "M1-receivers-ignored|$RS|s/let receivers = self\.processor\.receiver_count\(&k\.topic\(\)\);/let receivers = 0usize;/|${GW_TEST[*]}|ws_receiver_and_quick_reconnect_keep_subscription legacy_ws_receiver_keeps_subscription"
  "M2-config-not-protected|$RS|s/if view\.protected\.contains\(k\) \{\s*continue;\s*\}//|${GW_TEST[*]}|config_subscription_is_never_unsubscribed"
  "M3-grace-ignored|$RS|s/let in_grace = now\.saturating_duration_since\(st\.enabled_at\) < s\.startup_grace;/let in_grace = false;/|${GW_TEST[*]}|startup_grace_defers_idle_unsubscribe hot_toggle_takes_effect_without_restart"
  "M4-rest-reads-ignored|$RS|s/bump\(&mut st\.last_read, k, at\);/let _ = (k, at);/|${GW_TEST[*]}|rest_reads_refresh_idle_timer"
  "M5-no-reject-backoff|$RS|s/if now < r\.retry_at \{/if false \&\& now < r.retry_at {/|${GW_TEST[*]}|upstream_rejected_symbol_is_not_retried"
  "M6-no-limit|$RS|s/if runtime_count >= s\.max_runtime_subscriptions \{/if false {/|${GW_TEST[*]}|limit_refuses_on_demand_and_counts"
  "M7-no-on-demand|$RS|s/for raw in self\.processor\.topics_with_receivers\(\) \{/for raw in Vec::<String>::new() {/|${GW_TEST[*]}|auto_unsubscribe_then_on_demand_resubscribe_on_same_connection_without_duplicates subscribing_a_topic_missing_upstream_subscribes_on_demand delete_while_receiving_is_restored_on_demand"
  "M8-switch-ignored|$RS|s/if s\.enabled \&\& !was_enabled \{/if !was_enabled {/|${GW_TEST[*]}|switch_off_changes_nothing"
  "M9-unsubscribe-without-idle|$RS|s/if idle_for >= s\.idle \{/if idle_for >= Duration::ZERO {/|${GW_TEST[*]}|idle_runtime_subscription_is_unsubscribed_upstream rest_reads_refresh_idle_timer"
  "M10-ws-unsubscribe-keeps-receiver-1.1.1-regression|crates/md-gateway/src/ws.rs|s/let released = subscriptions\.remove\(stream\)\.is_some\(\);/let released = subscriptions.contains_key(stream);/|-p md-gateway --test runtime_subs_gateway --test unsubscribe_gateway|auto_unsubscribe_then_on_demand_resubscribe_on_same_connection_without_duplicates resubscribe_after_unsubscribe_delivers_exactly_one_copy"
  "M11-grpc-keeps-receiver-after-disconnect|crates/md-grpc/src/market_data.rs|s/_ = tx\.closed\(\) => break,/_ = std::future::pending::<()>() => break,/g|-p md-grpc --lib|grpc_stream_counts_as_receiver_and_releases_on_disconnect"
  "M12-okx-error-stream-not-parsed|crates/md-connector/src/okx.rs|s/stream_from_error_msg\(&message\)\.unwrap_or_default\(\)/String::new()/|-p md-connector --lib;${GW_TEST[*]}|subscribe_error_without_arg_carries_stream_from_msg upstream_rejected_symbol_is_not_retried"
  "M13-sighup-does-not-apply-runtime-section|crates/md-server/src/reload.rs|s/next\.runtime_subscriptions = file\.runtime_subscriptions\.clone\(\);/let _ = \&file;/|-p md-server|runtime_subscriptions_change_is_hot_applied_and_pushed"
  "M14-quote-on-demand-ignores-off-switch|$RS|s/if k\.data_type == DataType::Quote \&\& !s\.on_demand_quote \{/if false \&\& k.data_type == DataType::Quote {/|${GW_TEST[*]}|quote_topics_are_not_subscribed_on_demand_by_default"
  "M15-quote-never-on-demand-even-when-on|$RS|s/if k\.data_type == DataType::Quote \&\& !s\.on_demand_quote \{/if k.data_type == DataType::Quote {/|${GW_TEST[*]}|quote_topics_are_subscribed_on_demand_when_enabled"
  "M16-yaml-on-demand-quote-not-mapped|crates/md-server/src/runtime_subs.rs|s/on_demand_quote: rs\.on_demand_quote,/on_demand_quote: false,/|-p md-server|settings_follow_yaml"
)

overall=0
for m in "${MUTATIONS[@]}"; do
  IFS='|' read -r name file expr args expect <<< "$m"
  git -C "$WT" checkout -q -- .
  before="$(shasum "$WT/$file" | cut -d' ' -f1)"
  perl -0pi -e "$expr" "$WT/$file"
  after="$(shasum "$WT/$file" | cut -d' ' -f1)"
  log="$OUT/$name.log"
  if [ "$before" = "$after" ]; then
    echo "$name: MUTATION NOT APPLIED (pattern not found)" | tee -a "$SUMMARY"; overall=1; continue
  fi
  git -C "$WT" diff > "$OUT/$name.diff"
  : > "$log"
  code=0
  IFS=';' read -ra invocations <<< "$args"
  for inv in "${invocations[@]}"; do
    echo "\$ cargo test --no-fail-fast $inv -- --test-threads=4" >> "$log"
    # shellcheck disable=SC2086
    (cd "$WT" && cargo test --no-fail-fast $inv -- --test-threads=4) >> "$log" 2>&1 || code=$?
  done
  failed="$(grep -E '^test .* \.\.\. FAILED$' "$log" | sed -E 's/^test (.*) \.\.\. FAILED$/\1/' | sed -E 's/.*:://' | sort -u | tr '\n' ' ')"
  missing=""
  for t in $expect; do
    grep -qE "^test (.*::)?$t \.\.\. FAILED$" "$log" || missing="$missing $t"
  done
  if grep -q "error\[E" "$log"; then
    verdict="COMPILE-ERROR (not a valid control)"; overall=1
  elif [ $code -ne 0 ] && [ -z "$missing" ]; then
    verdict="CAUGHT"
  else
    verdict="NOT CAUGHT (missing:$missing)"; overall=1
  fi
  echo "$name: $verdict; exit=$code; failed=[$failed]" | tee -a "$SUMMARY"
done

git -C "$WT" checkout -q -- .
(cd "$WT" && cargo test --workspace) > "$OUT/M0-unmutated.log" 2>&1
code=$?
echo "M0-unmutated: exit=$code; $(grep -E '^test result' "$OUT/M0-unmutated.log" | awk '{p+=$4; f+=$6} END {print "passed="p" failed="f}')" | tee -a "$SUMMARY"
[ $code -eq 0 ] || overall=1
git worktree remove --force "$WT" >/dev/null 2>&1 || true
echo "overall=$([ $overall -eq 0 ] && echo PASS || echo FAIL)" | tee -a "$SUMMARY"
exit $overall
