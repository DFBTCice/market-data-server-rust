// 旁路追加取证，不重启原两小时负载：区分粗键碰撞与同一入站事件重复投递。
// node evidence/1.1.1/tools/anomaly_probe.mjs <subs.json> <out-dir> [seconds=330]
import { readFileSync, mkdirSync, writeFileSync, createWriteStream } from 'node:fs';
import { join } from 'node:path';
import { once } from 'node:events';

const [subsFile, out, seconds = '330'] = process.argv.slice(2);
if (!subsFile || !out) throw new Error('subs.json out-dir [seconds] required');
mkdirSync(out, { recursive: true });
const subs = JSON.parse(readFileSync(subsFile, 'utf8'));
const topic = (s) => s.data_type === 1 ? `tick.${s.exchange}.${s.symbol}` :
  s.data_type === 2 ? `kline.${s.kline_interval}.${s.exchange}.${s.symbol}` : `quote.${s.exchange}.${s.symbol}`;
const topics = subs.map(topic);
const churnTopics = subs.filter(s => s.data_type === 2).map(topic);
// 与正在运行的 load_consumers.mjs 一致；保留其口径，不将旧计数事后“修成零”。
const coarseKey = (o) => {
  const d = o.data ?? {};
  if (o.type === 'tick') return `${o.topic}|${d.trade_id}`;
  if (o.type === 'kline') return `${o.topic}|${d.open_time}|${d.volume}|${d.number_of_trades}|${d.close}|${d.exchange_event_ts}`;
  return `${o.topic}|${d.timestamp}|${d.bid_price}|${d.ask_price}`;
};
function classify(a, b) {
  const x = JSON.parse(a).data, y = JSON.parse(b).data;
  const fields = [...new Set([...Object.keys(x), ...Object.keys(y)])].filter(k => x[k] !== y[k]).sort();
  return { fields, kind: a === b ? 'same_ingest_repeat' :
    fields.every(k => k === 'connector_receive_ts') ? 'same_payload_new_ingest' : 'different_payload_same_coarse_key' };
}

// 正对照：同一字节重复必须被标为 same_ingest_repeat；改 is_closed/数量必须被标为碰撞。
const fixture = { type: 'kline', topic: 'kline.5m.okx.X', data: { open_time: 1, volume: '1', number_of_trades: 0, close: '2', exchange_event_ts: 1, connector_receive_ts: 3 } };
const closed = { ...fixture, data: { ...fixture.data, is_closed: true, connector_receive_ts: 4 } };
if (coarseKey(fixture) !== coarseKey(closed) || classify(JSON.stringify(fixture), JSON.stringify(closed)).kind !== 'different_payload_same_coarse_key' ||
    classify(JSON.stringify(fixture), JSON.stringify(fixture)).kind !== 'same_ingest_repeat') throw new Error('classifier positive control failed');
const events = createWriteStream(join(out, 'events.jsonl'));
const pairs = createWriteStream(join(out, 'duplicate-pairs.jsonl'));
const emit = (o) => events.write(JSON.stringify({ at: new Date().toISOString(), ...o }) + '\n');
emit({ event: 'classifier_selftest', pass: true });
const clients = [];
let maxLoopDelayMs = 0;
let prevTick = performance.now();
const lagTimer = setInterval(() => {
  const now = performance.now(), delay = now - prevTick - 100;
  prevTick = now;
  maxLoopDelayMs = Math.max(maxLoopDelayMs, delay);
  if (delay > 500) emit({ event: 'event_loop_delay', delay_ms: delay });
}, 100);
for (const [side, port] of [['old', 28108], ['new', 28109]]) {
  for (const mode of ['control', 'churn']) {
    const st = { side, mode, msgs: 0, coarse_duplicates: 0, same_ingest_repeat: 0, same_payload_new_ingest: 0,
      different_payload_same_coarse_key: 0, fields: {}, by_topic: {}, acks: 0, errors: [], closed: null, seen: new Map(), lastMsg: null };
    const ws = new WebSocket(`ws://192.168.1.198:${port}/ws/v1/data`);
    const c = { st, ws, opened: false };
    clients.push(c);
    ws.onopen = () => { c.opened = true; ws.send(JSON.stringify({ action: 'subscribe', streams: topics })); emit({ side, mode, event: 'open' }); };
    ws.onmessage = (event) => {
      let o;
      try { o = JSON.parse(event.data); } catch { return; }
      if (o.action) { st.acks++; return; }
      if (o.error) { st.errors.push(o.error); return; }
      const now = new Date().toISOString();
      if (st.lastMsg && Date.parse(now) - Date.parse(st.lastMsg) > 2000) emit({ side, mode, event: 'receive_gap', from: st.lastMsg, to: now });
      st.lastMsg = now;
      st.msgs++;
      const k = coarseKey(o), old = st.seen.get(k);
      if (old) {
        const classification = classify(old.raw, event.data);
        st.coarse_duplicates++; st[classification.kind]++;
        const f = classification.fields.join(',') || 'identical_bytes';
        st.fields[f] = (st.fields[f] ?? 0) + 1;
        st.by_topic[o.topic] ??= { same_ingest_repeat: 0, same_payload_new_ingest: 0, different_payload_same_coarse_key: 0 };
        st.by_topic[o.topic][classification.kind]++;
        if (st.coarse_duplicates <= 3000) pairs.write(JSON.stringify({ side, mode, key: k, before_at: old.at, after_at: now, ...classification, before: old.raw, after: event.data }) + '\n');
      }
      st.seen.set(k, { at: now, raw: event.data });
      if (st.seen.size > 20000) { const keys = st.seen.keys(); for (let i=0;i<10000;i++) st.seen.delete(keys.next().value); }
    };
    ws.onclose = (e) => { st.closed = { at: new Date().toISOString(), code: e.code, reason: e.reason, wasClean: e.wasClean }; emit({ side, mode, event: 'close', ...st.closed, last_msg: st.lastMsg }); };
    ws.onerror = () => { st.errors.push('websocket error'); };
  }
}
let round = 0;
const churnTimer = setInterval(() => {
  const batch = [churnTopics[round++ % churnTopics.length]];
  for (const c of clients.filter(c => c.st.mode === 'churn' && c.ws.readyState === WebSocket.OPEN)) c.ws.send(JSON.stringify({ action: 'unsubscribe', streams: batch }));
  emit({ event: 'unsubscribe', streams: batch });
  setTimeout(() => {
    for (const c of clients.filter(c => c.st.mode === 'churn' && c.ws.readyState === WebSocket.OPEN)) c.ws.send(JSON.stringify({ action: 'subscribe', streams: batch }));
    emit({ event: 'resubscribe', streams: batch });
  }, 3000);
}, 20000);
await new Promise(r => setTimeout(r, Number(seconds)*1000));
clearInterval(churnTimer); clearInterval(lagTimer);
const summary = { finished_at: new Date().toISOString(), seconds: Number(seconds), max_event_loop_delay_ms: maxLoopDelayMs,
  clients: clients.map(({st}) => { const {seen, ...rest}=st; return rest; }) };
for (const c of clients) c.ws.close();
writeFileSync(join(out, 'summary.json'), JSON.stringify(summary, null, 2)+'\n');
events.end(); pairs.end();
await Promise.all([once(events, 'finish'), once(pairs, 'finish')]);
console.log(JSON.stringify(summary, null, 2));
if (summary.clients.some(s => !s.msgs || s.errors.length || s.closed)) process.exitCode=1;
