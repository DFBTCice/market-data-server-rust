#!/usr/bin/env bash
# 1.2.0 最终产物：指定提交、干净 worktree、固定 SOURCE_DATE_EPOCH、同一路径连续两次从零构建（jemalloc 构建，与现网 1.1.1 同法）。
# 用法：bash evidence/1.2.0/tools/build_final.sh <提交>
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
COMMIT="$(git -C "$REPO" rev-parse "${1:?usage: build_final.sh <commit>}")"
SHORT="${COMMIT:0:7}"
WORKTREE="/tmp/md-final-$SHORT"
export CARGO_TARGET_DIR="/tmp/md-final-$SHORT-target"
export SOURCE_DATE_EPOCH="$(git -C "$REPO" show -s --format=%ct "$COMMIT")"
FEATURES=md-connector/vendored-tls,md-server/jemalloc-stats
LOGS="$REPO/evidence/1.2.0/build-logs"
DIST="$REPO/dist"
NAME="md-server-1.2.0-$SHORT"
mkdir -p "$LOGS" "$DIST"
git -C "$REPO" worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true
git -C "$REPO" worktree add --detach "$WORKTREE" "$COMMIT" >/dev/null
[[ "$(git -C "$WORKTREE" rev-parse HEAD)" == "$COMMIT" ]]
[[ -z "$(git -C "$WORKTREE" status --porcelain)" ]]
cd "$WORKTREE"
{
  echo "commit: $COMMIT dirty=0"
  echo "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH"
  echo "worktree=$WORKTREE"
  echo "CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
  echo "features=$FEATURES"
  rustc --version
  cargo --version
  cargo install --list | sed -n '/^cargo-zigbuild /p'
  zig version
  protoc --version
  echo "command: cargo zigbuild --locked --offline -p md-server --release --target x86_64-unknown-linux-musl --features $FEATURES"
} > "$LOGS/build-manifest.txt"
for attempt in 1 2; do
  rm -rf "$CARGO_TARGET_DIR"
  echo "build $attempt started $(date -u +%FT%TZ)"
  cargo zigbuild --locked --offline -p md-server --release \
    --target x86_64-unknown-linux-musl --features "$FEATURES" \
    > "$LOGS/final-jemalloc-build-$attempt.log" 2>&1
  cp "$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/release/md-server" "$DIST/$NAME-rebuild-$attempt-linux-x86_64"
  shasum -a 256 "$DIST/$NAME-rebuild-$attempt-linux-x86_64"
done
cmp "$DIST/$NAME-rebuild-1-linux-x86_64" "$DIST/$NAME-rebuild-2-linux-x86_64"
[[ -z "$(git -C "$WORKTREE" status --porcelain)" ]]
python3 - "$DIST/$NAME-rebuild-2-linux-x86_64" <<'PY'
import pathlib, sys
data = pathlib.Path(sys.argv[1]).read_bytes()
for marker in (b'md_jemalloc_', b'md_runtime_subscriptions_enabled', b'runtime subscription idle-unsubscribed',
               b'runtime subscription added on demand', b'md_ws_subscriptions_active', b'NOT applied'):
    assert marker in data, f'missing marker: {marker!r}'
    print('marker present:', marker.decode())
PY
cp "$DIST/$NAME-rebuild-2-linux-x86_64" "$DIST/$NAME-linux-x86_64"
{
  cat "$LOGS/build-manifest.txt"
  shasum -a 256 "$DIST/$NAME-rebuild-1-linux-x86_64" "$DIST/$NAME-rebuild-2-linux-x86_64" "$DIST/$NAME-linux-x86_64" | sed "s#$REPO/##"
  file "$DIST/$NAME-linux-x86_64" | sed "s#$REPO/##"
  echo "cmp: identical; dirty=0 after both builds"
} > "$LOGS/dist-sha256.txt"
git -C "$REPO" worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true
cat "$LOGS/dist-sha256.txt"
