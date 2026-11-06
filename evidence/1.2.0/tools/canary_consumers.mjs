#!/usr/bin/env node
// 开关打开时的金丝雀消费者（任务书 §四 旁路金丝雀）。零依赖，Node >= 22。
//
// 用法：node canary_consumers.mjs <http://host:port> <outdir> [--idle-sec=120] [--botd-hold-sec=90] [--total-sec=360]
//
// 模拟两类消费者（时间相对脚本启动）：
//   OMS 类（连接 A，全程在线）：t=0 订阅 tick.binance.BNBUSDT、tick.okx.BNB-USDT-SWAP（现网运行期订阅）；
//     t=30s 订阅从未登记过的 tick.okx.SOL-USDT-SWAP（应按需补订）；
//     发现 botd 的 SOLUSDT TICK 已被闲置退订后，在同一连接订阅 tick.binance.SOLUSDT（应自动补订、不重复）。
//   botd 类（连接 B）：t=0 POST 登记 TICK/KLINE(1m) binance SOLUSDT 并订阅两个话题；t=botd-hold 取消订阅（连接保持），不 DELETE。
// 每 5 秒采样 GET /api/v1/subscriptions 与 /metrics，写 timeline.jsonl；结束时写 summary.json（各项判据与原始时间点）。
import { mkdirSync, appendFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [, , base, outdir, ...flags] = process.argv;
if (!base || !outdir) {
  console.error('usage: canary_consumers.mjs <http://host:port> <outdir> [--idle-sec=120] [--botd-hold-sec=90] [--total-sec=360]');
  process.exit(2);
}
const opt = (name, dflt) => Number((flags.find((f) => f.startsWith(`--${name}=`)) ?? `=${dflt}`).split('=')[1]);
const IDLE = opt('idle-sec', 120), HOLD = opt('botd-hold-sec', 90), TOTAL = opt('total-sec', 360);
mkdirSync(outdir, { recursive: true });
const T0 = Date.now();
const t = () => Math.round((Date.now() - T0) / 100) / 10;
const log = (msg) => { const line = `${new Date().toISOString()} t=${t()}s ${msg}`; console.log(line); appendFileSync(join(outdir, 'consumers.log'), line + '\n'); };
const wsUrl = base.replace(/^http/, 'ws') + '/ws/v1/data';
const key = (s) => `${s.exchange}|${s.data_type}|${s.symbol}|${s.kline_interval ?? ''}`;

const stats = {}; // topic -> {count, ids:Set, dups}
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
async function post(body) {
  const r = await fetch(`${base}/api/v1/subscriptions`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  log(`POST ${JSON.stringify(body)} -> ${r.status} ${await r.text()}`);
}
const metricNames = ['md_runtime_subscriptions_enabled', 'md_runtime_subscriptions', 'md_runtime_subscriptions_idle_unsubscribed_total',
  'md_runtime_subscriptions_demand_subscribed_total', 'md_runtime_subscriptions_demand_failed_total', 'md_runtime_subscriptions_limit_rejected_total',
  'md_ws_active_clients', 'md_ws_subscriptions_active', 'process_resident_memory_bytes'];
const timeline = [];
async function sample() {
  try {
    const subs = await (await fetch(`${base}/api/v1/subscriptions`)).json();
    const text = await (await fetch(`${base}/metrics`)).text();
    const metrics = Object.fromEntries(metricNames.map((n) => [n, Number((text.match(new RegExp(`^${n} (\\S+)$`, 'm')) ?? [])[1])]));
    const row = { t: t(), at: new Date().toISOString(), subs: subs.map(key).sort(), runtime: subs.filter((s) => s.source === 'runtime').map(key).sort(), metrics };
    timeline.push(row);
    appendFileSync(join(outdir, 'timeline.jsonl'), JSON.stringify({ ...row, raw_subs: subs }) + '\n');
    return row;
  } catch (e) { log(`sample failed: ${e.message}`); return null; }
}
const has = (row, k) => row?.subs.includes(k);
const SOL_TICK = 'binance|1|SOLUSDT|', SOL_KLINE = 'binance|2|SOLUSDT|1m', SNDK = 'binance|1|SNDKUSDT|';
const OMS_KEYS = ['binance|1|BNBUSDT|', 'okx|1|BNB-USDT-SWAP|'];
const OKX_SOL = 'okx|1|SOL-USDT-SWAP|';

const A = conn('oms'), B = conn('botd');
await Promise.all([A.ready, B.ready]);
const initial = await sample();
A.send('subscribe', ['tick.binance.BNBUSDT', 'tick.okx.BNB-USDT-SWAP']);
await post({ exchange: 'binance', type: 'TICK', symbols: ['SOLUSDT'] });
await post({ exchange: 'binance', type: 'KLINE', symbols: ['SOLUSDT'], interval: '1m' });
B.send('subscribe', ['tick.binance.SOLUSDT', 'kline.1m.binance.SOLUSDT']);

const ev = { initial_subs: initial?.subs ?? [] };
let omsResubscribed = false;
while (t() < TOTAL) {
  await new Promise((r) => setTimeout(r, 5000));
  if (ev.okx_sol_subscribed_at === undefined && t() >= 30) { A.send('subscribe', ['tick.okx.SOL-USDT-SWAP']); ev.okx_sol_subscribed_at = t(); }
  if (ev.botd_cancel_at === undefined && t() >= HOLD) { B.send('unsubscribe', ['tick.binance.SOLUSDT', 'kline.1m.binance.SOLUSDT']); ev.botd_cancel_at = t(); }
  const row = await sample();
  if (!row) continue;
  if (ev.sndk_gone_at === undefined && ev.sndk_seen && !has(row, SNDK)) ev.sndk_gone_at = row.t;
  if (has(row, SNDK)) ev.sndk_seen = true;
  if (ev.okx_sol_upstream_at === undefined && has(row, OKX_SOL)) ev.okx_sol_upstream_at = row.t;
  if (ev.botd_cancel_at !== undefined && ev.sol_tick_gone_at === undefined && !has(row, SOL_TICK)) ev.sol_tick_gone_at = row.t;
  if (ev.botd_cancel_at !== undefined && ev.sol_kline_gone_at === undefined && !has(row, SOL_KLINE)) ev.sol_kline_gone_at = row.t;
  if (ev.sol_tick_gone_at !== undefined && !omsResubscribed) {
    A.send('subscribe', ['tick.binance.SOLUSDT']);
    omsResubscribed = true;
    ev.oms_resubscribe_at = t();
  }
  if (omsResubscribed && ev.sol_tick_back_at === undefined && has(row, SOL_TICK)) ev.sol_tick_back_at = row.t;
  for (const k of OMS_KEYS) if (!has(row, k)) (ev.oms_key_missing ??= []).push({ t: row.t, k });
}
const final = timeline.at(-1);
const st = (k) => { const s = stats[k]; return s ? { count: s.count, dups: s.dups, first_at: s.first_at } : { count: 0, dups: 0 }; };
const configKeys = (initial?.subs ?? []).filter((k) => !OMS_KEYS.includes(k) && k !== SNDK);
const summary = {
  base, idle_sec: IDLE, botd_hold_sec: HOLD, total_sec: TOTAL, started_at: new Date(T0).toISOString(), events: ev,
  streams: { oms_bnb_binance: st('oms:tick.binance.BNBUSDT'), oms_bnb_okx: st('oms:tick.okx.BNB-USDT-SWAP'), oms_sol_okx: st('oms:tick.okx.SOL-USDT-SWAP'),
    oms_sol_binance_after_resubscribe: st('oms:tick.binance.SOLUSDT'), botd_sol_tick: st('botd:tick.binance.SOLUSDT'), botd_sol_kline: st('botd:kline.1m.binance.SOLUSDT') },
  final_metrics: final?.metrics,
  checks: {
    oms_topics_never_unsubscribed: !ev.oms_key_missing && OMS_KEYS.every((k) => has(final, k)),
    config_subscriptions_kept: configKeys.every((k) => timeline.every((r) => has(r, k))),
    botd_unsubscribed_after_idle: ev.sol_tick_gone_at !== undefined && ev.sol_kline_gone_at !== undefined
      && ev.sol_tick_gone_at - ev.botd_cancel_at >= IDLE && ev.sol_kline_gone_at - ev.botd_cancel_at >= IDLE,
    restored_runtime_without_receivers_unsubscribed: ev.sndk_seen === true ? ev.sndk_gone_at !== undefined : 'not-in-baseline',
    oms_resubscribe_restored_upstream: ev.sol_tick_back_at !== undefined && st('oms:tick.binance.SOLUSDT').count > 0,
    oms_resubscribe_no_duplicates: st('oms:tick.binance.SOLUSDT').dups === 0 && st('oms:tick.binance.BNBUSDT').dups === 0,
    never_registered_topic_subscribed_on_demand: ev.okx_sol_upstream_at !== undefined && st('oms:tick.okx.SOL-USDT-SWAP').count > 0,
  },
};
summary.pass = Object.values(summary.checks).every((v) => v === true || v === 'not-in-baseline');
writeFileSync(join(outdir, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
log(`SUMMARY pass=${summary.pass} ${JSON.stringify(summary.checks)}`);
A.ws.close(); B.ws.close();
setTimeout(() => process.exit(summary.pass ? 0 : 1), 500);
