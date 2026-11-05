#!/usr/bin/env bash
# 最终候选的"同窗口"兼容性对比（NEW_IMG 默认 exp-v4 = A1+A2+A5 默认分配器；推荐候选 exp-v5 = 再加 jemalloc，用 NEW_IMG=md-server:exp-v5 OUT=evidence/1.1.1/compat-final-v5）：
#   old = 真正的 1.1.0 镜像（md-server:1.1.0-quote-bnb，与现网同镜像同配置）  new = md-server:exp-v4
# 两只旁路容器（2 核/256MiB）同时启动、同时抓 240 秒（Gateway + Legacy），再用 compare.mjs 对比。
set -u
cd "$(dirname "$0")/../../.."
OUT="${OUT:-evidence/1.1.1/compat-final}"; NEW_IMG="${NEW_IMG:-md-server:exp-v4}"; HOST=192.168.1.198; SECS="${1:-240}"
mkdir -p $OUT
T_GW="tick.binance.BTCUSDT tick.binance.ETHUSDT tick.binance.BNBUSDT tick.okx.BTC-USDT-SWAP tick.okx.ETH-USDT-SWAP tick.okx.BNB-USDT-SWAP kline.1m.binance.BNBUSDT kline.5m.binance.BTCUSDT kline.15m.binance.BTCUSDT kline.1m.okx.ETH-USDT-SWAP kline.5m.okx.ETH-USDT-SWAP kline.1H.okx.ETH-USDT-SWAP kline.15m.okx.BNB-USDT-SWAP quote.binance.BNBUSDT quote.okx.BNB-USDT-SWAP"
T_LG="tick.binance.BTCUSDT tick.binance.ETHUSDT tick.binance.BNBUSDT tick.okx.BTC-USDT-SWAP tick.okx.ETH-USDT-SWAP tick.okx.BNB-USDT-SWAP kline.1m.binance.BNBUSDT kline.5m.binance.BTCUSDT kline.15m.binance.BTCUSDT kline.1m.okx.ETH-USDT-SWAP kline.5m.okx.ETH-USDT-SWAP kline.1H.okx.ETH-USDT-SWAP kline.15m.okx.BNB-USDT-SWAP"
python3 evidence/1.1.1/tools/pt_exp.py run md-exp-cf-old-110 md-server:1.1.0-quote-bnb 28108 50108 --cpus 2
python3 evidence/1.1.1/tools/pt_exp.py run md-exp-cf-new  "$NEW_IMG"          28109 50109 --cpus 2
sleep 25
# 两侧补入同一份现网订阅集合（39 条），保证待比对的每个主题两侧都有数据（第一次尝试因镜像默认只有 10 条订阅、部分主题两侧都没数据而"空对空"失败，见 compat-final-attempt1-*）
for p in 28108 28109; do node scripts/compat/restore_subs.mjs evidence/1.1.1/d3-load/prod-subscriptions-39.json http://$HOST:$p --apply | tail -1; done
sleep 30
date -u +%FT%TZ > $OUT/captured_at_utc.txt
for side in old:28108 new:28109; do
  n=${side%%:*}; p=${side##*:}
  node scripts/compat/capture_ws.mjs gateway ws://$HOST:$p/ws/v1/data $SECS $OUT/$n-gateway $T_GW > $OUT/$n-gateway.log 2>&1 &
  node scripts/compat/capture_ws.mjs legacy  ws://$HOST:$p/ws         $SECS $OUT/$n-legacy  $T_LG > $OUT/$n-legacy.log 2>&1 &
done
wait
curl -s http://$HOST:28108/api/v1/subscriptions > $OUT/old.subscriptions.json
curl -s http://$HOST:28109/api/v1/subscriptions > $OUT/new.subscriptions.json
curl -s http://$HOST:28108/metrics > $OUT/old.metrics.txt
curl -s http://$HOST:28109/metrics > $OUT/new.metrics.txt
node scripts/compat/compare.mjs selftest > $OUT/compare-selftest.log 2>&1; echo "selftest exit=$?" >> $OUT/compare-selftest.log
node scripts/compat/compare.mjs push $OUT/old-gateway $OUT/new-gateway $T_GW > $OUT/compare-push-gateway.log 2>&1; echo "exit=$?" >> $OUT/compare-push-gateway.log
node scripts/compat/compare.mjs push $OUT/old-legacy  $OUT/new-legacy  $T_LG > $OUT/compare-push-legacy.log 2>&1; echo "exit=$?" >> $OUT/compare-push-legacy.log
node scripts/compat/compare.mjs noquote $OUT/new-legacy > $OUT/compare-noquote-legacy.log 2>&1; echo "exit=$?" >> $OUT/compare-noquote-legacy.log
node scripts/compat/compare.mjs subs $OUT/old.subscriptions.json $OUT/new.subscriptions.json > $OUT/compare-subs.log 2>&1; echo "exit=$?" >> $OUT/compare-subs.log
node scripts/compat/compare.mjs metrics $OUT/old.metrics.txt $OUT/new.metrics.txt > $OUT/compare-metrics.log 2>&1; echo "exit=$?" >> $OUT/compare-metrics.log
python3 evidence/1.1.1/tools/pt_exp.py rm md-exp-cf-old-110
python3 evidence/1.1.1/tools/pt_exp.py rm md-exp-cf-new
echo COMPAT_DONE
