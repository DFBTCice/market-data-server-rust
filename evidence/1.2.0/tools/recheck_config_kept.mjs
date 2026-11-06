#!/usr/bin/env node
// 用原始 timeline.jsonl 复核「配置订阅全程不少」：配置订阅以首个采样里服务端 GET 返回的 source=config 为准。
// 背景：canary_consumers.mjs 第一版把"初始订阅里除已知运行期键之外的全部"当成配置订阅，198 现网另有 botd 登记的
// 运行期 K 线（binance SNDKUSDT 15m），它被正确地闲置退订后，第一版判据误报 config_subscriptions_kept=false。
// 内置正对照：把一个配置键从中间某个采样里删掉，复核必须报出缺失。
// 用法：node evidence/1.2.0/tools/recheck_config_kept.mjs <timeline.jsonl>...
import { readFileSync } from 'node:fs';

const key = (s) => `${s.exchange}|${s.data_type}|${s.symbol}|${s.kline_interval ?? ''}`;
function check(rows) {
  const first = rows[0].raw_subs;
  const config = first.filter((s) => s.source === 'config').map(key);
  const missing = [];
  for (const r of rows) for (const k of config) if (!r.subs.includes(k)) missing.push(`t=${r.t} ${k}`);
  const runtimeGone = first.filter((s) => s.source === 'runtime').map(key).filter((k) => !rows.at(-1).subs.includes(k));
  return { samples: rows.length, config_keys: config, missing, initial_runtime_keys_gone: runtimeGone,
    config_subscriptions_kept: config.length > 0 && missing.length === 0 };
}
let ok = true;
for (const file of process.argv.slice(2)) {
  const rows = readFileSync(file, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  const result = check(rows);
  const mutated = structuredClone(rows);
  const mid = Math.floor(mutated.length / 2);
  mutated[mid].subs = mutated[mid].subs.filter((k) => k !== result.config_keys[0]);
  const control = check(mutated);
  const controlCaught = !control.config_subscriptions_kept && control.missing.length === 1;
  console.log(JSON.stringify({ file, ...result, positive_control: { removed: `t=${mutated[mid].t} ${result.config_keys[0]}`, caught: controlCaught } }, null, 1));
  ok &&= result.config_subscriptions_kept && controlCaught;
}
process.exit(ok ? 0 : 1);
