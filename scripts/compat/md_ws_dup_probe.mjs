#!/usr/bin/env node
// Gateway WS 退订泄漏探针（1.1.1，任务书 §七）：判断 /ws/v1/data 在 订阅→退订→重订 之后是否把同一事件推送多份。
//
//   对照连接 C：一次性订阅 tick.binance.BNBUSDT 与 kline.1m.binance.BNBUSDT，不做任何变更。
//   抖动连接 H：同样订阅，4s 时退订，7s 时重订。
//   统计窗口：从第 12s 起，持续 52s（共 64s）。按消息唯一键计数：
//       tick  : topic|trade_id
//       kline : topic|open_time|volume|number_of_trades|close|exchange_event_ts
//   重复数 = 总消息数 - 唯一键数。
//
// 预期：修复前（1.0.0 / 1.1.0）C 无重复、H 的重复 ≈ 总数的一半；修复后 C 与 H 均为 0。
//
// 用法（Node >= 22）：node scripts/compat/md_ws_dup_probe.mjs [ws-url] [--topics tick.x.y,kline.1m.x.y] [--json out.json]
//   退出码：0 = 两条连接均无重复；1 = 有重复；2 = 参数/连接错误。
import { writeFileSync } from 'node:fs';

const args = process.argv.slice(2);
const url = args.find((a) => a.startsWith('ws://') || a.startsWith('wss://')) ?? 'ws://192.168.1.198:8081/ws/v1/data';
const topicsArg = args.indexOf('--topics');
const TOPICS =
  topicsArg >= 0
    ? args[topicsArg + 1].split(',')
    : ['tick.binance.BNBUSDT', 'kline.1m.binance.BNBUSDT'];
const jsonOut = args.indexOf('--json') >= 0 ? args[args.indexOf('--json') + 1] : null;

const T_UNSUB = 4000;
const T_RESUB = 7000;
const T_COUNT_FROM = 12000;
const T_COUNT_LEN = 52000;

const key = (o) => {
  const d = o.data ?? {};
  if (o.type === 'tick') return `${o.topic}|${d.trade_id}`;
  if (o.type === 'kline')
    return `${o.topic}|${d.open_time}|${d.volume}|${d.number_of_trades}|${d.close}|${d.exchange_event_ts}`;
  return `${o.topic}|${JSON.stringify(d)}`;
};

function makeConn(name) {
  const st = { name, total: 0, byType: {}, keys: new Map(), acks: [], errors: [] };
  const t0 = Date.now();
  const ws = new WebSocket(url);
  ws.onmessage = (ev) => {
    let o;
    try { o = JSON.parse(ev.data); } catch { return; }
    if (o.action) { st.acks.push(`${((Date.now() - t0) / 1000).toFixed(1)}s ${o.action}:${o.stream}`); return; }
    if (o.error) { st.errors.push(o.error); return; }
    const el = Date.now() - t0;
    if (el < T_COUNT_FROM || el > T_COUNT_FROM + T_COUNT_LEN) return;
    st.total++;
    st.byType[o.type] = (st.byType[o.type] ?? 0) + 1;
    const k = key(o);
    st.keys.set(k, (st.keys.get(k) ?? 0) + 1);
  };
  ws.onerror = () => { console.error(`${name}: ws error`); };
  const send = (action) => ws.readyState === WebSocket.OPEN && ws.send(JSON.stringify({ action, streams: TOPICS }));
  return { st, ws, send, t0 };
}

const ctl = makeConn('control');
const chn = makeConn('churn');
await new Promise((r) => {
  let n = 0;
  for (const c of [ctl, chn]) c.ws.onopen = () => { if (++n === 2) r(); };
  setTimeout(() => r(), 5000);
});
if (ctl.ws.readyState !== WebSocket.OPEN || chn.ws.readyState !== WebSocket.OPEN) {
  console.error('连接失败');
  process.exit(2);
}
ctl.send('subscribe');
chn.send('subscribe');
setTimeout(() => chn.send('unsubscribe'), T_UNSUB);
setTimeout(() => chn.send('subscribe'), T_RESUB);

await new Promise((r) => setTimeout(r, T_COUNT_FROM + T_COUNT_LEN + 1500));
ctl.ws.close(); chn.ws.close();

const summarize = (st) => {
  const unique = st.keys.size;
  const dups = st.total - unique;
  const mult = {};
  for (const v of st.keys.values()) mult[v] = (mult[v] ?? 0) + 1;
  return { name: st.name, total: st.total, byType: st.byType, unique, duplicates: dups, dupRatio: st.total ? +(dups / st.total).toFixed(3) : 0, multiplicity: mult, acks: st.acks, errors: st.errors };
};
const res = { url, topics: TOPICS, window: { fromSec: T_COUNT_FROM / 1000, lenSec: T_COUNT_LEN / 1000 }, control: summarize(ctl.st), churn: summarize(chn.st) };
console.log(JSON.stringify(res, null, 2));
if (jsonOut) writeFileSync(jsonOut, JSON.stringify(res, null, 2));
const bad = res.control.duplicates > 0 || res.churn.duplicates > 0;
const empty = res.control.total === 0 || res.churn.total === 0;
if (empty) console.error('WARN: 窗口内没有收到任何数据，结果无效');
console.log(bad ? 'VERDICT: DUPLICATES FOUND' : empty ? 'VERDICT: NO DATA' : 'VERDICT: NO DUPLICATES');
process.exit(bad ? 1 : empty ? 2 : 0);
