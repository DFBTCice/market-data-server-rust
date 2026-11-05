#!/usr/bin/env bash
# 构建 D1/D3 对照实验用的 4 个静态 musl 二进制（仅旁路诊断用，不用于生产）：
#   v0 = 1.1.0 网关逻辑 + 仪表（gauge/tokio 指标/调试重连开关），默认分配器   <- 正向对照（应当增长）
#   v1 = 1.1.1（修复后），默认分配器（与生产镜像相同构建）                     <- 候选
#   v2 = v0 + jemalloc-stats（allocated/resident 区分泄漏 vs 碎片）
#   v3 = v1 + jemalloc-stats
set -eu
REPO=/Users/lihao/Desktop/web3/rust/rest重构
WT0=/tmp/md-wt-v0
OUT=/tmp/md-variants
LOGS=$REPO/evidence/1.1.1/build-logs
mkdir -p "$OUT" "$LOGS"
# 注意：不同源码树必须使用各自独立的 target 目录。工作区成员 crate 的 metadata 哈希与路径无关，
# 共用 target 会让 cargo 复用另一棵树编出的 md-gateway rlib（首次运行时 v3/v2 就因此串了代码，已用 strings 校验发现并修正）。
build() { # name srcdir targetdir marker(应出现于二进制:1 / 不应出现:0) [extra features]
  local name=$1 src=$2 tdir=$3 want_fix=$4 feat="md-connector/vendored-tls${5:+,$5}"
  echo "=== build $name ($src) target=$tdir features=$feat $(date -u +%FT%TZ)"
  ( cd "$src" && CARGO_TARGET_DIR="$tdir" cargo zigbuild --release --target x86_64-unknown-linux-musl --features "$feat" -p md-server ) > "$LOGS/zigbuild-$name.log" 2>&1
  cp "$tdir/x86_64-unknown-linux-musl/release/md-server" "$OUT/md-server-$name"
  local has_fix; has_fix=$(strings -a "$OUT/md-server-$name" | grep -c 'gateway ws subscribe stream' || true)
  if [ "$has_fix" != "$want_fix" ] && { [ "$want_fix" = 0 ] || [ "$has_fix" = 0 ]; }; then echo "!! $name 校验失败: fix 标记=$has_fix 期望=$want_fix"; exit 1; fi
  echo "$name fix_marker=$has_fix jemalloc_marker=$(strings -a "$OUT/md-server-$name" | grep -c md_jemalloc_ || true)"
  shasum -a 256 "$OUT/md-server-$name" | tee -a "$LOGS/variants-sha256.txt"
}
: > "$LOGS/variants-sha256.txt"
T1=$REPO/target
T0=/tmp/md-target-v0
build v1 "$REPO" "$T1" 1
build v3 "$REPO" "$T1" 1 md-server/jemalloc-stats
build v0 "$WT0" "$T0" 0
build v2 "$WT0" "$T0" 0 md-server/jemalloc-stats
echo "ALL DONE $(date -u +%FT%TZ)"
