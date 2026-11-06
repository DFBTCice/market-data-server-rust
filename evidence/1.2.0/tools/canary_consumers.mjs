#!/usr/bin/env node
// 开关打开时的金丝雀消费者（主审 2026-11-06 复核意见 §四 第 2 部分）。零依赖，Node >= 22。
//
// 用法：node canary_consumers.mjs <http://host:port> <outdir> [--idle-sec=120] [--botd-hold-sec=90] [--total-sec=420]
//   试跑（参数调短）：--idle-sec=120 --botd-hold-sec=90 --total-sec=420
//   现网参数复跑：  --idle-sec=600 --botd-hold-sec=60 --total-sec=900
//
// 三类消费者（时间相对脚本启动；脚本启动前应已用 restore_subs.mjs 灌入现网订阅）：
//   OMS 类（连接 A，全程在线）：t=0 订阅 tick.binance.BNBUSDT、tick.okx.BNB-USDT-SWAP；t=30s 订阅从未登记的 tick.okx.SOL-USDT-SWAP（应按需补订）；
//     发现 1stbotv3 类的 SOLUSDT TICK 已被闲置退订后，在同一连接重订 tick.binance.SOLUSDT（应自动补订、推送不翻倍）。
//   1stbotv3 类（连接 B）：t=0 POST 登记 TICK、KLINE(1m) binance SOLUSDT 并订阅两个话题；t=botd-hold 按 botd 顺序先退 WS 话题，
//     约 50ms 后 DELETE 自己登记的 KLINE（TICK 不 DELETE，与 botd 一致）。KLINE 应立即消失且不被补回；TICK 应在闲置到期后被退、不被补回。
//   报价类（连接 Q）：t=20s 订阅配置名单外的 quote.binance.BTCUSDT、quote.binance.ETHUSDT、quote.okx.BTC-USDT-SWAP、quote.okx.ETH-USDT-SWAP，
//     以及配置名单内的 quote.binance.BNBUSDT、quote.okx.BNB-USDT-SWAP。名单外的不得补订 QUOTE（订阅列表不变、无数据、md_quotes_processed 增速不变），名单内的照常推送。
// 每 5 秒采样 GET /api/v1/subscriptions 与 /metrics，写 timeline.jsonl；结束时写 summary.json。
import { mkdirSync, appendFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [, , base, outdir, ...flags] = process.argv;
if (!base || !outdir) {
  console.error('usage: canary_consumers.mjs <http://host:port> <outdir> [--idle-sec=120] [--botd-hold-sec=90] [--total-sec=420]');
  process.exit(2);
}
const opt = (name, dflt) => Number((flags.find((f) => f.startsWith(`--${name}=`)) ?? `=${dflt}`).split('=')[1]);
const IDLE = opt('idle-sec', 120), HOLD = opt('botd-hold-sec', 90), TOTAL = opt('total-sec', 420);
const QUOTE_AT = 20;
mkdirSync(outdir, { recursive: true });
const T0 = Date.now();
const t = () => Math.round((Date.now() - T0) / 100) / 10;
const log = (msg) => { const line = `${new Date().toISOString()} t=${t()}s ${msg}`; console.log(line); appendFileSync(join(outdir, 'consumers.log'), line + '\n'); };
const wsUrl = base.replace(/^http/, 'ws') + '/ws/v1/data';
const key = (s) => `${s.exchange}|${s.data_type}|${s.symbol}|${s.kline_interval ?? ''}`;

const stats = {}; // name:topic -> {count, ids:Set, dups, first_at}
function conn(name) {
  const ws = new WebSocket(wsUrl);
  const ready = new Promise((r) => (ws.onopen = r));
  ws.onmessage = (e) => {
    const m = JSON.parse(String(e.data));
    if (!m.topic) return;
    const s = (stats[`${name}:${m.topic}`] ??= { count: 0, ids: new Set(), dups: 0, first_at: t() });
    s.count++;
    if (m.type === 'tick') {
      const id = `${m.data.trade_id}`;
      if (s.ids.has(id)) s.dups++;
      s.ids.add(id);
    }
  };
  ws.onclose = () => log(`${name} closed`);
  const send = (action, streams) => { ws.send(JSON.stringify({ action, streams })); log(`${name} ${action} ${streams.join(',')}`); };
  return { ws, ready, send };
}
async function rest(method, body) {
  const r = await fetch(`${base}/api/v1/subscriptions`, { method, headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  log(`${method} ${JSON.stringify(body)} -> ${r.status} ${await r.text()}`);
}
const metricNames = ['md_runtime_subscriptions_enabled', 'md_runtime_subscriptions', 'md_runtime_subscriptions_idle_unsubscribed_total',
  'md_runtime_subscriptions_demand_subscribed_total', 'md_runtime_subscriptions_demand_failed_total', 'md_runtime_subscriptions_limit_rejected_total',
  'md_ws_active_clients', 'md_ws_subscriptions_active', 'md_quotes_processed', 'md_ticks_processed', 'md_ticks_dropped', 'md_quotes_dropped',
  'process_resident_memory_bytes', 'process_cpu_seconds_total'];
const timeline = [];
async function sample() {
  try {
    const subs = await (await fetch(`${base}/api/v1/subscriptions`)).json();
    const text = await (await fetch(`${base}/metrics`)).text();
    const metrics = Object.fromEntries(metricNames.map((n) => [n, Number((text.match(new RegExp(`^${n} (\\S+)$`, 'm')) ?? [])[1])]));
    const row = { t: t(), at: new Date().toISOString(), subs: subs.map(key).sort(),
      sources: Object.fromEntries(subs.map((s) => [key(s), s.source])), metrics };
    timeline.push(row);
    appendFileSync(join(outdir, 'timeline.jsonl'), JSON.stringify({ ...row, raw_subs: subs }) + '\n');
    return row;
  } catch (e) { log(`sample failed: ${e.message}`); return null; }
}
const has = (row, k) => row?.subs.includes(k);
const SOL_TICK = 'binance|1|SOLUSDT|', SOL_KLINE = 'binance|2|SOLUSDT|1m', OKX_SOL = 'okx|1|SOL-USDT-SWAP|';
const OMS_KEYS = ['binance|1|BNBUSDT|', 'okx|1|BNB-USDT-SWAP|'];
const QUOTE_OUTSIDE = ['quote.binance.BTCUSDT', 'quote.binance.ETHUSDT', 'quote.okx.BTC-USDT-SWAP', 'quote.okx.ETH-USDT-SWAP'];
const QUOTE_CONFIG = ['quote.binance.BNBUSDT', 'quote.okx.BNB-USDT-SWAP'];

const A = conn('oms'), B = conn('botd'), Q = conn('quote');
await Promise.all([A.ready, B.ready, Q.ready]);
const initial = await sample();
const initialQuoteKeys = (initial?.subs ?? []).filter((k) => k.split('|')[1] === '3');
A.send('subscribe', OMS_KEYS.map((k) => { const [ex, , sym] = k.split('|'); return `tick.${ex}.${sym}`; }));
await rest('POST', { exchange: 'binance', type: 'TICK', symbols: ['SOLUSDT'] });
await rest('POST', { exchange: 'binance', type: 'KLINE', symbols: ['SOLUSDT'], interval: '1m' });
B.send('subscribe', ['tick.binance.SOLUSDT', 'kline.1m.binance.SOLUSDT']);

const ev = { initial_subs: initial?.subs ?? [] };
let omsResubscribed = false;
while (t() < TOTAL) {
  await new Promise((r) => setTimeout(r, 5000));
  if (ev.quote_subscribed_at === undefined && t() >= QUOTE_AT) { Q.send('subscribe', [...QUOTE_OUTSIDE, ...QUOTE_CONFIG]); ev.quote_subscribed_at = t(); }
  if (ev.okx_sol_subscribed_at === undefined && t() >= 30) { A.send('subscribe', ['tick.okx.SOL-USDT-SWAP']); ev.okx_sol_subscribed_at = t(); }
  if (ev.botd_cancel_at === undefined && t() >= HOLD) {
    B.send('unsubscribe', ['tick.binance.SOLUSDT', 'kline.1m.binance.SOLUSDT']);
    ev.botd_cancel_at = t();
    await new Promise((r) => setTimeout(r, 50));
    await rest('DELETE', { exchange: 'binance', type: 'KLINE', symbols: ['SOLUSDT'], interval: '1m' });
    ev.botd_delete_at = t();
  }
  const row = await sample();
  if (!row) continue;
  for (const k of row.subs.filter((k) => k.split('|')[1] === '3')) {
    if (!initialQuoteKeys.includes(k)) (ev.unexpected_quote_subscriptions ??= []).push({ t: row.t, k });
  }
  if (has(row, 'binance|1|SNDKUSDT|')) ev.sndk_seen = true;
  if (ev.sndk_seen && ev.sndk_gone_at === undefined && !has(row, 'binance|1|SNDKUSDT|')) ev.sndk_gone_at = row.t;
  if (ev.okx_sol_upstream_at === undefined && has(row, OKX_SOL)) ev.okx_sol_upstream_at = row.t;
  if (ev.botd_delete_at !== undefined) {
    if (ev.sol_kline_gone_at === undefined && !has(row, SOL_KLINE)) ev.sol_kline_gone_at = row.t;
    if (ev.sol_kline_gone_at !== undefined && has(row, SOL_KLINE)) (ev.sol_kline_readded ??= []).push(row.t);
    if (ev.sol_tick_gone_at === undefined && !has(row, SOL_TICK)) ev.sol_tick_gone_at = row.t;
    if (ev.sol_tick_gone_at !== undefined && !omsResubscribed && has(row, SOL_TICK)) (ev.sol_tick_readded_before_oms ??= []).push(row.t);
  }
  if (ev.sol_tick_gone_at !== undefined && !omsResubscribed) {
    A.send('subscribe', ['tick.binance.SOLUSDT']);
    omsResubscribed = true;
    ev.oms_resubscribe_at = t();
  }
  if (omsResubscribed && ev.sol_tick_back_at === undefined && has(row, SOL_TICK)) ev.sol_tick_back_at = row.t;
  for (const k of OMS_KEYS) if (!has(row, k)) (ev.oms_key_missing ??= []).push({ t: row.t, k });
}

// md_quotes_processed 增速：报价消费者订阅前 vs 订阅后（每段取首尾样本求平均速率）
const rate = (rows, name) => rows.length < 2 ? null
  : (rows.at(-1).metrics[name] - rows[0].metrics[name]) / (rows.at(-1).t - rows[0].t);
const pre = timeline.filter((r) => r.t <= QUOTE_AT + 0.5);
const post = timeline.filter((r) => r.t >= QUOTE_AT + 5);
const quoteRatePre = rate(pre, 'md_quotes_processed'), quoteRatePost = rate(post, 'md_quotes_processed');
const cpuPct = (rows) => rows.length < 2 ? null
  : 100 * (rows.at(-1).metrics.process_cpu_seconds_total - rows[0].metrics.process_cpu_seconds_total) / (rows.at(-1).t - rows[0].t);
const rssMiB = (r) => r ? Math.round(r.metrics.process_resident_memory_bytes / 1048576 * 10) / 10 : null;
const final = timeline.at(-1);
const st = (k) => { const s = stats[k]; return s ? { count: s.count, dups: s.dups, first_at: s.first_at } : { count: 0, dups: 0 }; };
// 配置订阅以服务端 GET 的 source 字段为准（开关打开时才有）；补入的现网运行期订阅（如 botd 登记的 K 线）本来就会被闲置退订
const configKeys = (initial?.subs ?? []).filter((k) => initial.sources[k] === 'config');
const quoteOutside = Object.fromEntries(QUOTE_OUTSIDE.map((q) => [q, st(`quote:${q}`)]));
const quoteConfig = Object.fromEntries(QUOTE_CONFIG.map((q) => [q, st(`quote:${q}`)]));
const summary = {
  base, idle_sec: IDLE, botd_hold_sec: HOLD, total_sec: TOTAL, started_at: new Date(T0).toISOString(), events: ev,
  streams: {
    oms_bnb_binance: st('oms:tick.binance.BNBUSDT'), oms_bnb_okx: st('oms:tick.okx.BNB-USDT-SWAP'), oms_sol_okx: st('oms:tick.okx.SOL-USDT-SWAP'),
    oms_sol_binance_after_resubscribe: st('oms:tick.binance.SOLUSDT'), botd_sol_tick: st('botd:tick.binance.SOLUSDT'), botd_sol_kline: st('botd:kline.1m.binance.SOLUSDT'),
    quote_outside_config: quoteOutside, quote_in_config: quoteConfig,
  },
  config_keys: configKeys,
  initial_runtime_keys_gone: (initial?.subs ?? []).filter((k) => initial.sources[k] === 'runtime' && !has(final, k)),
  quote_rate_per_sec: { before_quote_consumer: quoteRatePre, after_quote_consumer: quoteRatePost,
    ratio: quoteRatePre ? Math.round(quoteRatePost / quoteRatePre * 100) / 100 : null },
  resources: { rss_mib_start: rssMiB(timeline[0]), rss_mib_end: rssMiB(final), cpu_pct_avg: cpuPct(timeline) },
  final_metrics: final?.metrics,
  checks: {
    oms_topics_never_unsubscribed: !ev.oms_key_missing && OMS_KEYS.every((k) => has(final, k)),
    config_subscriptions_kept: configKeys.length > 0 && configKeys.every((k) => timeline.every((r) => has(r, k))),
    botd_kline_deleted_and_not_readded: ev.sol_kline_gone_at !== undefined && !ev.sol_kline_readded,
    botd_tick_unsubscribed_after_idle: ev.sol_tick_gone_at !== undefined && ev.sol_tick_gone_at - ev.botd_cancel_at >= IDLE,
    botd_tick_not_readded_before_resubscribe: !ev.sol_tick_readded_before_oms,
    restored_runtime_without_receivers_unsubscribed: ev.sndk_seen === true ? ev.sndk_gone_at !== undefined : 'not-in-baseline',
    oms_resubscribe_restored_upstream: ev.sol_tick_back_at !== undefined && st('oms:tick.binance.SOLUSDT').count > 0,
    oms_resubscribe_no_duplicates: st('oms:tick.binance.SOLUSDT').dups === 0 && st('oms:tick.binance.BNBUSDT').dups === 0,
    never_registered_tick_subscribed_on_demand: ev.okx_sol_upstream_at !== undefined && st('oms:tick.okx.SOL-USDT-SWAP').count > 0,
    quote_outside_config_not_subscribed: !ev.unexpected_quote_subscriptions && Object.values(quoteOutside).every((s) => s.count === 0),
    quote_in_config_still_pushed: Object.values(quoteConfig).every((s) => s.count > 0),
    quote_rate_not_jumped: quoteRatePre > 0 && quoteRatePost / quoteRatePre < 2,
  },
};
summary.pass = Object.values(summary.checks).every((v) => v === true || v === 'not-in-baseline');
writeFileSync(join(outdir, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
log(`SUMMARY pass=${summary.pass} ${JSON.stringify(summary.checks)}`);
A.ws.close(); B.ws.close(); Q.ws.close();
setTimeout(() => process.exit(summary.pass ? 0 : 1), 500);
