#!/usr/bin/env bash
# 最终产物对应源码的门禁，完整 stdout/stderr 入库供主审复跑。
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="$REPO/evidence/1.1.1/final-artifact/verify"
mkdir -p "$OUT"
cd /tmp/md-final-a06fe20
export CARGO_TARGET_DIR="$REPO/target"
git rev-parse HEAD > "$OUT/commit.txt"
[[ -z "$(git status --porcelain)" ]]
cargo fmt --all --check > "$OUT/fmt.log" 2>&1
cargo clippy --locked --offline --workspace --all-targets > "$OUT/clippy.log" 2>&1
cargo test --locked --offline --workspace > "$OUT/workspace-test.log" 2>&1
cargo test --locked --offline -p md-tests -- --nocapture > "$OUT/md-tests.log" 2>&1
cargo test --locked --offline -p md-gateway --test compat_1_0_0 -- --nocapture > "$OUT/compat_1_0_0.log" 2>&1
node scripts/compat/compare.mjs selftest > "$OUT/compare-selftest.log" 2>&1
[[ -z "$(git status --porcelain)" ]]
echo 'fmt/clippy/workspace/md-tests/compat_1_0_0/selftest: completed'
