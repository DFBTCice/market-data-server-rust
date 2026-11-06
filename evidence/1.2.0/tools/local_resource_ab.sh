#!/usr/bin/env bash
# docs/24 §三-8：开关关闭时 CPU / 内存 / 连接数与 1.1.1 同量级（本机原生构建，同配置、同订阅、同一消费者负载，同时运行）。
# macOS 上 /metrics 的 process_* 恒为 0（仅 Linux 有值），这里用 ps / lsof 采样。依赖 local_compat.sh 已构建的两个二进制。
# 用法：bash evidence/1.2.0/tools/local_resource_ab.sh [秒数=180] [输出目录]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SECS="${1:-180}"
OUT="${2:-$ROOT/evidence/1.2.0/resource-local-switch-off}"
OLD_BIN=/tmp/md120-old-target/release/md-server; NEW_BIN=/tmp/md120-new-target/release/md-server
mkdir -p "$OUT"
cd "$ROOT"
{ echo "old $(shasum -a 256 $OLD_BIN | cut -d' ' -f1)"; echo "new $(shasum -a 256 $NEW_BIN | cut -d' ' -f1)"; } | tee "$OUT/binaries.txt"
CFG=evidence/1.1.1/final-artifact/config.final.yaml
RUST_LOG=warn $OLD_BIN --config $CFG --port-offset 4000 > "$OUT/old.server.log" 2>&1 & OLD=$!
RUST_LOG=warn $NEW_BIN --config $CFG --port-offset 5000 > "$OUT/new.server.log" 2>&1 & NEW=$!
trap 'kill $OLD $NEW 2>/dev/null; wait $OLD $NEW 2>/dev/null' EXIT
sleep 12
T="tick.binance.BTCUSDT tick.binance.ETHUSDT tick.binance.BNBUSDT tick.okx.BTC-USDT-SWAP tick.okx.ETH-USDT-SWAP tick.okx.BNB-USDT-SWAP kline.5m.binance.BTCUSDT kline.5m.binance.ETHUSDT kline.5m.okx.BTC-USDT-SWAP kline.5m.okx.ETH-USDT-SWAP quote.binance.BNBUSDT quote.okx.BNB-USDT-SWAP"
CAP=()
for side in old:12081 new:13081; do
  n=${side%%:*}; p=${side##*:}
  node scripts/compat/restore_subs.mjs evidence/1.2.0/preflight/198.subscriptions.json "http://127.0.0.1:$p" --apply | tail -1
  # shellcheck disable=SC2086
  node scripts/compat/capture_ws.mjs gateway "ws://127.0.0.1:$p/ws/v1/data" "$SECS" "/tmp/md120-resource-$n" $T > "$OUT/$n.consumer.log" 2>&1 & CAP+=($!)
done
echo "t_s,side,rss_kib,cpu_pct,cpu_time,established_conns" > "$OUT/samples.csv"
start=$(date +%s)
while [ $(( $(date +%s) - start )) -lt "$SECS" ]; do
  for side in old:$OLD new:$NEW; do
    n=${side%%:*}; pid=${side##*:}
    read -r rss cpu ctime <<< "$(ps -o rss=,%cpu=,time= -p "$pid")"
    conns=$(lsof -nP -a -p "$pid" -iTCP -sTCP:ESTABLISHED 2>/dev/null | awk 'NR>1 && $9 !~ /127\.0\.0\.1:[0-9]+->127\.0\.0\.1|->127\.0\.0\.1/' | wc -l | tr -d ' ')
    echo "$(( $(date +%s) - start )),$n,$rss,$cpu,$ctime,$conns" >> "$OUT/samples.csv"
  done
  sleep 10
done
wait "${CAP[@]}"
for side in old:$OLD new:$NEW; do n=${side%%:*}; echo "$n final: $(ps -o rss=,time= -p ${side##*:})"; done | tee "$OUT/final.txt"
python3 - "$OUT/samples.csv" <<'PY' | tee "$OUT/summary.txt"
import csv, sys, statistics
rows = list(csv.DictReader(open(sys.argv[1])))
for side in ("old", "new"):
    r = [x for x in rows if x["side"] == side]
    rss = [int(x["rss_kib"]) / 1024 for x in r]
    cpu = [float(x["cpu_pct"]) for x in r]
    conns = sorted({int(x["established_conns"]) for x in r})
    print(f"{side}: samples={len(r)} rss_mib mean={statistics.mean(rss):.1f} max={max(rss):.1f} "
          f"cpu_pct mean={statistics.mean(cpu):.2f} max={max(cpu):.2f} external_established_conns={conns} cpu_time_end={r[-1]['cpu_time']}")
PY
rm -rf /tmp/md120-resource-old /tmp/md120-resource-new
