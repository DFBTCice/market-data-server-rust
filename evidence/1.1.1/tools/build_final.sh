#!/usr/bin/env bash
# docs/33：固定 a06fe20、干净 worktree、相同路径连续两次从零构建最终 jemalloc 产物。
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
COMMIT=a06fe2095e8fc58bff1e038a594b25e7002aff1b
WORKTREE=/tmp/md-final-a06fe20
export CARGO_TARGET_DIR=/tmp/md-final-a06fe20-target
export SOURCE_DATE_EPOCH="$(git -C "$REPO" show -s --format=%ct "$COMMIT")"
FEATURES=md-connector/vendored-tls,md-server/jemalloc-stats
LOGS="$REPO/evidence/1.1.1/build-logs"
OUT="$REPO/evidence/1.1.1/final-artifact"
mkdir -p "$LOGS" "$OUT" "$REPO/dist"
if [[ ! -d "$WORKTREE" ]]; then
  git -C "$REPO" worktree add --detach "$WORKTREE" "$COMMIT"
fi
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
} > "$OUT/build-manifest.txt"
for attempt in 1 2; do
  # 路径固定且专用于本次重建；OpenSSL 内嵌路径在两次构建间必须一致。
  rm -rf "$CARGO_TARGET_DIR"
  echo "build $attempt started $(date -u +%FT%TZ)"
  cargo zigbuild --locked --offline -p md-server --release \
    --target x86_64-unknown-linux-musl --features "$FEATURES" \
    > "$LOGS/final-jemalloc-build-$attempt.log" 2>&1
  cp "$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/release/md-server" "$REPO/dist/md-server-1.1.1-rebuild-$attempt-linux-x86_64"
  shasum -a 256 "$REPO/dist/md-server-1.1.1-rebuild-$attempt-linux-x86_64"
done
cmp "$REPO/dist/md-server-1.1.1-rebuild-1-linux-x86_64" "$REPO/dist/md-server-1.1.1-rebuild-2-linux-x86_64"
[[ -z "$(git -C "$WORKTREE" status --porcelain)" ]]
python3 - "$REPO/dist/md-server-1.1.1-rebuild-2-linux-x86_64" <<'PY'
import pathlib, sys
data = pathlib.Path(sys.argv[1]).read_bytes()
for marker in (b'md_jemalloc_', b'md_ws_subscriptions_active',
               b'write blocked longer than ws_write_wait', b'RUST_LOG', b'NOT applied'):
    assert marker in data, f'missing marker: {marker!r}'
    print('marker present:', marker.decode())
PY
cp "$REPO/dist/md-server-1.1.1-rebuild-2-linux-x86_64" "$REPO/dist/md-server-1.1.1-linux-x86_64"
{
  cat "$OUT/build-manifest.txt"
  shasum -a 256 "$REPO/dist/md-server-1.1.1-rebuild-1-linux-x86_64" "$REPO/dist/md-server-1.1.1-rebuild-2-linux-x86_64" "$REPO/dist/md-server-1.1.1-linux-x86_64"
  file "$REPO/dist/md-server-1.1.1-linux-x86_64"
  echo "cmp: identical; dirty=0 after both builds"
  echo "Supersedes historical dirty-tree/no-jemalloc artifacts; old hashes remain in git history."
} > "$LOGS/dist-sha256.txt"
cat "$LOGS/dist-sha256.txt"
