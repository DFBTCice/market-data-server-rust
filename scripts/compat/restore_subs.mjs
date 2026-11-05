#!/usr/bin/env node
// 把"基线订阅集合"里目标实例缺少的 TICK/KLINE 订阅补回去（只增不删，不处理 QUOTE）。
//
// 背景：动态订阅（POST /api/v1/subscriptions）不会写回 config.yaml，进程/容器重启后丢失。
// 现网 198 在 1.0.0 上除 config.yaml 的 8 条外还有 9 条运行期注册的订阅，升级重启后需要补回，
// 否则下游（OMS 等）会丢这几路行情。
//
// 用法：
//   node scripts/compat/restore_subs.mjs <baseline-subscriptions.json> <target-base-url> [--apply]
//   不带 --apply 只打印差异（dry-run）。
import { readFileSync } from 'node:fs';

const [, , baselineFile, base, flag] = process.argv;
if (!baselineFile || !base) {
  console.error('usage: restore_subs.mjs <baseline-subscriptions.json> <base-url> [--apply]');
  process.exit(2);
}
const apply = flag === '--apply';

const key = (s) =>
  `${s.exchange}|${s.data_type}|${s.symbol}|${s.kline_interval ?? ''}`;

const baseline = JSON.parse(readFileSync(baselineFile, 'utf8'));
const cur = await (await fetch(`${base}/api/v1/subscriptions`)).json();
const have = new Set(cur.map(key));
const missing = baseline.filter((s) => (s.data_type === 1 || s.data_type === 2) && !have.has(key(s)));

console.log(`baseline=${baseline.length} current=${cur.length} missing(TICK/KLINE)=${missing.length}`);
let failed = 0;
for (const s of missing) {
  const body = {
    exchange: s.exchange,
    type: s.data_type === 1 ? 'TICK' : 'KLINE',
    symbols: [s.symbol],
  };
  if (s.data_type === 2) body.interval = s.kline_interval;
  if (!apply) {
    console.log('[dry-run] would POST', JSON.stringify(body));
    continue;
  }
  const r = await fetch(`${base}/api/v1/subscriptions`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const text = await r.text();
  console.log(r.status, JSON.stringify(body), text.slice(0, 120));
  if (!r.ok) failed++;
}

if (apply) {
  const after = await (await fetch(`${base}/api/v1/subscriptions`)).json();
  const afterSet = new Set(after.map(key));
  const stillMissing = baseline.filter(
    (s) => (s.data_type === 1 || s.data_type === 2) && !afterSet.has(key(s)),
  );
  const extra = after.filter((s) => !new Set(baseline.map(key)).has(key(s)));
  console.log(`after: total=${after.length} stillMissing=${stillMissing.length} extraVsBaseline=${extra.length}`);
  if (failed || stillMissing.length) process.exit(1);
}
