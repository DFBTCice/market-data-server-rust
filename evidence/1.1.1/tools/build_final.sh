#!/usr/bin/env bash
# 最终发布候选的构建（1.1.1）：从当前提交构建两个 x86_64 musl 静态二进制，并做标记字符串校验：
#   A  dist/md-server-1.1.1-linux-x86_64                    jemalloc 分配器（推荐候选，对应旁路 v5）
#   B  dist/md-server-1.1.1-default-alloc-linux-x86_64      默认分配器（备选，对应旁路 v4）
# 校验：两者都必须含 "write blocked longer than ws_write_wait"（A5）与 "md_ws_subscriptions_active"（A3）；
#       只有 A 含 "md_jemalloc_"。结果追加到 evidence/1.1.1/build-logs/dist-sha256.txt，
#       并与旁路实验用的 v4/v5 二进制比较 sha256（相同 = 旁路 ≥6h 的结论逐字节适用于发布产物）。
set -eu
REPO=/Users/lihao/Desktop/web3/rust/rest重构
LOGS=$REPO/evidence/1.1.1/build-logs
mkdir -p "$REPO/dist"
cd "$REPO"
echo "commit: $(git rev-parse HEAD) dirty=$(git status --porcelain -- crates Cargo.toml Cargo.lock | wc -l)" | tee "$LOGS/dist-sha256.txt"

build() { # $1=features $2=out
  cargo zigbuild --release --target x86_64-unknown-linux-musl --features "$1" -p md-server
  cp target/x86_64-unknown-linux-musl/release/md-server "$2"
}
build md-connector/vendored-tls,md-server/jemalloc-stats dist/md-server-1.1.1-linux-x86_64 > "$LOGS/zigbuild-final-jemalloc.log" 2>&1
build md-connector/vendored-tls                           dist/md-server-1.1.1-default-alloc-linux-x86_64 > "$LOGS/zigbuild-final-default.log" 2>&1

chk() { # $1=file $2=pattern -> count
  strings -a "$1" | grep -c "$2" || true
}
for f in dist/md-server-1.1.1-linux-x86_64 dist/md-server-1.1.1-default-alloc-linux-x86_64; do
  echo "$f write_timeout=$(chk $f 'write blocked longer than ws_write_wait') subs_gauge=$(chk $f md_ws_subscriptions_active) jemalloc=$(chk $f md_jemalloc_)" | tee -a "$LOGS/dist-sha256.txt"
done
shasum -a 256 dist/md-server-1.1.1-linux-x86_64 dist/md-server-1.1.1-default-alloc-linux-x86_64 | tee -a "$LOGS/dist-sha256.txt"
echo "--- 旁路实验二进制（对照）" | tee -a "$LOGS/dist-sha256.txt"
shasum -a 256 /tmp/md-variants/md-server-v5 /tmp/md-variants/md-server-v4 2>/dev/null | tee -a "$LOGS/dist-sha256.txt" || true
file dist/md-server-1.1.1-linux-x86_64 | tee -a "$LOGS/dist-sha256.txt"
