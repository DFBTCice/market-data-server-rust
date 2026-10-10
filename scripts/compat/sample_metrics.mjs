#!/usr/bin/env node
// sample_metrics.mjs -- 只读采样 md-server 的 /metrics 与 /api/v1/subscriptions，写 CSV + 事件日志。
// 用于「RSS 台阶与事件对齐」（任务书 1.1.1 §三 D3/D4）：每个采样点一行，另把"订阅集合变化 / 重连计数增长 /
// 连接状态翻转 / ticks_dropped 增长 / WS 客户端数变化"作为事件写入 <out>.events.log。
//
// 用法: node scripts/compat/sample_metrics.mjs <base-url> <总秒数> <间隔秒> <out.csv> [--snapshot-every-min 60]
// 只发 GET，不修改被采样实例；采样失败的行用空值占位，不中断。
import { appendFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';

const [base, totalS, everyS, out] = process.argv.slice(2);
if (!base || !totalS || !everyS || !out) {
  console.error('usage: sample_metrics.mjs <base-url> <seconds> <interval> <out.csv> [--snapshot-every-min N]');
  process.exit(2);
}
const snapIdx = process.argv.indexOf('--snapshot-every-min');
const snapEveryMin = snapIdx > 0 ? Number(process.argv[snapIdx + 1]) : 0;
mkdirSync(dirname(out), { recursive: true });
const evPath = out.replace(/\.csv$/, '') + '.events.log';

/** 解析 Prometheus 文本：返回 Map<"name{labels}", number> */
function parseMetrics(text) {
  const m = new Map();
  for (const line of text.split('\n')) {
    if (!line || line[0] === '#') continue;
    const sp = line.lastIndexOf(' ');
    if (sp < 0) continue;
    const v = Number(line.slice(sp + 1));
    if (Number.isFinite(v)) m.set(line.slice(0, sp), v);
  }
  return m;
}
const g = (m, key) => (m.has(key) ? m.get(key) : '');
const sumPrefix = (m, prefix) => {
  let s = 0, any = false;
  for (const [k, v] of m) if (k.startsWith(prefix)) { s += v; any = true; }
  return any ? s : '';
};

const COLS = [
  'ts_utc', 'rss_bytes', 'vsz_bytes', 'cpu_seconds', 'open_fds', 'uptime_s', 'ws_clients', 'ws_subscriptions_active',
  'subs_total', 'subs_tick', 'subs_kline', 'subs_quote',
  'ticks_processed', 'ticks_dropped', 'klines_processed', 'klines_dropped', 'quotes_processed', 'quotes_dropped',
  'reconnect_okx', 'reconnect_okx_kline', 'reconnect_okx_quote', 'reconnect_binance', 'reconnect_binance_quote',
  'connected_sum', 'ws_sent_tick', 'ws_sent_kline', 'ws_sent_quote', 'metrics_lines',
  'tokio_alive_tasks', 'jemalloc_allocated', 'jemalloc_active', 'jemalloc_resident', 'jemalloc_retained', 'jemalloc_mapped',
];
writeFileSync(out, COLS.join(',') + '\n');

const subKey = (s) => `${s.exchange}|${s.data_type}|${s.symbol}|${s.kline_interval ?? ''}`;
let prevSubs = null, prevM = null;
const ev = (msg) => appendFileSync(evPath, `${new Date().toISOString()} ${msg}\n`);

const end = Date.now() + Number(totalS) * 1000;
let n = 0;
while (true) {
  const t0 = Date.now();
  let m = null, subs = null, text = '';
  try { text = await (await fetch(`${base}/metrics`, { signal: AbortSignal.timeout(8000) })).text(); m = parseMetrics(text); } catch (e) { ev(`WARN metrics fetch failed: ${e.message}`); }
  try { subs = await (await fetch(`${base}/api/v1/subscriptions`, { signal: AbortSignal.timeout(8000) })).json(); } catch (e) { ev(`WARN subscriptions fetch failed: ${e.message}`); }

  const cnt = (dt) => (subs ? subs.filter((s) => s.data_type === dt).length : '');
  const row = {
    ts_utc: new Date().toISOString().replace(/\.\d+Z$/, 'Z'),
    rss_bytes: m ? g(m, 'process_resident_memory_bytes') : '',
    vsz_bytes: m ? g(m, 'process_virtual_memory_bytes') : '',
    cpu_seconds: m ? g(m, 'process_cpu_seconds_total') : '',
    open_fds: m ? g(m, 'process_open_fds') : '',
    uptime_s: m ? g(m, 'process_uptime_seconds') : '',
    ws_clients: m ? g(m, 'md_ws_active_clients') : '',
    ws_subscriptions_active: m ? g(m, 'md_ws_subscriptions_active') : '',
    subs_total: subs ? subs.length : '', subs_tick: cnt(1), subs_kline: cnt(2), subs_quote: cnt(3),
    ticks_processed: m ? g(m, 'md_ticks_processed') : '', ticks_dropped: m ? g(m, 'md_ticks_dropped') : '',
    klines_processed: m ? g(m, 'md_klines_processed') : '', klines_dropped: m ? g(m, 'md_klines_dropped') : '',
    quotes_processed: m ? g(m, 'md_quotes_processed') : '', quotes_dropped: m ? g(m, 'md_quotes_dropped') : '',
    reconnect_okx: m ? g(m, 'md_connector_reconnect_total{exchange="okx"}') : '',
    reconnect_okx_kline: m ? g(m, 'md_connector_reconnect_total{exchange="okx-kline"}') : '',
    reconnect_okx_quote: m ? g(m, 'md_connector_reconnect_total{exchange="okx-quote"}') : '',
    reconnect_binance: m ? g(m, 'md_connector_reconnect_total{exchange="binance"}') : '',
    reconnect_binance_quote: m ? g(m, 'md_connector_reconnect_total{exchange="binance-quote"}') : '',
    connected_sum: m ? sumPrefix(m, 'md_connector_connected{') : '',
    ws_sent_tick: m ? g(m, 'md_ws_messages_sent_total{kind="tick"}') : '',
    ws_sent_kline: m ? g(m, 'md_ws_messages_sent_total{kind="kline"}') : '',
    ws_sent_quote: m ? g(m, 'md_ws_messages_sent_total{kind="quote"}') : '',
    metrics_lines: text ? text.split('\n').length : '',
    tokio_alive_tasks: m ? g(m, 'md_tokio_alive_tasks') : '',
    jemalloc_allocated: m ? g(m, 'md_jemalloc_allocated_bytes') : '',
    jemalloc_active: m ? g(m, 'md_jemalloc_active_bytes') : '',
    jemalloc_resident: m ? g(m, 'md_jemalloc_resident_bytes') : '',
    jemalloc_retained: m ? g(m, 'md_jemalloc_retained_bytes') : '',
    jemalloc_mapped: m ? g(m, 'md_jemalloc_mapped_bytes') : '',
  };
  appendFileSync(out, COLS.map((c) => row[c]).join(',') + '\n');

  // ---- 事件 ----
  if (subs) {
    const cur = new Map(subs.map((s) => [subKey(s), s]));
    if (prevSubs) {
      for (const [k] of cur) if (!prevSubs.has(k)) ev(`SUB+ ${k}`);
      for (const [k] of prevSubs) if (!cur.has(k)) ev(`SUB- ${k}`);
    }
    prevSubs = cur;
  }
  if (m && prevM) {
    for (const [k, v] of m) {
      if (k.startsWith('md_connector_reconnect_total{') && v > (prevM.get(k) ?? 0)) ev(`RECONNECT ${k} ${prevM.get(k) ?? 0}->${v}`);
      if (k.startsWith('md_connector_connected{') && v !== prevM.get(k)) ev(`CONNECTED ${k} ${prevM.get(k)}->${v}`);
      if (k === 'md_ticks_dropped' && v > (prevM.get(k) ?? 0)) ev(`TICKS_DROPPED ${prevM.get(k)}->${v}`);
      if (k === 'md_klines_dropped' && v > (prevM.get(k) ?? 0)) ev(`KLINES_DROPPED ${prevM.get(k)}->${v}`);
      if (k === 'md_ws_active_clients' && v !== prevM.get(k)) ev(`WS_CLIENTS ${prevM.get(k)}->${v}`);
    }
  }
  if (m) prevM = m;
  if (snapEveryMin > 0 && n % Math.max(1, Math.round((snapEveryMin * 60) / Number(everyS))) === 0 && text) {
    writeFileSync(join(dirname(out), `snapshot-${row.ts_utc.replace(/[:]/g, '')}.metrics.txt`), text);
  }
  n++;
  if (Date.now() >= end) break;
  const wait = Number(everyS) * 1000 - (Date.now() - t0);
  if (wait > 0) await new Promise((r) => setTimeout(r, wait));
}
