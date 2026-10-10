#!/usr/bin/env node
// capture_ws.mjs -- 抓取 md-server WebSocket 推送原文（兼容性证据用）
//
// 用法:
//   node scripts/compat/capture_ws.mjs <mode> <ws-url> <seconds> <outdir> <topic> [topic...]
//     mode = gateway : /ws/v1/data   订阅 {"action":"subscribe","streams":[...]}
//     mode = legacy  : /ws           订阅 {"op":"subscribe","args":[...]}
//
// 输出（outdir 下，文件名中 topic 的 '/' 不会出现）:
//   <topic>.raw.jsonl   该 topic 的推送原文，一行一条，字节原样（不重排、不重序列化）
//   <topic>.ts          与 .raw.jsonl 逐行对应的本机接收时刻（毫秒）
//   _acks.jsonl         订阅确认/错误等控制消息原文
//   _all.raw.jsonl      本连接收到的所有非控制消息原文（用于检查是否混入非预期 type）
//   _summary.json       每个 topic 的条数、相邻间隔统计
//
// 说明: 数据消息归属 topic 靠解析 JSON 中的 "topic" 字段（仅用于分文件，写出的仍是原文）。
//       Node >= 22 自带 WebSocket，无需额外依赖。

import { mkdirSync, appendFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [, , mode, url, secsArg, outdir, ...topics] = process.argv;
if (!mode || !url || !secsArg || !outdir || topics.length === 0) {
  console.error('usage: capture_ws.mjs <gateway|legacy> <ws-url> <seconds> <outdir> <topic>...');
  process.exit(2);
}
const seconds = Number(secsArg);
mkdirSync(outdir, { recursive: true });

const files = {};
const stats = {};
for (const t of topics) {
  files[t] = { raw: join(outdir, `${t}.raw.jsonl`), ts: join(outdir, `${t}.ts`) };
  writeFileSync(files[t].raw, '');
  writeFileSync(files[t].ts, '');
  stats[t] = { count: 0, last: null, gaps: [] };
}
writeFileSync(join(outdir, '_acks.jsonl'), '');
writeFileSync(join(outdir, '_all.raw.jsonl'), '');

const sub = mode === 'gateway'
  ? JSON.stringify({ action: 'subscribe', streams: topics })
  : JSON.stringify({ op: 'subscribe', args: topics });

const ws = new WebSocket(url);
let started = 0;
let other = 0;

function finish(reason) {
  const summary = {
    mode, url, seconds, reason, started_at_ms: started, other_messages: other,
    topics: Object.fromEntries(topics.map((t) => {
      const g = stats[t].gaps;
      return [t, {
        count: stats[t].count,
        max_gap_ms: g.length ? Math.max(...g) : null,
        min_gap_ms: g.length ? Math.min(...g) : null,
        gaps_over_2000ms: g.filter((x) => x > 2000).length,
        gaps_over_10000ms: g.filter((x) => x > 10000).length,
      }];
    })),
  };
  writeFileSync(join(outdir, '_summary.json'), JSON.stringify(summary, null, 2) + '\n');
  console.log(JSON.stringify(summary));
  process.exit(0);
}

ws.onopen = () => {
  started = Date.now();
  ws.send(sub);
  setTimeout(() => { try { ws.close(); } catch {} finish('timer'); }, seconds * 1000);
};
ws.onmessage = (e) => {
  const now = Date.now();
  const text = String(e.data);
  // 控制消息: gateway 的 {"action":...} / {"error":...}；legacy 的 {"event":...}
  if (text.startsWith('{"action"') || text.startsWith('{"error"') || text.startsWith('{"event"')) {
    appendFileSync(join(outdir, '_acks.jsonl'), text + '\n');
    return;
  }
  appendFileSync(join(outdir, '_all.raw.jsonl'), text + '\n');
  let topic = null;
  try { topic = JSON.parse(text).topic; } catch { /* 非 JSON 视为 other */ }
  if (topic && files[topic]) {
    appendFileSync(files[topic].raw, text + '\n');
    appendFileSync(files[topic].ts, now + '\n');
    const s = stats[topic];
    if (s.last !== null) s.gaps.push(now - s.last);
    s.last = now;
    s.count++;
  } else {
    other++;
  }
};
ws.onerror = (e) => { console.error('[ws error]', e.message || e.type); };
ws.onclose = () => { finish('closed'); };
