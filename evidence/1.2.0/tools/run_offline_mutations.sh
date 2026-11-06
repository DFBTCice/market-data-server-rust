#!/usr/bin/env bash
# 离线测试的正对照：在临时目录里把 swaplib.py / check_after_swap.py 逐个改坏，test_swaplib_offline.py 必须失败，且失败项包含预期项；
# 最后原样代码必须全过。临时目录镜像仓库结构（证据目录用软链接指回仓库，只读），不碰工作区。
# 用法：bash evidence/1.2.0/tools/run_offline_mutations.sh [输出目录]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="${1:-$ROOT/evidence/1.2.0/positive-controls/offline-tools}"
TMP="/tmp/md120-offline-mut"
mkdir -p "$OUT"
SUMMARY="$OUT/summary.txt"
echo "tools_commit=$(git -C "$ROOT" rev-parse HEAD) tools_dirty=$(git -C "$ROOT" status --porcelain -- evidence/1.2.0/tools | wc -l | tr -d ' ')" | tee "$SUMMARY"

# 名称 | 文件 | perl 替换（-0 整文件） | 必须为 false 的用例（空格分隔）
MUTATIONS=(
  'O1-no-bak-image-check|swaplib.py|s/\n\s*assert bak\["Image"\] == OLD_IMAGE_ID, f"\{target\.bak\} [^\n]*//|bak_wrong_image_aborts_before_touching_current'
  'O2-no-bak-stopped-check|swaplib.py|s/\n\s*assert not bak\["State"\]\["Running"\][^\n]*//|bak_running_aborts_before_touching_current'
  'O3-no-bak-params-check|swaplib.py|s/assert bak\[section\]\[field\] == old\[section\]\[field\], f"\{target\.bak\}[^\n]*/pass/|bak_wrong_cmd_aborts_before_touching_current'
  'O4-mismatched-recreated-bak-kept|swaplib.py|s/\n\s*call\("DELETE", f"\{D\}\/containers\/\{created\[.Id.\]\}\?force=1"\)//|recreated_bak_mismatch_is_deleted_and_current_untouched'
  'O5-ensure-bak-after-stop|swaplib.py|s/\n    bak_status = ensure_bak\(target, inspect_path, log\)//; s/(\n        call\("POST", f"\{D\}\/containers\/\{cur\[.Id.\]\}\/stop\?t=15"\)\n)/$1        bak_status = ensure_bak(target, inspect_path, log)\n/|bak_missing_recreated_then_switched bak_wrong_image_aborts_before_touching_current'
  'O6-no-put-back|swaplib.py|s/\n            _put_back\(target, cur, cur_name\)\n/\n            pass\n/|unhealthy_111_puts_previous_back stop_call_errors_after_stopping_restarts_current'
  'O7-put-back-stop-rejects-304|swaplib.py|s/(\/stop\?t=15", )ok=\(204, 304\)\)/$1)/|start_of_111_fails_puts_previous_back'
  'O8-put-back-keeps-restart-policy-no|swaplib.py|s/\n    call\("POST", f"\{D\}\/containers\/\{cur\[.Id.\]\}\/update", \{"RestartPolicy": cur\["HostConfig"\]\["RestartPolicy"\]\}\)//|stop_call_errors_before_stopping_restores_restart_policy'
  'O9-put-back-never-starts|swaplib.py|s/\n    if not call\("GET", f"\{D\}\/containers\/\{cur\[.Id.\]\}\/json"\)\["State"\]\["Running"\]:\n        call\([^\n]*//|stop_call_errors_after_stopping_restarts_current'
  'O10-failed-put-back-reported-as-restored|swaplib.py|s/restored=False\)/restored=True)/|put_back_failure_reports_manual_recovery'
  'C1-unexplained-tick-quote-excused|check_after_swap.py|s/ and k\[1\] != 2\]/ and False]/|ten_minute_unexplained_tick_fails ten_minute_unexplained_quote_fails'
  'C2-unexplained-kline-counted-as-failure|check_after_swap.py|s/ and k\[1\] != 2\]/]/|ten_minute_unexplained_runtime_kline_needs_botd_not_fail'
  'C3-idle-counter-not-checked|check_after_swap.py|s/ and idle_total >= len\(explained\)//|ten_minute_counter_below_explained_fails'
  'C4-config-kline-excused|check_after_swap.py|s/missing_config = sorted\(expected_config - now_keys\)/missing_config = sorted(k for k in expected_config - now_keys if k[1] != 2)/|ten_minute_missing_config_kline_fails'
)

prepare() {
  rm -rf "$TMP"
  mkdir -p "$TMP/evidence/1.2.0"
  cp -R "$ROOT/evidence/1.2.0/tools" "$TMP/evidence/1.2.0/tools"
  rm -rf "$TMP/evidence/1.2.0/tools/__pycache__"
  ln -s "$ROOT/evidence/1.2.0/build-logs" "$TMP/evidence/1.2.0/build-logs"
  ln -s "$ROOT/evidence/1.2.0/nas-198" "$TMP/evidence/1.2.0/nas-198"
}

false_cases() {
  python3 - "$1" <<'PY'
import json, re, sys
text = open(sys.argv[1]).read()
m = re.search(r"^\{\n.*?^\}$", text, re.S | re.M)
print(" ".join(k for k, v in json.loads(m.group(0)).items() if not v) if m else "NO-RESULTS")
PY
}

overall=0
for m in "${MUTATIONS[@]}"; do
  IFS='|' read -r name file expr expect <<< "$m"
  prepare
  f="$TMP/evidence/1.2.0/tools/$file"
  cp "$f" "$f.orig"
  perl -0pi -e "$expr" "$f"
  if cmp -s "$f" "$f.orig"; then
    echo "$name: NOT APPLIED" | tee -a "$SUMMARY"; overall=1; continue
  fi
  (cd "$TMP" && python3 evidence/1.2.0/tools/test_swaplib_offline.py) > "$OUT/$name.log" 2>&1
  code=$?
  failed="$(false_cases "$OUT/$name.log")"
  missing=""
  for e in $expect; do [[ " $failed " == *" $e "* ]] || missing="$missing $e"; done
  if [ $code -ne 0 ] && [ -z "$missing" ]; then verdict="CAUGHT"; else verdict="NOT CAUGHT (missing:$missing)"; overall=1; fi
  echo "$name: $verdict; exit=$code; false=[$failed]" | tee -a "$SUMMARY"
done

prepare
(cd "$TMP" && python3 evidence/1.2.0/tools/test_swaplib_offline.py) > "$OUT/unmutated.log" 2>&1
code=$?
echo "unmutated: exit=$code; false=[$(false_cases "$OUT/unmutated.log")]" | tee -a "$SUMMARY"
[ $code -eq 0 ] || overall=1
rm -rf "$TMP"
echo "overall=$([ $overall -eq 0 ] && echo PASS || echo FAIL)" | tee -a "$SUMMARY"
exit $overall
