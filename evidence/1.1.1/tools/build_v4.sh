#!/usr/bin/env bash
# v4 = 1.1.1 + A5（Gateway WS 写超时：api_gateway.ws_write_wait），默认分配器（与生产镜像同构建）
# 用于旁路实验 E 组"停读客户端"的修复验证（对照 c11 = v1，同样的停读负载但没有写超时）。
set -eu
REPO=/Users/lihao/Desktop/web3/rust/rest重构
OUT=/tmp/md-variants; LOGS=$REPO/evidence/1.1.1/build-logs
mkdir -p "$OUT" "$LOGS"
( cd "$REPO" && CARGO_TARGET_DIR="$REPO/target" cargo zigbuild --release --target x86_64-unknown-linux-musl --features md-connector/vendored-tls -p md-server ) > "$LOGS/zigbuild-v4.log" 2>&1
cp "$REPO/target/x86_64-unknown-linux-musl/release/md-server" "$OUT/md-server-v4"
m=$(strings -a "$OUT/md-server-v4" | grep -c 'write blocked longer than ws_write_wait' || true)
echo "v4 write_timeout_marker=$m"; [ "$m" -ge 1 ] || { echo "!! v4 校验失败"; exit 1; }
shasum -a 256 "$OUT/md-server-v4" | tee -a "$LOGS/variants-sha256.txt"
