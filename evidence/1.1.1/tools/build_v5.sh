#!/usr/bin/env bash
# v5 = v4 (A1+A2+A5) + jemalloc（--features md-server/jemalloc-stats）。补充对照：回答"换分配器能否让 RSS 随负载回落"。
set -eu
REPO=/Users/lihao/Desktop/web3/rust/rest重构
OUT=/tmp/md-variants; LOGS=$REPO/evidence/1.1.1/build-logs
( cd "$REPO" && CARGO_TARGET_DIR="$REPO/target" cargo zigbuild --release --target x86_64-unknown-linux-musl --features md-connector/vendored-tls,md-server/jemalloc-stats -p md-server ) > "$LOGS/zigbuild-v5.log" 2>&1
cp "$REPO/target/x86_64-unknown-linux-musl/release/md-server" "$OUT/md-server-v5"
m=$(strings -a "$OUT/md-server-v5" | grep -c 'write blocked longer than ws_write_wait' || true); j=$(strings -a "$OUT/md-server-v5" | grep -c md_jemalloc_ || true)
echo "v5 write_timeout_marker=$m jemalloc_marker=$j"; [ "$m" -ge 1 ] && [ "$j" -ge 1 ] || { echo "!! v5 校验失败"; exit 1; }
shasum -a 256 "$OUT/md-server-v5" | tee -a "$LOGS/variants-sha256.txt"
