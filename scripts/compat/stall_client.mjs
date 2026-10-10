#!/usr/bin/env node
// "停读"消费者（1.1.1 内存增长调查：慢/半死客户端对 broadcast 环的滞留效应）。
//
// 用裸 TCP 完成 WebSocket 握手并订阅一批主题，然后 **不再读取 socket**（socket.pause()），
// 模拟"进程还活着但事件循环卡死 / 进程被挂起 / 网络半开"的客户端：
//   - 内核接收缓冲塞满 → TCP 窗口为 0 → 服务端 `sender.send().await` 永远阻塞；
//   - 服务端该连接的处理循环停在 send 上，不再 poll broadcast Receiver，
//     于是各主题的 broadcast 环里这批事件无法被"最后一个接收者"取走，滞留（最多每主题 4096 条）。
// 持续 --hold 秒后关闭连接（--hold 0 表示永不关闭，直到脚本被杀）。
//
// 用法：node scripts/compat/stall_client.mjs <host:port> <subs.json> --hold 1800 [--log file] [--no-quote] [--rcvbuf 4096]
import net from 'node:net';
import crypto from 'node:crypto';
import { readFileSync, appendFileSync, writeFileSync } from 'node:fs';

const [, , hostport, subsFile, ...rest] = process.argv;
if (!hostport || !subsFile) {
  console.error('usage: stall_client.mjs <host:port> <subs.json> [--hold sec] [--log file] [--no-quote]');
  process.exit(2);
}
const opt = (n, d) => {
  const i = rest.indexOf(n);
  return i >= 0 ? rest[i + 1] : d;
};
const HOLD = Number(opt('--hold', 1800)) * 1000;
const LOG = opt('--log', null);
const WITH_QUOTE = !rest.includes('--no-quote');
const [host, port] = hostport.split(':');

const log = (m) => {
  const line = `${new Date().toISOString()} ${m}`;
  console.log(line);
  if (LOG) appendFileSync(LOG, line + '\n');
};
if (LOG) writeFileSync(LOG, '');

const subs = JSON.parse(readFileSync(subsFile, 'utf8'));
const topicOf = (s) =>
  s.data_type === 1
    ? `tick.${s.exchange}.${s.symbol}`
    : s.data_type === 2
      ? `kline.${s.kline_interval}.${s.exchange}.${s.symbol}`
      : `quote.${s.exchange}.${s.symbol}`;
const streams = subs.filter((s) => WITH_QUOTE || s.data_type !== 3).map(topicOf);

function maskedTextFrame(text) {
  const payload = Buffer.from(text, 'utf8');
  const mask = crypto.randomBytes(4);
  let head;
  if (payload.length < 126) head = Buffer.from([0x81, 0x80 | payload.length]);
  else head = Buffer.from([0x81, 0x80 | 126, payload.length >> 8, payload.length & 0xff]);
  const masked = Buffer.alloc(payload.length);
  for (let i = 0; i < payload.length; i++) masked[i] = payload[i] ^ mask[i % 4];
  return Buffer.concat([head, mask, masked]);
}

const sock = net.createConnection({ host, port: Number(port) });
let handshaken = false;
let bytes = 0;
sock.on('connect', () => {
  const key = crypto.randomBytes(16).toString('base64');
  sock.write(
    `GET /ws/v1/data HTTP/1.1\r\nHost: ${host}:${port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
      `Sec-WebSocket-Key: ${key}\r\nSec-WebSocket-Version: 13\r\n\r\n`,
  );
});
sock.on('data', (buf) => {
  bytes += buf.length;
  if (!handshaken) {
    handshaken = true;
    log(`handshake ok (${buf.toString('utf8', 0, 20).trim()}), subscribing ${streams.length} streams, then STOP READING for ${HOLD / 1000}s`);
    sock.write(maskedTextFrame(JSON.stringify({ action: 'subscribe', streams })));
    // 收到握手响应与订阅 ack 后立即停读
    setTimeout(() => {
      sock.pause();
      log(`socket paused; bytes read so far=${bytes}`);
    }, 1500);
  }
});
sock.on('close', () => {
  log('socket closed');
  process.exit(0);
});
sock.on('error', (e) => log(`socket error ${e.code ?? e.message}`));

const tick = setInterval(() => log(`holding... bytes read=${bytes} ${sock.destroyed ? 'destroyed' : 'open'}`), 60_000);
if (HOLD > 0) {
  setTimeout(() => {
    log('hold finished -> closing connection');
    clearInterval(tick);
    sock.destroy();
  }, HOLD);
}
