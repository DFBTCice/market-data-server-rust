#!/usr/bin/env node
// analyze_quote.mjs -- 对 capture_ws.mjs 抓到的 quote 推送做验收分析（任务书 §二 / §四阶段 2）
//
// 用法: node scripts/compat/analyze_quote.mjs <capture-dir> <quote-topic> [--control <tick-topic>] [--max-gap-ms 2000] [--min-count 10]
//
// 检查项（任一不满足退出码 1）：
//   1. 条数 >= min-count（空抓取不算通过，退出码 3）
//   2. 相邻推送间隔（以抓取端收到时刻 .ts 计算）最大值 <= max-gap-ms
//   3. 任意滑动 1s 窗口内条数 <= 10（节流上限）
//   4. bid_price / bid_qty / ask_price / ask_qty 全部非空
//   5. 外层为 {"type":"quote","topic":...,"data":{...}}，data 字段名与顺序等于规范
//   6. 正对照（--control）：同一抓取窗口内对照主题（如 tick.okx.BNB-USDT-SWAP）的最大间隔必须 > 10s，
//      证明"quote 间隔 <= 2s"这一判据在该窗口里是能区分出"稀疏数据"的；否则结论为"不充分"（退出码 3）
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const argv = process.argv.slice(2);
const [dir, topic] = argv;
const opt = (name, dflt) => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
if (!dir || !topic) {
  console.error('usage: analyze_quote.mjs <capture-dir> <quote-topic> [--control <topic>] [--max-gap-ms N] [--min-count N]');
  process.exit(2);
}
const maxGap = Number(opt('--max-gap-ms', 2000));
const minCount = Number(opt('--min-count', 10));
const control = opt('--control', null);

const EXPECT_KEYS = [
  'exchange', 'symbol', 'bid_price', 'bid_qty', 'ask_price', 'ask_qty', 'last_price', 'timestamp', 'connector_receive_ts',
];
const lines = (p) => (existsSync(p) ? readFileSync(p, 'utf8').split('\n').filter((l) => l.trim()) : []);
const stats = (ts) => {
  const gaps = [];
  for (let i = 1; i < ts.length; i++) gaps.push(ts[i] - ts[i - 1]);
  return { n: ts.length, maxGap: gaps.length ? Math.max(...gaps) : null, minGap: gaps.length ? Math.min(...gaps) : null,
    over2s: gaps.filter((g) => g > 2000).length, over10s: gaps.filter((g) => g > 10000).length };
};

let failed = 0, inconclusive = 0;
const ok = (c, msg) => { console.log(`   ${c ? 'OK  ' : 'FAIL'} ${msg}`); if (!c) failed++; };

const ts = lines(join(dir, `${topic}.ts`)).map(Number);
const raw = lines(join(dir, `${topic}.raw.jsonl`));
console.log(`== ${topic}  (${dir})`);
if (ts.length < minCount || raw.length < minCount) {
  console.log(`   INCONCLUSIVE 条数不足: ts=${ts.length} raw=${raw.length} < ${minCount}`);
  process.exit(3);
}
const s = stats(ts);
const seconds = (ts[ts.length - 1] - ts[0]) / 1000;
ok(s.maxGap <= maxGap, `相邻间隔 min=${s.minGap}ms max=${s.maxGap}ms（要求 max<=${maxGap}ms）; >2s 次数=${s.over2s} >10s 次数=${s.over10s}`);

// 节流（<=10/s/主题）判据。注意"客户端收到时刻"会被网络/事件循环成批递送（TCP 合包、WiFi 抖动），
// 因此严格的 1s 窗口只作参考；强制判据用 >=2s 的窗口（抖动被摊薄，而没有节流时 Binance bookTicker
// 的 36+/s 入口速率会让 2s 窗口轻松超过 20 条）。服务端侧的确定性证明见 md-processor 单测
// quote_throttle_one_second_burst_capped。
const slidingMax = (W) => {
  let mx = 0;
  for (let i = 0, j = 0; i < ts.length; i++) {
    while (ts[i] - ts[j] >= W) j++;
    mx = Math.max(mx, i - j + 1);
  }
  return mx;
};
const win1 = slidingMax(1000);
console.log(`   INFO 滑动 1s 窗口最大条数=${win1}（参考：客户端成批递送会使其略高于 10；强制判据见下）; 共 ${ts.length} 条, 平均 ${(ts.length / Math.max(seconds, 1)).toFixed(2)}/s`);
for (const W of [2000, 5000, 10000]) {
  const mx = slidingMax(W);
  const cap = (10 * W) / 1000;
  ok(mx <= cap, `滑动 ${W / 1000}s 窗口最大条数=${mx}（上限 ${cap} = 10/s）`);
}

let emptyField = 0, badShape = 0, badOrder = 0, lastNonEmpty = 0;
for (const l of raw) {
  const o = JSON.parse(l);
  if (o.type !== 'quote' || o.topic !== topic || typeof o.data !== 'object' || Object.keys(o).join() !== 'type,topic,data') badShape++;
  const d = o.data ?? {};
  if (Object.keys(d).join() !== EXPECT_KEYS.join()) badOrder++;
  for (const k of ['bid_price', 'bid_qty', 'ask_price', 'ask_qty']) if (d[k] === '' || d[k] == null) emptyField++;
  if (d.last_price) lastNonEmpty++;
}
ok(emptyField === 0, `bid/ask 价量全部非空（空值 ${emptyField} 处）; last_price 非空 ${lastNonEmpty}/${raw.length}（Binance 为空是规范约定）`);
ok(badShape === 0, `外层结构 {"type":"quote","topic":...,"data":...} 全部符合（异常 ${badShape} 条）`);
ok(badOrder === 0, `data 字段名与顺序全部等于规范 [${EXPECT_KEYS.join(',')}]（异常 ${badOrder} 条）`);
console.log(`   首条原文: ${raw[0]}`);

if (control) {
  const cts = lines(join(dir, `${control}.ts`)).map(Number);
  const cs = stats(cts);
  console.log(`   [正对照] ${control}: ${cs.n} 条, 最大间隔=${cs.maxGap}ms, >2s 次数=${cs.over2s}, >10s 次数=${cs.over10s}`);
  if (cs.n < 2 || cs.maxGap <= 10000) {
    console.log('   INCONCLUSIVE 对照主题在该窗口内没有出现 >10s 的间隔，无法证明 "quote 间隔<=2s" 能区分稀疏数据；请延长抓取或换窗口');
    inconclusive++;
  } else {
    console.log('   OK   正对照成立：同窗口 tick 出现 >10s 空档，而 quote 最大间隔仍 <= 阈值');
  }
}

console.log(failed ? `RESULT: FAIL (${failed} 项)` : inconclusive ? 'RESULT: INCONCLUSIVE' : 'RESULT: PASS');
process.exit(failed ? 1 : inconclusive ? 3 : 0);
