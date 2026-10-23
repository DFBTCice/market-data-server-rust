#!/usr/bin/env node
// 现网等价负载生成器（1.1.1 内存增长调查 D3）：对一个 md-server 实例模拟两类 Gateway WS 消费者。
//
//   消费者 ①（"churn"，模拟 1stbotv3）：一条长连接，常驻订阅一部分 kline，
//        并每隔 --churn-period 秒对轮换的 --churn-n 个 kline 做 退订 → 等 --churn-off 秒 → 重订。
//   消费者 ②（"static"，模拟 OMS 价格源）：一条长连接，订阅全部 tick.*（以及可选 quote.*），永不变更。
//
// 所有收到的消息只计数、不落盘；每 60s 打印一行统计（消息数/重复数/重连数）。
// 若连接被服务端踢出（lagged）或断开，3 秒后自动重连并按当前应有订阅重新订阅（与真实客户端行为一致）。
//
// 用法（Node >= 22，使用内置 WebSocket）：
//   node scripts/compat/load_consumers.mjs <ws-url> <subs.json> <seconds> <log-file> \
//        [--churn-period 90] [--churn-n 4] [--churn-off 20] [--static-klines 15] [--no-quote] [--seed 1]
//
// <subs.json> 是 GET /api/v1/subscriptions 的输出（evidence/1.1.1/d3-load/prod-subscriptions-39.json）。
import { readFileSync, appendFileSync, writeFileSync } from 'node:fs';

const [, , url, subsFile, secsArg, logFile, ...rest] = process.argv;
if (!url || !subsFile || !secsArg || !logFile) {
  console.error('usage: load_consumers.mjs <ws-url> <subs.json> <seconds> <log-file> [options]');
  process.exit(2);
}
const opt = (name, def) => {
  const i = rest.indexOf(name);
  return i >= 0 ? Number(rest[i + 1]) : def;
};
const flag = (name) => rest.includes(name);
const CHURN_PERIOD = opt('--churn-period', 90) * 1000;
const CHURN_N = opt('--churn-n', 4);
const CHURN_OFF = opt('--churn-off', 20) * 1000;
const STATIC_KLINES = opt('--static-klines', 15);
const DURATION = Number(secsArg) * 1000;
const WITH_QUOTE = !flag('--no-quote');

const subs = JSON.parse(readFileSync(subsFile, 'utf8'));
const topicOf = (s) =>
  s.data_type === 1
    ? `tick.${s.exchange}.${s.symbol}`
    : s.data_type === 2
      ? `kline.${s.kline_interval}.${s.exchange}.${s.symbol}`
      : `quote.${s.exchange}.${s.symbol}`;
const ticks = subs.filter((s) => s.data_type === 1).map(topicOf);
const quotes = subs.filter((s) => s.data_type === 3).map(topicOf);
const klines = subs.filter((s) => s.data_type === 2).map(topicOf);
const staticKlines = klines.slice(0, STATIC_KLINES);
const churnPool = klines.slice(STATIC_KLINES);

const log = (m) => {
  const line = `${new Date().toISOString()} ${m}`;
  appendFileSync(logFile, line + '\n');
  console.log(line);
};
writeFileSync(logFile, '');
log(
  `start url=${url} secs=${secsArg} ticks=${ticks.length} quotes=${quotes.length} klines=${klines.length} ` +
    `staticKlines=${staticKlines.length} churnPool=${churnPool.length} churn=${CHURN_N}/${CHURN_PERIOD / 1000}s off=${CHURN_OFF / 1000}s`,
);

const stats = { static: mk(), churn: mk() };
function mk() {
  return { msgs: 0, dups: 0, reconnects: 0, acks: 0, errors: 0, seen: new Map() };
}

// 消息去重判据：tick 用 trade_id，kline 用 (open_time|volume|number_of_trades|close|exchange_event_ts)
function msgKey(o) {
  const d = o.data ?? {};
  if (o.type === 'tick') return `${o.topic}|${d.trade_id}`;
  if (o.type === 'kline')
    return `${o.topic}|${d.open_time}|${d.volume}|${d.number_of_trades}|${d.close}|${d.exchange_event_ts}`;
  return `${o.topic}|${d.timestamp}|${d.bid_price}|${d.ask_price}`;
}

class Consumer {
  constructor(name, wanted) {
    this.name = name;
    this.wanted = new Set(wanted); // 当前"应当订阅"的集合
    this.ws = null;
    this.st = stats[name];
    this.stopped = false;
  }
  connect() {
    if (this.stopped) return;
    const ws = new WebSocket(url);
    this.ws = ws;
    ws.onopen = () => {
      this.send('subscribe', [...this.wanted]);
    };
    ws.onmessage = (ev) => {
      let o;
      try {
        o = JSON.parse(ev.data);
      } catch {
        return;
      }
      if (o.action) {
        this.st.acks++;
        return;
      }
      if (o.error) {
        this.st.errors++;
        return;
      }
      this.st.msgs++;
      // 重复检测（只对短窗口内的 key 做记忆，防止 Map 无限增长）
      const k = msgKey(o);
      if (this.st.seen.has(k)) this.st.dups++;
      this.st.seen.set(k, 1);
      if (this.st.seen.size > 20000) {
        const it = this.st.seen.keys();
        for (let i = 0; i < 10000; i++) this.st.seen.delete(it.next().value);
      }
    };
    ws.onclose = () => {
      if (this.stopped) return;
      this.st.reconnects++;
      log(`${this.name}: connection closed, reconnect in 3s (wanted=${this.wanted.size})`);
      setTimeout(() => this.connect(), 3000);
    };
    ws.onerror = () => {};
  }
  send(action, streams) {
    if (!streams.length) return;
    if (this.ws?.readyState === WebSocket.OPEN)
      this.ws.send(JSON.stringify({ action, streams }));
  }
  subscribe(streams) {
    streams.forEach((s) => this.wanted.add(s));
    this.send('subscribe', streams);
  }
  unsubscribe(streams) {
    streams.forEach((s) => this.wanted.delete(s));
    this.send('unsubscribe', streams);
  }
  stop() {
    this.stopped = true;
    try {
      this.ws?.close();
    } catch {}
  }
}

const staticC = new Consumer('static', [...ticks, ...(WITH_QUOTE ? quotes : [])]);
const churnC = new Consumer('churn', [...staticKlines, ...churnPool]);
staticC.connect();
churnC.connect();

// churn 循环：每个周期轮换 CHURN_N 个 topic：退订 -> 等 CHURN_OFF -> 重订
let cursor = 0;
let cycles = 0;
const churnTimer = setInterval(() => {
  if (!churnPool.length) return;
  const batch = [];
  for (let i = 0; i < CHURN_N; i++) batch.push(churnPool[(cursor + i) % churnPool.length]);
  cursor = (cursor + CHURN_N) % churnPool.length;
  // 初始已订阅全部 K 线，轮换执行退订 -> 等待 -> 重订。
  churnC.unsubscribe(batch);
  log(`CHURN unsubscribe ${batch.join(',')}`);
  cycles++;
  setTimeout(() => {
    churnC.subscribe(batch);
    log(`CHURN resubscribe ${batch.join(',')}`);
  }, CHURN_OFF);
}, CHURN_PERIOD);

const statTimer = setInterval(() => {
  for (const n of ['static', 'churn']) {
    const s = stats[n];
    log(
      `${n}: msgs=${s.msgs} dups=${s.dups} acks=${s.acks} errors=${s.errors} reconnects=${s.reconnects} ` +
        (n === 'churn' ? `churn_cycles=${cycles} wanted=${churnC.wanted.size}` : `wanted=${staticC.wanted.size}`),
    );
  }
}, 60000);

setTimeout(() => {
  clearInterval(churnTimer);
  clearInterval(statTimer);
  staticC.stop();
  churnC.stop();
  for (const n of ['static', 'churn']) {
    const s = stats[n];
    log(`FINAL ${n}: msgs=${s.msgs} dups=${s.dups} acks=${s.acks} errors=${s.errors} reconnects=${s.reconnects}`);
  }
  setTimeout(() => process.exit(0), 500);
}, DURATION);
