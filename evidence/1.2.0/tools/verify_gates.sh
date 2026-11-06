#!/usr/bin/env bash
# 门禁（任务书 §四 第一条）：在指定提交的干净 worktree 里跑 fmt / clippy / 全量测试 / md-tests / compat_1_0_0 / compare selftest，
# 完整输出入库供主审复跑。用法：bash evidence/1.2.0/tools/verify_gates.sh <提交> [输出目录]
set -uo pipefail
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
COMMIT="$(git -C "$REPO" rev-parse "${1:?usage: verify_gates.sh <commit> [outdir]}")"
OUT="${2:-$REPO/evidence/1.2.0/gates/${COMMIT:0:7}}"
WT="/tmp/md120-gates-${COMMIT:0:7}"
export CARGO_TARGET_DIR="/tmp/md120-gates-target"
mkdir -p "$OUT"
git -C "$REPO" worktree remove --force "$WT" >/dev/null 2>&1 || true
git -C "$REPO" worktree add --detach "$WT" "$COMMIT" >/dev/null
cd "$WT"
echo "$COMMIT" > "$OUT/commit.txt"
run() { local name=$1; shift; echo "\$ $*" > "$OUT/$name.log"; "$@" >> "$OUT/$name.log" 2>&1; echo "exit=$?" >> "$OUT/$name.log"; }
run fmt cargo fmt --all --check
run clippy cargo clippy --locked --workspace --all-targets
run workspace-test cargo test --locked --workspace
run md-tests cargo test --locked -p md-tests -- --nocapture
run compat_1_0_0 cargo test --locked -p md-gateway --test compat_1_0_0 -- --nocapture
run runtime_subs_gateway cargo test --locked -p md-gateway --test runtime_subs_gateway
run compare-selftest node scripts/compat/compare.mjs selftest
echo "dirty_after=$(git status --porcelain | wc -l | tr -d ' ')" > "$OUT/dirty.txt"
{
  echo "commit=$COMMIT"
  echo "fmt: $(tail -1 "$OUT/fmt.log"); diff lines=$(($(wc -l < "$OUT/fmt.log") - 2))"
  echo "clippy: $(tail -1 "$OUT/clippy.log"); warning lines=$(grep -c '^warning' "$OUT/clippy.log")"
  echo "workspace-test: $(tail -1 "$OUT/workspace-test.log"); $(grep -E '^test result' "$OUT/workspace-test.log" | awk '{p+=$4; f+=$6} END {print "passed="p" failed="f}')"
  for n in md-tests compat_1_0_0 runtime_subs_gateway; do
    echo "$n: $(tail -1 "$OUT/$n.log"); $(grep -E '^test result' "$OUT/$n.log" | tail -1)"
  done
  echo "compare-selftest: $(tail -1 "$OUT/compare-selftest.log"); $(grep -E 'selftest' "$OUT/compare-selftest.log" | tail -1)"
  cat "$OUT/dirty.txt"
} | tee "$OUT/summary.txt"
cd "$REPO"; git worktree remove --force "$WT" >/dev/null 2>&1 || true
