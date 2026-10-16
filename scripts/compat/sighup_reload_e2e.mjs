#!/usr/bin/env node
// sighup_reload_e2e.mjs -- SIGHUP 配置重载端到端测试（真实 md-server 进程 + 真实 SIGHUP + 假上游）
//
// 验证点（每条断言都按"正确行为"来写，所以对有缺陷的版本会 FAIL，对修复后的版本应全部 PASS）：
//   1. 四个连接器地址（OKX public / OKX business / Binance market / Binance public(quote)）任意一个变更：
//      SIGHUP 明确提示"需要重启"，不得输出"已成功应用"；连续两次 SIGHUP 的结论相同。
//   2. 待重启的变更不会影响订阅列表的热更新；订阅热更新不重复下发。
//   3. 连接器重连不会读新配置（设计如此，改地址必须重启）；重启后连接器连到新地址。
//   4. YAML 读取/解析/校验失败：保留运行配置并明确报错，进程不退出，修好后可再次重载。
//   5. 端口偏移 / 环境变量覆盖下，未改动的配置文件 SIGHUP 不得误报"监听地址变更"。
//   6. 连接器未运行（启动时列表为空）时，订阅列表变更不得静默吞掉、不得误报成功。
//   7. log_level 变更不得声称"已应用"（日志过滤器启动时由 RUST_LOG 决定）。
//
// 用法:
//   node scripts/compat/sighup_reload_e2e.mjs --runner native --bin target/debug/md-server --out <dir>
//   node scripts/compat/sighup_reload_e2e.mjs --runner docker --image <镜像> --out <dir>
//     docker 模式用 linux/amd64 镜像，入口为二进制本身；上游地址用 host.docker.internal。
//     例：用线上同一个二进制构建本地探针镜像（FROM scratch + COPY dist/md-server-1.1.0-linux-x86_64）。
//   可选: --only id1,id2   只跑指定场景；--list 列出场景；--report-only 有失败也返回 0（对旧版本取证用）
//
// 输出（<out>/ 下）：results.md、results.json、<场景>/md-server.log、<场景>/upstream-events.jsonl、<场景>/config.step*.yaml
// 退出码：失败断言数（--report-only 时恒为 0）。

import { spawn, execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, renameSync, mkdtempSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import net from 'node:net';
import { FakeUpstream } from './fake_upstream.mjs';

// ------------------------------------------------------------------ 参数
const args = process.argv.slice(2);
const opt = (name, def = undefined) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : def;
};
const flag = (name) => args.includes(`--${name}`);

const RUNNER = opt('runner', 'native');
const BIN = opt('bin');
const IMAGE = opt('image');
const OUT = resolve(opt('out', 'sighup-e2e-out'));
const ONLY = (opt('only', '') || '').split(',').filter(Boolean);
const REPORT_ONLY = flag('report-only');
const HOST_FOR_UPSTREAM = RUNNER === 'docker' ? 'host.docker.internal' : '127.0.0.1';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const stripAnsi = (s) => s.replace(/\x1b\[[0-9;]*m/g, '');

async function waitFor(pred, timeoutMs, what) {
  const t0 = Date.now();
  for (;;) {
    if (pred()) return;
    if (Date.now() - t0 > timeoutMs) throw new Error(`timeout waiting for: ${what}`);
    await sleep(100);
  }
}

function freePort() {
  return new Promise((res, rej) => {
    const s = net.createServer();
    s.once('error', rej);
    s.listen(0, '127.0.0.1', () => {
      const p = s.address().port;
      s.close(() => res(p));
    });
  });
}

const canBind = (port) =>
  new Promise((res) => {
    const s = net.createServer();
    s.once('error', () => res(false));
    s.listen(port, '127.0.0.1', () => s.close(() => res(true)));
  });

/** 找一个 base，使 base 与 base+off 都空闲（端口偏移场景用）；在 20000~40000 内随机扫描 */
async function freePortWithOffset(off) {
  for (let i = 0; i < 300; i++) {
    const p = 20000 + Math.floor(Math.random() * 20000);
    if ((await canBind(p)) && (await canBind(p + off))) return p;
  }
  throw new Error('no free port pair');
}

// ------------------------------------------------------------------ 被测进程的两种运行方式
class NativeRunner {
  constructor(bin) {
    if (!bin) throw new Error('--bin required for native runner');
    this.bin = resolve(bin);
  }
  async start({ dir, args: extra = [], env = {}, onLine }) {
    this.child = spawn(this.bin, ['--config', join(dir, 'config.yaml'), ...extra], {
      env: { ...process.env, RUST_LOG: 'info', NO_COLOR: '1', ...env },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const feed = (stream) => {
      let buf = '';
      stream.on('data', (d) => {
        buf += d.toString();
        let i;
        while ((i = buf.indexOf('\n')) >= 0) {
          onLine(buf.slice(0, i));
          buf = buf.slice(i + 1);
        }
      });
    };
    feed(this.child.stdout);
    feed(this.child.stderr);
    this.exited = new Promise((r) => this.child.on('exit', (code, sig) => r({ code, sig })));
  }
  sendHup() {
    process.kill(this.child.pid, 'SIGHUP');
  }
  alive() {
    return this.child.exitCode === null && this.child.signalCode === null;
  }
  async stop() {
    if (!this.child) return;
    if (this.alive()) {
      this.child.kill('SIGTERM');
      await Promise.race([this.exited, sleep(8000)]);
      if (this.alive()) this.child.kill('SIGKILL');
    }
  }
}

let dockerSeq = 0;
class DockerRunner {
  constructor(image) {
    if (!image) throw new Error('--image required for docker runner');
    this.image = image;
  }
  async start({ dir, args: extra = [], env = {}, onLine }) {
    this.name = `sighup-e2e-${process.pid}-${++dockerSeq}`;
    const envArgs = Object.entries({ RUST_LOG: 'info', NO_COLOR: '1', ...env }).flatMap(([k, v]) => ['-e', `${k}=${v}`]);
    execFileSync('docker', [
      'run', '-d', '--name', this.name, '--platform', 'linux/amd64',
      '-v', `${dir}:/cfg`, ...envArgs, this.image, '--config', '/cfg/config.yaml', ...extra,
    ]);
    this.logs = spawn('docker', ['logs', '-f', this.name], { stdio: ['ignore', 'pipe', 'pipe'] });
    const feed = (stream) => {
      let buf = '';
      stream.on('data', (d) => {
        buf += d.toString();
        let i;
        while ((i = buf.indexOf('\n')) >= 0) {
          onLine(buf.slice(0, i));
          buf = buf.slice(i + 1);
        }
      });
    };
    feed(this.logs.stdout);
    feed(this.logs.stderr);
  }
  sendHup() {
    execFileSync('docker', ['kill', '--signal=HUP', this.name]);
  }
  alive() {
    const r = spawnSync('docker', ['inspect', '-f', '{{.State.Running}}', this.name]);
    return r.stdout.toString().trim() === 'true';
  }
  async stop() {
    if (!this.name) return;
    spawnSync('docker', ['stop', '-t', '3', this.name]);
    await sleep(300);
    if (this.logs) this.logs.kill();
    spawnSync('docker', ['rm', '-f', this.name]);
    this.name = null;
  }
}

// ------------------------------------------------------------------ 配置模型与 YAML 渲染（固定结构，不引入 YAML 库）
const q = (s) => JSON.stringify(s);
const list = (a) => `[${a.map(q).join(', ')}]`;
const klines = (m) => {
  const e = Object.entries(m);
  return e.length ? `{ ${e.map(([k, v]) => `${q(k)}: ${list(v)}`).join(', ')} }` : '{}';
};

function renderYaml(c) {
  const b = c.binance;
  const o = c.okx;
  return `log_level: ${q(c.logLevel)}
grpc_server:
  enabled: true
  listen_address: ${q(c.grpc)}
admin_server:
  listen_address: ${q(c.admin)}
api_gateway:
  enabled: true
  listen_address: ${q(c.gateway)}
processor:
  tick_channel_buffer: 1000
  kline_channel_buffer: 1000
  broadcast_capacity: 4096
connectors:
  binance:
    enabled: ${b.enabled}
    stream_base_url: ${q(b.market)}
    stream_base_url_public: ${q(b.public)}
    rest_base_url: "http://127.0.0.1:1"
    subscribe_ticks: ${list(b.ticks)}
    subscribe_klines: ${klines(b.klines)}
    reconnect_delay: "1s"
    ping_interval: "30s"
    quotes_enabled: ${b.quotesEnabled}
    subscribe_quotes: ${list(b.quotes)}
  okx:
    enabled: ${o.enabled}
    stream_base_url_public: ${q(o.pub)}
    stream_base_url_business: ${q(o.biz)}
    rest_base_url: "http://127.0.0.1:1"
    subscribe_ticks: ${list(o.ticks)}
    subscribe_klines: ${klines(o.klines)}
    reconnect_delay: "1s"
    ping_interval: "25s"
    quotes_enabled: ${o.quotesEnabled}
    subscribe_quotes: ${list(o.quotes)}
`;
}

// 四类地址：label 后缀 A = 启动时的旧上游，B = 改后的新上游
const KINDS = {
  okxPublic: { re: /okx\.stream_base_url_public/, set: (c, u) => (c.okx.pub = u), a: 'okxPubA', b: 'okxPubB', title: 'OKX Public' },
  okxBusiness: { re: /okx\.stream_base_url_business/, set: (c, u) => (c.okx.biz = u), a: 'okxBizA', b: 'okxBizB', title: 'OKX Business' },
  binanceMarket: { re: /binance\.stream_base_url(?!_)/, set: (c, u) => (c.binance.market = u), a: 'bnMarketA', b: 'bnMarketB', title: 'Binance Market(tick/kline)' },
  binancePublic: { re: /binance\.stream_base_url_public/, set: (c, u) => (c.binance.public = u), a: 'bnPublicA', b: 'bnPublicB', title: 'Binance Public(quote)' },
};

// ------------------------------------------------------------------ 断言用的正则
const FINAL_MARKER = /(config reloaded and applied successfully|some config changes require|failed to reload config)/;
const SUCCESS = /config reloaded and applied successfully/;
const RESTART_SUMMARY = /some config changes require connector restart -- please restart the service/;
const LOAD_FAILED = /failed to reload config/;
// 订阅 / 退订帧（OKX: op=subscribe|unsubscribe；Binance: method=SUBSCRIBE|UNSUBSCRIBE），不含 ping 等心跳
const SUB_FRAME = /"op":"(un)?subscribe"|"method":"(UN)?SUBSCRIBE"/;

// ------------------------------------------------------------------ 场景运行环境
const results = [];

class Harness {
  constructor(id, title, upstream) {
    this.id = id;
    this.title = title;
    this.up = upstream;
    this.lines = [];
    this.step = 0;
    this.dir = mkdtempSync(join(tmpdir(), `sighup-${id}-`));
    this.outDir = join(OUT, id);
    mkdirSync(this.outDir, { recursive: true });
    this.runner = RUNNER === 'docker' ? new DockerRunner(IMAGE) : new NativeRunner(BIN);
  }

  url(label) {
    return `ws://${HOST_FOR_UPSTREAM}:${this.up.port(label)}/ws`;
  }

  async defaultCfg() {
    return {
      logLevel: 'info',
      grpc: `127.0.0.1:${await freePort()}`,
      admin: `127.0.0.1:${await freePort()}`,
      gateway: `127.0.0.1:${await freePort()}`,
      binance: {
        enabled: true,
        market: this.url('bnMarketA'),
        public: this.url('bnPublicA'),
        ticks: ['BTCUSDT'],
        klines: { '1m': ['BTCUSDT'] },
        quotesEnabled: true,
        quotes: ['BNBUSDT'],
      },
      okx: {
        enabled: true,
        pub: this.url('okxPubA'),
        biz: this.url('okxBizA'),
        ticks: ['BTC-USDT-SWAP'],
        klines: { '1m': ['BTC-USDT-SWAP'] },
        quotesEnabled: true,
        quotes: ['BNB-USDT-SWAP'],
      },
    };
  }

  /** 按配置推算应出现的"connected and synced"数量 */
  expectedConnectors(c) {
    let n = 0;
    if (c.binance.enabled) {
      n += 1; // binance tick/kline
      if (c.binance.quotesEnabled && c.binance.quotes.length) n += 1;
    }
    if (c.okx.enabled) {
      if (c.okx.ticks.length) n += 1;
      if (Object.keys(c.okx.klines).length) n += 1;
      if (c.okx.quotesEnabled && c.okx.quotes.length) n += 1;
    }
    return n;
  }

  write(cfg) {
    const y = renderYaml(cfg);
    writeFileSync(join(this.dir, 'config.yaml'), y);
    writeFileSync(join(this.outDir, `config.step${++this.step}.yaml`), y);
  }

  writeRaw(text) {
    writeFileSync(join(this.dir, 'config.yaml'), text);
    writeFileSync(join(this.outDir, `config.step${++this.step}.raw.yaml`), text);
  }

  onLine(raw) {
    const text = stripAnsi(raw);
    this.lines.push({ t: Date.now(), text });
  }

  count(re) {
    return this.lines.filter((l) => re.test(l.text)).length;
  }

  async start(cfg, { args: extra = [], env = {} } = {}) {
    this.write(cfg);
    this.cfg = cfg;
    const before = this.count(/connected and synced/);
    await this.runner.start({ dir: this.dir, args: extra, env, onLine: (l) => this.onLine(l) });
    const want = before + this.expectedConnectors(cfg);
    await waitFor(() => this.count(/connected and synced/) >= want, 25000, `${want} connectors synced`);
    // 日志流（尤其 docker logs）会比 WS 帧晚/早到：留一段静默期，让启动阶段的订阅帧、心跳帧都被假上游处理完，
    // 避免它们落进后面某个断言的观察窗口（曾导致"没有任何订阅动作"偶发误判）。
    await sleep(1000);
    this.onLine('[harness] ---- process ready ----');
  }

  async restart(cfg, opts) {
    await this.runner.stop();
    this.onLine('[harness] ---- restart ----');
    await this.start(cfg, opts);
  }

  /**
   * 发 SIGHUP，等到重载结论行出现（或超时），返回此后新增的日志。
   * quiet=true：被测进程的日志级别会过滤掉结论行（如 RUST_LOG=warn），只固定等待 1.2s。
   */
  async hup(label = '', { quiet = false } = {}) {
    const from = this.lines.length;
    this.onLine(`[harness] ---- SIGHUP ${label} ----`);
    const from2 = this.lines.length;
    this.runner.sendHup();
    if (quiet) {
      await sleep(1200);
    } else {
      try {
        await waitFor(() => this.lines.slice(from2).some((l) => FINAL_MARKER.test(l.text)), 6000, 'reload verdict line');
      } catch {
        // 没有结论行本身就是信息：交给断言判定
      }
    }
    await sleep(400);
    const lines = this.lines.slice(from);
    return { lines, text: lines.map((l) => l.text).join('\n') };
  }

  check(name, ok, detail = '') {
    results.push({ scenario: this.id, title: this.title, name, ok: !!ok, detail });
    console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name}${!ok && detail ? `\n        ${detail.split('\n').slice(0, 6).join('\n        ')}` : ''}`);
  }

  async finish() {
    await this.runner.stop();
    writeFileSync(join(this.outDir, 'md-server.log'), this.lines.map((l) => `${new Date(l.t).toISOString()} ${l.text}`).join('\n') + '\n');
    writeFileSync(join(this.outDir, 'upstream-events.jsonl'), this.up.events.map((e) => JSON.stringify(e)).join('\n') + '\n');
    try {
      rmSync(this.dir, { recursive: true, force: true });
    } catch {}
  }
}

// ------------------------------------------------------------------ 场景
const scenarios = [];
const scenario = (id, title, fn, o = {}) => scenarios.push({ id, title, fn, ...o });

const tail = (text, n = 6) => text.split('\n').slice(-n).join('\n');

// 0. 对照：配置文件没有任何变化，SIGHUP 应报"成功"，且没有任何"需要重启"
scenario('s00-noop', '无变更 SIGHUP：应成功，无重启提示', async (h) => {
  await h.start(await h.defaultCfg());
  const r = await h.hup();
  h.check('报告成功', SUCCESS.test(r.text), tail(r.text));
  h.check('无重启提示', !/requires restart|require connector restart|NOT applied/.test(r.text), tail(r.text));
});

// 1~4. 四个地址，各改一个，连续两次 SIGHUP
for (const [kind, k] of Object.entries(KINDS)) {
  scenario(`s1-url-${kind}`, `仅修改 ${k.title} 地址，连续两次 SIGHUP`, async (h) => {
    const cfg = await h.defaultCfg();
    await h.start(cfg);
    const next = structuredClone(cfg);
    k.set(next, h.url(k.b));
    h.write(next);
    for (const n of [1, 2]) {
      const r = await h.hup(`#${n}`);
      h.check(`第${n}次：检测到 ${k.title} 地址变更并点名`, k.re.test(r.text), tail(r.text));
      h.check(`第${n}次：给出"需要重启"的结论`, RESTART_SUMMARY.test(r.text), tail(r.text));
      h.check(`第${n}次：不得报告"已成功应用"`, !SUCCESS.test(r.text), tail(r.text));
    }
    h.check('旧上游连接保持、新上游无连接（地址未被热切换）', h.up.connects(k.b) === 0 && h.up.active(k.a) >= 1, `connects(B)=${h.up.connects(k.b)} active(A)=${h.up.active(k.a)}`);
  });
}

// 5. 自动重连不会读新配置：改四个地址 + SIGHUP 后掐断旧上游，重连仍回旧地址（设计如此，必须重启）
scenario('s2-reconnect-keeps-startup-urls', '改地址后上游断线：自动重连仍用启动时地址（需重启才换）', async (h) => {
  const cfg = await h.defaultCfg();
  await h.start(cfg);
  const next = structuredClone(cfg);
  for (const k of Object.values(KINDS)) k.set(next, h.url(k.b));
  h.write(next);
  const r = await h.hup();
  h.check('SIGHUP 给出"需要重启"，且不报成功', RESTART_SUMMARY.test(r.text) && !SUCCESS.test(r.text), tail(r.text));
  const before = Object.fromEntries(Object.values(KINDS).map((k) => [k.a, h.up.connects(k.a)]));
  for (const k of Object.values(KINDS)) h.up.dropAll(k.a);
  await sleep(4500);
  for (const k of Object.values(KINDS)) {
    h.check(`${k.title}：重连回旧地址`, h.up.connects(k.a) > before[k.a], `connects(A) ${before[k.a]} -> ${h.up.connects(k.a)}`);
    h.check(`${k.title}：新地址仍无连接`, h.up.connects(k.b) === 0, `connects(B)=${h.up.connects(k.b)}`);
  }
});

// 6. 重启后连接器取得新地址
scenario('s3-restart-picks-new-urls', '改四个地址后重启：连接器连到新地址', async (h) => {
  const cfg = await h.defaultCfg();
  await h.start(cfg);
  const next = structuredClone(cfg);
  for (const k of Object.values(KINDS)) k.set(next, h.url(k.b));
  const aBefore = Object.fromEntries(Object.values(KINDS).map((k) => [k.a, h.up.connects(k.a)]));
  await h.restart(next);
  for (const k of Object.values(KINDS)) {
    h.check(`${k.title}：新进程连到新地址`, h.up.connects(k.b) >= 1, `connects(B)=${h.up.connects(k.b)}`);
    h.check(`${k.title}：新进程不再连旧地址`, h.up.connects(k.a) === aBefore[k.a], `connects(A) ${aBefore[k.a]} -> ${h.up.connects(k.a)}`);
  }
  const r = await h.hup('after restart');
  h.check('重启后文件与运行配置一致：SIGHUP 报成功', SUCCESS.test(r.text) && !RESTART_SUMMARY.test(r.text), tail(r.text));
});

// 7. 地址待重启期间，订阅列表热更新照常生效，且第二次 SIGHUP 不重复下发
scenario('s4-subs-hot-update-with-pending-url', '订阅列表热更新不受影响（同时有待重启的地址变更）', async (h) => {
  const cfg = await h.defaultCfg();
  await h.start(cfg);
  const next = structuredClone(cfg);
  next.okx.biz = h.url('okxBizB'); // 待重启
  next.okx.ticks = ['ETH-USDT-SWAP']; // 热更：加 ETH、删 BTC
  next.binance.ticks = ['BTCUSDT', 'ETHUSDT']; // 热更：加 ETH
  next.okx.klines = { '1m': ['BTC-USDT-SWAP', 'ETH-USDT-SWAP'] }; // 热更：K 线加 ETH
  h.write(next);
  const m1 = h.up.mark();
  const r1 = await h.hup('#1');
  const okxPub = h.up.since(m1, 'okxPubA', 'frame').map((e) => e.text).join('\n');
  const bnMkt = h.up.since(m1, 'bnMarketA', 'frame').map((e) => e.text).join('\n');
  const okxBiz = h.up.since(m1, 'okxBizA', 'frame').map((e) => e.text).join('\n');
  h.check('OKX 逐笔：新增 ETH 订阅帧已发出', /"op":"subscribe"/.test(okxPub) && /ETH-USDT-SWAP/.test(okxPub), okxPub);
  h.check('OKX 逐笔：BTC 取消订阅帧已发出', /"op":"unsubscribe"/.test(okxPub) && /BTC-USDT-SWAP/.test(okxPub), okxPub);
  h.check('Binance：新增 ethusdt@aggTrade 订阅帧已发出', /SUBSCRIBE/.test(bnMkt) && /ethusdt@aggTrade/.test(bnMkt), bnMkt);
  h.check('OKX K 线：新增 ETH 订阅帧已发出', /"op":"subscribe"/.test(okxBiz) && /ETH-USDT-SWAP/.test(okxBiz), okxBiz);
  h.check('地址变更仍被点名为待重启，不报成功', /okx\.stream_base_url_business/.test(r1.text) && RESTART_SUMMARY.test(r1.text) && !SUCCESS.test(r1.text), tail(r1.text));
  const m2 = h.up.mark();
  const r2 = await h.hup('#2');
  const dup = ['okxPubA', 'bnMarketA', 'okxBizA'].flatMap((l) => h.up.since(m2, l, 'frame')).filter((e) => SUB_FRAME.test(e.text));
  h.check('第二次 SIGHUP 不重复下发订阅帧', dup.length === 0, dup.map((e) => `${e.label}: ${e.text}`).join('\n'));
  h.check('第二次 SIGHUP 仍提示待重启，不报成功', /okx\.stream_base_url_business/.test(r2.text) && RESTART_SUMMARY.test(r2.text) && !SUCCESS.test(r2.text), tail(r2.text));
});

// 8. YAML 读取/解析/校验失败：保留原运行配置并明确报错，修好后可再次重载
scenario('s5-invalid-yaml-keeps-running-config', 'YAML 坏了：保留运行配置并报错，修好后恢复', async (h) => {
  const cfg = await h.defaultCfg();
  await h.start(cfg);
  const goodYaml = renderYaml(cfg);
  const bad = [
    ['语法错误', () => h.writeRaw('connectors: [unclosed\n  binance: {\n')],
    ['校验失败(okx.enabled 但 business 地址为空)', () => {
      const c = structuredClone(cfg);
      c.okx.biz = '';
      h.write(c);
    }],
    ['文件不存在', () => {
      renameSync(join(h.dir, 'config.yaml'), join(h.dir, 'config.yaml.gone'));
    }],
  ];
  for (const [name, breakIt] of bad) {
    const m = h.up.mark();
    breakIt();
    const r = await h.hup(name);
    h.check(`${name}：明确报错`, LOAD_FAILED.test(r.text), tail(r.text));
    h.check(`${name}：声明保留运行配置`, /keeping/i.test(r.text), tail(r.text));
    h.check(`${name}：不报成功`, !SUCCESS.test(r.text), tail(r.text));
    h.check(`${name}：进程仍在运行`, h.runner.alive(), '');
    const subFrames = h.up.since(m, undefined, 'frame').filter((e) => SUB_FRAME.test(e.text));
    h.check(`${name}：没有任何订阅动作`, subFrames.length === 0, subFrames.map((e) => `${e.label}: ${e.text}`).join('\n'));
    if (name === '文件不存在') renameSync(join(h.dir, 'config.yaml.gone'), join(h.dir, 'config.yaml'));
    writeFileSync(join(h.dir, 'config.yaml'), goodYaml);
  }
  // 修好后做一次订阅变更，应正常热更
  const next = structuredClone(cfg);
  next.okx.ticks = ['BTC-USDT-SWAP', 'ETH-USDT-SWAP'];
  h.write(next);
  const m = h.up.mark();
  const r = await h.hup('recover');
  const okxPub = h.up.since(m, 'okxPubA', 'frame').map((e) => e.text).join('\n');
  h.check('恢复后：订阅热更新生效', /"op":"subscribe"/.test(okxPub) && /ETH-USDT-SWAP/.test(okxPub), okxPub);
  h.check('恢复后：报成功', SUCCESS.test(r.text), tail(r.text));
});

// 9. 端口偏移 / 环境变量覆盖：配置文件没改，SIGHUP 不得误报监听地址变更
scenario('s6-port-offset-no-false-positive', '--port-offset：配置没改，SIGHUP 不得误报监听地址变更', async (h) => {
  const cfg = await h.defaultCfg();
  const base = await freePortWithOffset(10);
  const base2 = await freePortWithOffset(10);
  cfg.grpc = `127.0.0.1:${base}`;
  cfg.gateway = `127.0.0.1:${base2}`;
  await h.start(cfg, { args: ['--port-offset', '10'] });
  const r = await h.hup();
  h.check('不误报 listen_address 变更', !/listen_address/.test(r.text), tail(r.text));
  h.check('报成功', SUCCESS.test(r.text), tail(r.text));
});

scenario('s7-env-override-no-false-positive', '环境变量覆盖监听地址：配置没改，SIGHUP 不得误报', async (h) => {
  const cfg = await h.defaultCfg();
  const envPort = await freePort();
  await h.start(cfg, { env: { API_GATEWAY_LISTEN_ADDRESS: `127.0.0.1:${envPort}` } });
  const r = await h.hup();
  h.check('不误报 api_gateway.listen_address 变更', !/listen_address/.test(r.text), tail(r.text));
  h.check('报成功', SUCCESS.test(r.text), tail(r.text));
});

// 10. 连接器未运行（启动时列表为空）：订阅列表变更不得静默吞掉，不得误报成功
scenario('s8-absent-connectors', '连接器未运行时订阅变更：每次 SIGHUP 都明确提示，不报成功', async (h) => {
  const cfg = await h.defaultCfg();
  cfg.okx.klines = {}; // 启动时没有 OKX Business 连接器
  cfg.okx.quotes = []; // 启动时没有 OKX quote 连接器
  await h.start(cfg);
  const next = structuredClone(cfg);
  next.okx.klines = { '1m': ['BTC-USDT-SWAP'] };
  next.okx.quotes = ['BNB-USDT-SWAP'];
  h.write(next);
  for (const n of [1, 2]) {
    const r = await h.hup(`#${n}`);
    h.check(`第${n}次：点名 okx.subscribe_klines 未能应用`, /okx\.subscribe_klines/.test(r.text), tail(r.text));
    h.check(`第${n}次：点名 okx.subscribe_quotes 未能应用`, /okx\.subscribe_quotes/.test(r.text), tail(r.text));
    h.check(`第${n}次：给出"需要重启"，不报成功`, RESTART_SUMMARY.test(r.text) && !SUCCESS.test(r.text), tail(r.text));
  }
});

// 11. log_level：日志过滤器启动时由 RUST_LOG 决定，SIGHUP 不能声称已应用
scenario('s9-log-level-not-claimed-applied', 'log_level 变更：不得声称已应用', async (h) => {
  const cfg = await h.defaultCfg();
  await h.start(cfg);
  const next = structuredClone(cfg);
  next.logLevel = 'error';
  h.write(next);
  const r1 = await h.hup('#1');
  h.check('点名 log_level', /log_level/.test(r1.text), tail(r1.text));
  h.check('不得写"(applied)"', !/log_level[^\n]*\(applied\)/.test(r1.text), tail(r1.text));
  h.check('不报成功', !SUCCESS.test(r1.text), tail(r1.text));
  const r2 = await h.hup('#2');
  h.check('实际日志级别未变（第二次 SIGHUP 的 info 日志仍可见）——仅作事实记录', /received SIGHUP/.test(r2.text), tail(r2.text));
});

// 12. 线上同款日志级别（198 的 RUST_LOG=warn）：info 级的"成功"提示看不到，所以"待重启/失败"必须在 warn 以上可见
scenario('s10-prod-log-level-warn', 'RUST_LOG=warn（线上同款）：待重启/失败提示可见，无变更时安静', async (h) => {
  const cfg = await h.defaultCfg();
  await h.start(cfg, { env: { RUST_LOG: 'warn,md_connector=info' } });
  const loud = (t) => /\s(WARN|ERROR)\s/.test(t);

  const r0 = await h.hup('no change', { quiet: true });
  h.check('无变更：没有 WARN/ERROR 输出', !loud(r0.text), tail(r0.text));

  const next = structuredClone(cfg);
  next.okx.biz = h.url('okxBizB');
  h.write(next);
  for (const n of [1, 2]) {
    const r = await h.hup(`business url #${n}`, { quiet: true });
    h.check(`仅改 OKX Business 地址，第${n}次：WARN 级别可见并点名`, loud(r.text) && /okx\.stream_base_url_business/.test(r.text), r.text || '(无任何输出：静默)');
    h.check(`仅改 OKX Business 地址，第${n}次：WARN 级别给出需要重启的结论`, RESTART_SUMMARY.test(r.text), r.text || '(无任何输出：静默)');
  }

  h.writeRaw('connectors: [unclosed\n');
  const rb = await h.hup('bad yaml', { quiet: true });
  h.check('YAML 解析失败：WARN/ERROR 级别可见', loud(rb.text) && LOAD_FAILED.test(rb.text), rb.text || '(无任何输出：静默)');
});

// ------------------------------------------------------------------ 主流程
if (flag('list')) {
  for (const s of scenarios) console.log(`${s.id}\t${s.title}`);
  process.exit(0);
}

mkdirSync(OUT, { recursive: true });
const selected = scenarios.filter((s) => ONLY.length === 0 || ONLY.includes(s.id));
console.log(`runner=${RUNNER} ${RUNNER === 'docker' ? `image=${IMAGE}` : `bin=${BIN}`} out=${OUT} scenarios=${selected.length}`);

for (const s of selected) {
  console.log(`\n== ${s.id}  ${s.title}`);
  const up = new FakeUpstream();
  for (const label of ['bnMarketA', 'bnMarketB', 'bnPublicA', 'bnPublicB', 'okxPubA', 'okxPubB', 'okxBizA', 'okxBizB']) {
    await up.listen(label);
  }
  const h = new Harness(s.id, s.title, up);
  try {
    await s.fn(h);
  } catch (e) {
    h.check(`场景执行异常: ${e.message}`, false, String(e.stack || e));
  } finally {
    await h.finish();
    await up.close();
  }
}

// ------------------------------------------------------------------ 汇总
const failed = results.filter((r) => !r.ok);
let md = `# SIGHUP 重载端到端结果\n\nrunner=${RUNNER} ${RUNNER === 'docker' ? `image=${IMAGE}` : `bin=${BIN}`}  时间=${new Date().toISOString()}\n\n`;
md += `共 ${results.length} 条断言，通过 ${results.length - failed.length}，失败 ${failed.length}。\n\n| 场景 | 断言 | 结果 |\n|---|---|---|\n`;
for (const r of results) md += `| ${r.scenario} | ${r.name.replace(/\|/g, '\\|')} | ${r.ok ? 'PASS' : '**FAIL**'} |\n`;
if (failed.length) {
  md += '\n## 失败断言的证据（日志/观察摘录）\n';
  for (const r of failed) md += `\n### ${r.scenario} / ${r.name}\n\n\`\`\`\n${r.detail || '(无)'}\n\`\`\`\n`;
}
writeFileSync(join(OUT, 'results.md'), md);
writeFileSync(join(OUT, 'results.json'), JSON.stringify(results, null, 2));
console.log(`\n==== 汇总: ${results.length - failed.length}/${results.length} PASS, ${failed.length} FAIL  -> ${join(OUT, 'results.md')}`);
process.exit(REPORT_ONLY ? 0 : Math.min(failed.length, 125));
