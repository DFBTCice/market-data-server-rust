#!/usr/bin/env node
// compare.mjs -- 新旧版本兼容性对比工具（任务书 §三 的证据生成器）
//
// 子命令:
//   push     <old_dir> <new_dir> <topic>...   比较 capture_ws.mjs 抓到的两份推送原文
//   noquote  <dir>                            断言抓取结果中没有任何 quote 消息（旧 /ws 用）
//   subs     <old.json> <new.json>            比较 GET /api/v1/subscriptions 响应
//   metrics  <old.txt> <new.txt>              比较 /metrics：只允许新增，不允许删除/改类型
//   selftest                                  正对照：用已知不同的样本证明本工具真的能发现差异
//
// 退出码: 0 = 全部通过；1 = 发现差异；2 = 用法错误；3 = 结论不充分（例如没有可比对的重叠记录）
//
// "0 差异" 只有在 selftest 通过后才有意义：selftest 会构造 字段重排 / 数值改动 / 多字段 /
// 丢记录 / 混入 quote / metrics 删行 / metrics 改类型 等已知不同样本，并断言本工具报告了差异。

import { readFileSync, existsSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';

// ------------------------------------------------------------------ 公共
const readLines = (p) => (existsSync(p) ? readFileSync(p, 'utf8').split('\n').filter((l) => l.trim() !== '') : []);

/**
 * 去掉每个连接各自产生的时间戳（文本级删除，不重序列化，保持其余字节原样）：
 *  - connector_receive_ts：本机接收时刻，任务书明确允许不同；
 *  - kline 的 exchange_event_ts：交易所按"每条连接"各自推送的事件时刻，两条独立连接天然不同
 *    （tick 的 exchange_event_ts 取自成交本身，两侧相同，因此 tick 不去掉，照样逐字节比较）。
 */
export function normalize(line, topic = '') {
  let out = line.replace(/,"connector_receive_ts":-?\d+/, '');
  if (topic.startsWith('kline.')) out = out.replace(/,"exchange_event_ts":-?\d+/, '');
  return out;
}

/** 一条推送的 data 内 key 顺序 */
function dataKeys(line) {
  const o = JSON.parse(line);
  return Object.keys(o.data ?? o);
}

/** a 是否为 master 的子序列（保持相对顺序） */
function isSubsequence(a, master) {
  let i = 0;
  for (const k of master) if (i < a.length && a[i] === k) i++;
  return i === a.length;
}

function joinKey(topic, line) {
  const o = JSON.parse(line);
  const d = o.data ?? {};
  if (topic.startsWith('tick.')) {
    return d.trade_id ? `${d.exchange}|${d.symbol}|${d.trade_id}` : `${d.exchange}|${d.symbol}|${d.timestamp}|${d.price}|${d.quantity}`;
  }
  if (topic.startsWith('kline.')) {
    // 以"K 线累计状态"（开盘时刻 + 累计成交量 + 累计成交笔数）为键：两条独立连接上同一状态会各自出现；
    // 不把 open/high/low/close 放进键里，这样这些取值若被改动会表现为"对应记录不同"而不是"对不上号"被悄悄忽略。
    return [d.exchange, d.symbol, d.interval, d.open_time, d.volume, d.number_of_trades].join('|');
  }
  if (topic.startsWith('quote.')) {
    return `${d.exchange}|${d.symbol}|${d.timestamp}|${d.bid_price}|${d.ask_price}`;
  }
  return line;
}

// ------------------------------------------------------------------ push
export function comparePush(oldDir, newDir, topics, log = console.log) {
  let failed = 0;
  let inconclusive = 0;
  for (const topic of topics) {
    const oldLines = readLines(join(oldDir, `${topic}.raw.jsonl`));
    const newLines = readLines(join(newDir, `${topic}.raw.jsonl`));
    const oldTs = readLines(join(oldDir, `${topic}.ts`)).map(Number);
    const newTs = readLines(join(newDir, `${topic}.ts`)).map(Number);
    log(`\n== ${topic}`);
    log(`   条数: old=${oldLines.length} new=${newLines.length}`);
    if (oldLines.length === 0 || newLines.length === 0) {
      log('   FAIL: 某一侧没有任何数据（空对空比较没有意义）');
      failed++;
      continue;
    }

    // 1) 字段顺序：new 的每条 data key 序列必须是 old 中"最长 key 序列"的子序列，且没有新增字段
    const master = oldLines.map(dataKeys).reduce((a, b) => (b.length > a.length ? b : a), []);
    const badOrder = newLines.filter((l) => !isSubsequence(dataKeys(l), master));
    const badOld = oldLines.filter((l) => !isSubsequence(dataKeys(l), master));
    if (badOrder.length || badOld.length) {
      failed++;
      log(`   FAIL 字段名/顺序: new 有 ${badOrder.length} 条、old 有 ${badOld.length} 条与基线 key 顺序 [${master.join(',')}] 不一致`);
      log(`        例: ${(badOrder[0] ?? badOld[0]).slice(0, 300)}`);
    } else {
      log(`   OK   字段名/顺序: 全部 ${newLines.length} 条与基线 key 顺序一致 [${master.join(',')}]`);
    }

    // 外层结构：{"type":..,"topic":..,"data":..} 的前缀字节
    const prefix = (l) => l.slice(0, l.indexOf('"data":') + 7);
    const oldPrefixes = new Set(oldLines.map(prefix));
    const badPrefix = newLines.filter((l) => !oldPrefixes.has(prefix(l)));
    if (badPrefix.length) {
      failed++;
      log(`   FAIL 外层结构: ${badPrefix.length} 条外层前缀与基线不同，例: ${prefix(badPrefix[0])}`);
    } else {
      log(`   OK   外层结构: 外层前缀与基线一致 (${[...oldPrefixes][0]})`);
    }

    // 2) 逐字节比较（按业务键 join，去掉 connector_receive_ts）
    const t0 = Math.max(oldTs[0], newTs[0]) + 2000;
    const t1 = Math.min(oldTs[oldTs.length - 1], newTs[newTs.length - 1]) - 2000;
    const idx = (lines, ts) => {
      const m = new Map();
      lines.forEach((l, i) => m.set(joinKey(topic, l), { l, t: ts[i] }));
      return m;
    };
    const om = idx(oldLines, oldTs);
    const nm = idx(newLines, newTs);
    let joined = 0, mismatched = 0, missingInNew = 0, missingInOld = 0;
    let firstMismatch = null;
    for (const [k, o] of om) {
      const n = nm.get(k);
      if (n) {
        joined++;
        if (normalize(o.l, topic) !== normalize(n.l, topic)) {
          mismatched++;
          firstMismatch ??= { old: normalize(o.l, topic), neu: normalize(n.l, topic) };
        }
      } else if (o.t >= t0 && o.t <= t1) missingInNew++;
    }
    for (const [k, n] of nm) if (!om.has(k) && n.t >= t0 && n.t <= t1) missingInOld++;
    if (joined === 0 || joined < Math.min(oldLines.length, newLines.length) * 0.3) {
      inconclusive++;
      log(`   INCONCLUSIVE: 可对应的记录只有 ${joined} 条（< 较小一侧条数的 30%），逐字节比较的覆盖不足`);
    } else if (mismatched) {
      failed++;
      log(`   FAIL 逐字节: 对应记录 ${joined} 条中 ${mismatched} 条不同`);
      log(`        old: ${firstMismatch.old.slice(0, 300)}`);
      log(`        new: ${firstMismatch.neu.slice(0, 300)}`);
    } else {
      log(`   OK   逐字节: 对应记录 ${joined} 条（去掉 connector_receive_ts 后）完全相同`);
    }
    // tick 是逐笔成交流，同一窗口内两侧应当看到同一批；kline 是按推送节奏更新，不要求逐条对应
    if (topic.startsWith('tick.')) {
      if (missingInNew || missingInOld) {
        failed++;
        log(`   FAIL 覆盖: 公共窗口内 old 有而 new 无=${missingInNew}，new 有而 old 无=${missingInOld}`);
      } else {
        log('   OK   覆盖: 公共窗口内两侧成交集合一致（无丢失/无多出）');
      }
    }

    // 3) 频率：条数比与最大间隔
    const gaps = (ts) => ts.slice(1).map((t, i) => t - ts[i]);
    const maxGap = (ts) => (ts.length > 1 ? Math.max(...gaps(ts)) : null);
    const ratio = newLines.length / oldLines.length;
    const tol = Math.max(5, oldLines.length * 0.2);
    const okFreq = Math.abs(newLines.length - oldLines.length) <= tol;
    log(`   ${okFreq ? 'OK  ' : 'FAIL'} 频率: new/old=${ratio.toFixed(2)}（容差 ±${tol.toFixed(0)} 条）, 最大间隔 old=${maxGap(oldTs)}ms new=${maxGap(newTs)}ms`);
    if (!okFreq) failed++;
  }
  return { failed, inconclusive };
}

// ------------------------------------------------------------------ noquote
export function assertNoQuote(dir, log = console.log) {
  const files = ['_all.raw.jsonl'];
  let total = 0;
  let quotes = 0;
  for (const f of files) {
    for (const l of readLines(join(dir, f))) {
      total++;
      if (/"type"\s*:\s*"quote"/.test(l) || /"topic"\s*:\s*"quote\./.test(l)) quotes++;
    }
  }
  const acks = readLines(join(dir, '_acks.jsonl'));
  log(`   ${dir}: 数据消息 ${total} 条, 其中 quote ${quotes} 条; 控制消息 ${acks.length} 条`);
  if (total === 0) {
    log('   INCONCLUSIVE: 没有任何数据消息，无法证明"无 quote"');
    return { failed: 0, inconclusive: 1 };
  }
  return { failed: quotes ? 1 : 0, inconclusive: 0 };
}

// ------------------------------------------------------------------ subs
const stableSubs = (arr) => arr.map((o) => JSON.stringify(o)).sort();
export function compareSubs(oldPath, newPath, log = console.log, allowAddedDataTypes = []) {
  const o = JSON.parse(readFileSync(oldPath, 'utf8'));
  const n = JSON.parse(readFileSync(newPath, 'utf8'));
  const os = stableSubs(o), ns = stableSubs(n);
  const removed = os.filter((x) => !ns.includes(x));
  const addedAll = ns.filter((x) => !os.includes(x));
  // 允许"新增 data_type 取值"（如开启 quote 后的 data_type=3）；其它任何新增仍算差异
  const allowed = addedAll.filter((x) => allowAddedDataTypes.includes(JSON.parse(x).data_type));
  const added = addedAll.filter((x) => !allowed.includes(x));
  if (allowed.length) log(`   允许的新增(data_type ∈ [${allowAddedDataTypes}]): ${allowed.length} 条`);
  log(`   old=${os.length} 条, new=${ns.length} 条, 缺失=${removed.length}, 非预期多出=${added.length}`);
  removed.slice(0, 5).forEach((x) => log(`     缺失: ${x}`));
  added.slice(0, 5).forEach((x) => log(`     多出: ${x}`));
  // 元素的 key 顺序与取值格式逐条比较
  const keyOrderOld = new Set(o.map((e) => Object.keys(e).join(',')));
  const keyOrderNew = new Set(n.map((e) => Object.keys(e).join(',')));
  const orderBad = [...keyOrderNew].filter((k) => !keyOrderOld.has(k));
  if (orderBad.length) log(`     字段顺序与基线不同: ${orderBad.join(' | ')}`);
  const failed = removed.length || added.length || orderBad.length ? 1 : 0;
  return { failed, inconclusive: os.length === 0 ? 1 : 0 };
}

// ------------------------------------------------------------------ metrics
function parseMetrics(text) {
  const types = new Map(), helps = new Map(), samples = new Set();
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (!line) continue;
    if (line.startsWith('# TYPE ')) { const [, , name, ty] = line.split(/\s+/); types.set(name, ty); continue; }
    if (line.startsWith('# HELP ')) { const m = line.match(/^# HELP (\S+) (.*)$/); if (m) helps.set(m[1], m[2]); continue; }
    if (line.startsWith('#')) continue;
    // 样本键 = 名称 + 标签（去掉最后的数值），直方图桶 le 也属于键的一部分
    const m = line.match(/^(\S+?)(\{.*\})?\s+\S+$/);
    if (m) samples.add(m[1] + (m[2] ?? ''));
  }
  return { types, helps, samples };
}
export function compareMetrics(oldPath, newPath, log = console.log) {
  const o = parseMetrics(readFileSync(oldPath, 'utf8'));
  const n = parseMetrics(readFileSync(newPath, 'utf8'));
  const removedSamples = [...o.samples].filter((s) => !n.samples.has(s));
  const addedSamples = [...n.samples].filter((s) => !o.samples.has(s));
  const typeChanged = [...o.types].filter(([k, v]) => n.types.get(k) !== v);
  const helpChanged = [...o.helps].filter(([k, v]) => n.helps.get(k) !== v);
  log(`   old 样本 ${o.samples.size}, new 样本 ${n.samples.size}; 删除=${removedSamples.length}, 新增=${addedSamples.length}`);
  log(`   old 指标族 ${o.types.size}, new 指标族 ${n.types.size}; TYPE 变更/删除=${typeChanged.length}, HELP 变更/删除=${helpChanged.length}`);
  removedSamples.slice(0, 10).forEach((s) => log(`     删除: ${s}`));
  typeChanged.slice(0, 10).forEach(([k, v]) => log(`     TYPE 变化: ${k} ${v} -> ${n.types.get(k)}`));
  helpChanged.slice(0, 10).forEach(([k]) => log(`     HELP 变化: ${k}`));
  const newFamilies = [...n.types.keys()].filter((k) => !o.types.has(k));
  log(`   新增指标族: ${newFamilies.join(', ') || '(无)'}`);
  const addedPreview = addedSamples.slice(0, 12);
  log(`   新增样本(前 ${addedPreview.length} 条): ${addedPreview.join(' ; ')}`);
  const failed = removedSamples.length || typeChanged.length || helpChanged.length ? 1 : 0;
  return { failed, inconclusive: o.samples.size === 0 ? 1 : 0 };
}

// ------------------------------------------------------------------ selftest
function selftest() {
  const base = join(tmpdir(), `compare-selftest-${process.pid}`);
  rmSync(base, { recursive: true, force: true });
  const mk = (name, topic, lines, tsStart = 1_000_000) => {
    const d = join(base, name);
    mkdirSync(d, { recursive: true });
    writeFileSync(join(d, `${topic}.raw.jsonl`), lines.join('\n') + '\n');
    writeFileSync(join(d, `${topic}.ts`), lines.map((_, i) => tsStart + i * 500).join('\n') + '\n');
    writeFileSync(join(d, '_all.raw.jsonl'), lines.join('\n') + '\n');
    writeFileSync(join(d, '_acks.jsonl'), '');
    return d;
  };
  const topic = 'tick.binance.BNBUSDT';
  const mkTick = (i, extra = {}) => {
    const o = {
      exchange: 'binance', symbol: 'BNBUSDT', timestamp: 1791532727000 + i, price: '744.870', quantity: '0.02',
      trade_id: 873742000 + i, exchange_event_ts: 1791532727000 + i, connector_receive_ts: 1791532727060 + i, ...extra,
    };
    return `{"type":"tick","topic":"${topic}","data":${JSON.stringify(o)}}`;
  };
  const N = 60;
  const oldLines = Array.from({ length: N }, (_, i) => mkTick(i));
  const quiet = () => {};
  const run = (name, newLines, ts) => comparePush(mk('old', topic, oldLines), mk(name, topic, newLines, ts), [topic], quiet);

  const results = [];
  const expect = (label, got, want) => {
    const ok = want === 'pass' ? got.failed === 0 && got.inconclusive === 0 : want === 'fail' ? got.failed > 0 : got.inconclusive > 0;
    results.push({ label, ok, got, want });
  };

  // 1. 仅 connector_receive_ts 不同 -> 应通过（这正是任务书允许的差异）
  expect('仅 connector_receive_ts 不同', run('n1', oldLines.map((l, i) => l.replace(/"connector_receive_ts":\d+/, `"connector_receive_ts":${9_999_000 + i * 7}`))), 'pass');
  // 2. 字段顺序交换 price/quantity -> 应失败
  expect('字段顺序被交换', run('n2', oldLines.map((l) => l.replace('"price":"744.870","quantity":"0.02"', '"quantity":"0.02","price":"744.870"'))), 'fail');
  // 3. 数值格式改变 744.870 -> 744.87 -> 应失败
  expect('数值格式被改变', run('n3', oldLines.map((l) => l.replace('"744.870"', '"744.87"'))), 'fail');
  // 4. 新增字段 -> 应失败
  expect('多出一个字段', run('n4', oldLines.map((l, i) => mkTick(i, { bid_price: '1' }))), 'fail');
  // 5. 丢了一半记录 -> 应失败（频率与覆盖）
  expect('丢失一半记录', run('n5', oldLines.filter((_, i) => i % 2 === 0), 1_000_000), 'fail');
  // 6. 外层 type 被改 -> 应失败
  expect('外层 type 被改', run('n6', oldLines.map((l) => l.replace('"type":"tick"', '"type":"trade"'))), 'fail');
  // 7. 值被改 -> 应失败
  expect('某条价格被改', run('n7', oldLines.map((l, i) => (i === 10 ? l.replace('"744.870"', '"999.000"') : l))), 'fail');
  // 8. 完全不重叠 -> 应"结论不充分"，而不是通过
  expect('无重叠记录不得算通过', run('n8', Array.from({ length: N }, (_, i) => mkTick(i + 100000))), 'inconclusive');
  // 9. 一侧为空 -> 应失败
  expect('一侧为空', (() => { const d = join(base, 'empty'); mkdirSync(d, { recursive: true }); writeFileSync(join(d, `${topic}.raw.jsonl`), ''); writeFileSync(join(d, `${topic}.ts`), ''); return comparePush(mk('old', topic, oldLines), d, [topic], quiet); })(), 'fail');

  // kline：exchange_event_ts / connector_receive_ts 不同应通过；close 不同应失败
  const kt = 'kline.5m.binance.BTCUSDT';
  const mkK = (i, o = {}) => {
    const d = { exchange: 'binance', symbol: 'BTCUSDT', interval: '5m', open_time: 1791532500000 + Math.floor(i / 10) * 300000, open: '80000.0', high: '80010.0', low: '79990.0', close: `${80000 + i}.0`, volume: `${i}.5`, close_time: 1791532799999, quote_asset_volume: '1.0', number_of_trades: 100 + i, exchange_event_ts: 1791532600000 + i, connector_receive_ts: 1791532600050 + i, ...o };
    return `{"type":"kline","topic":"${kt}","data":${JSON.stringify(d)}}`;
  };
  const kOld = Array.from({ length: 40 }, (_, i) => mkK(i));
  const runK = (name, lines) => comparePush(mk('kold', kt, kOld), mk(name, kt, lines), [kt], quiet);
  expect('kline: 事件时刻/接收时刻不同应通过', runK('k1', kOld.map((l, i) => l.replace(/"exchange_event_ts":\d+/, `"exchange_event_ts":${5_000 + i}`).replace(/"connector_receive_ts":\d+/, `"connector_receive_ts":${7_000 + i}`))), 'pass');
  expect('kline: close 被改必须被发现', runK('k2', kOld.map((l, i) => (i === 7 ? l.replace(/"close":"[^"]+"/, '"close":"1.0"') : l))), 'fail');
  expect('kline: 字段顺序被交换必须被发现', runK('k3', kOld.map((l) => l.replace('"open":"80000.0","high":"80010.0"', '"high":"80010.0","open":"80000.0"'))), 'fail');

  // noquote
  const dClean = mk('nq_clean', topic, oldLines);
  expect('noquote: 干净样本', assertNoQuote(dClean, quiet), 'pass');
  const dDirty = mk('nq_dirty', topic, [...oldLines, '{"type":"quote","topic":"quote.binance.BNBUSDT","data":{}}']);
  expect('noquote: 混入 quote 必须被发现', assertNoQuote(dDirty, quiet), 'fail');
  const dEmpty = mk('nq_empty', topic, []);
  writeFileSync(join(dEmpty, '_all.raw.jsonl'), '');
  expect('noquote: 空抓取不得算通过', assertNoQuote(dEmpty, quiet), 'inconclusive');

  // subs
  const sOld = join(base, 's_old.json'), sSame = join(base, 's_same.json'), sMiss = join(base, 's_miss.json'), sAdd = join(base, 's_add.json'), sOrd = join(base, 's_ord.json');
  const subs = [{ exchange: 'binance', data_type: 1, symbol: 'BTCUSDT' }, { exchange: 'okx', data_type: 2, symbol: 'ETH-USDT-SWAP', kline_interval: '5m' }];
  writeFileSync(sOld, JSON.stringify(subs));
  writeFileSync(sSame, JSON.stringify([...subs].reverse())); // 顺序不同但集合相同
  writeFileSync(sMiss, JSON.stringify(subs.slice(0, 1)));
  writeFileSync(sAdd, JSON.stringify([...subs, { exchange: 'binance', data_type: 3, symbol: 'BNBUSDT' }]));
  writeFileSync(sOrd, JSON.stringify([{ symbol: 'BTCUSDT', exchange: 'binance', data_type: 1 }, subs[1]]));
  expect('subs: 集合相同(顺序不同)', compareSubs(sOld, sSame, quiet), 'pass');
  expect('subs: 缺一条必须被发现', compareSubs(sOld, sMiss, quiet), 'fail');
  expect('subs: 多一条(QUOTE)必须被发现', compareSubs(sOld, sAdd, quiet), 'fail');
  expect('subs: 字段顺序改变必须被发现', compareSubs(sOld, sOrd, quiet), 'fail');
  expect('subs: 允许 data_type=3 新增时，QUOTE 多出应通过', compareSubs(sOld, sAdd, quiet, [3]), 'pass');
  const sAddTick = join(base, 's_addtick.json');
  writeFileSync(sAddTick, JSON.stringify([...subs, { exchange: 'binance', data_type: 1, symbol: 'ETHUSDT' }]));
  expect('subs: 允许 data_type=3 时，多出 TICK 仍必须被发现', compareSubs(sOld, sAddTick, quiet, [3]), 'fail');
  expect('subs: 允许 data_type=3 时，缺失仍必须被发现', compareSubs(sOld, sMiss, quiet, [3]), 'fail');

  // metrics
  const mOld = join(base, 'm_old.txt'), mAdd = join(base, 'm_add.txt'), mDel = join(base, 'm_del.txt'), mTyp = join(base, 'm_typ.txt');
  const m0 = ['# HELP a_total A', '# TYPE a_total counter', 'a_total 1', '# HELP g G', '# TYPE g gauge', 'g{x="1"} 2', ''].join('\n');
  writeFileSync(mOld, m0);
  writeFileSync(mAdd, m0 + ['# HELP q_total Q', '# TYPE q_total counter', 'q_total 0', 'g{x="2"} 3', ''].join('\n'));
  writeFileSync(mDel, m0.replace('g{x="1"} 2\n', ''));
  writeFileSync(mTyp, m0.replace('# TYPE a_total counter', '# TYPE a_total gauge'));
  expect('metrics: 只新增', compareMetrics(mOld, mAdd, quiet), 'pass');
  expect('metrics: 删除样本必须被发现', compareMetrics(mOld, mDel, quiet), 'fail');
  expect('metrics: 改类型必须被发现', compareMetrics(mOld, mTyp, quiet), 'fail');

  rmSync(base, { recursive: true, force: true });
  let bad = 0;
  for (const r of results) {
    console.log(`${r.ok ? 'OK  ' : 'BAD '} selftest: ${r.label}  (期望=${r.want}, failed=${r.got.failed}, inconclusive=${r.got.inconclusive})`);
    if (!r.ok) bad++;
  }
  console.log(bad ? `SELFTEST FAILED: ${bad}/${results.length}` : `SELFTEST OK: ${results.length}/${results.length} 个已知样本均得到预期结论`);
  return bad ? 1 : 0;
}

// ------------------------------------------------------------------ main
function main() {
  const [cmd, ...a] = process.argv.slice(2);
  let r;
  switch (cmd) {
    case 'push':
      if (a.length < 3) usage();
      r = comparePush(a[0], a[1], a.slice(2));
      break;
    case 'noquote':
      if (a.length < 1) usage();
      r = assertNoQuote(a[0]);
      break;
    case 'subs': {
      const allow = a.filter((x) => x.startsWith('--allow-data-type=')).flatMap((x) => x.split('=')[1].split(',').map(Number));
      const rest = a.filter((x) => !x.startsWith('--'));
      if (rest.length !== 2) usage();
      r = compareSubs(rest[0], rest[1], console.log, allow);
      break;
    }
    case 'metrics':
      if (a.length !== 2) usage();
      r = compareMetrics(a[0], a[1]);
      break;
    case 'selftest':
      process.exit(selftest());
      break;
    default:
      usage();
  }
  const code = r.failed ? 1 : r.inconclusive ? 3 : 0;
  console.log(code === 0 ? '\nRESULT: PASS' : code === 1 ? `\nRESULT: FAIL (${r.failed} 项差异)` : `\nRESULT: INCONCLUSIVE (${r.inconclusive} 项结论不充分)`);
  process.exit(code);
}
function usage() {
  console.error('usage: compare.mjs push <old_dir> <new_dir> <topic>... | noquote <dir> | subs <old> <new> [--allow-data-type=3] | metrics <old> <new> | selftest');
  process.exit(2);
}
if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
