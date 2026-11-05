// fake_upstream.mjs -- 零依赖的假"交易所 WebSocket 上游"（只用 Node 内置模块）
//
// 给 SIGHUP 重载端到端测试用：每个 label 一个监听端口，记录
//   - 谁连上来了（connect）、断开了（close）
//   - 收到了什么文本帧（frame，例如 OKX 的 {"op":"subscribe",...}、币安的 {"method":"SUBSCRIBE",...}）
// 并能主动掐断某个 label 的全部连接（dropAll），用来触发被测进程的自动重连。
//
// 只实现被测连接器需要的最小 WebSocket 服务端：握手、客户端帧（掩码）解析、ping/pong、close。
// OKX 的应用层心跳是文本 "ping"，这里回文本 "pong"，避免被测进程判定 pong 超时而重连。

import http from 'node:http';
import crypto from 'node:crypto';

const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';

/** 编码一个服务端帧（服务端发出的帧不加掩码） */
function encodeFrame(opcode, payload = Buffer.alloc(0)) {
  const len = payload.length;
  let header;
  if (len < 126) {
    header = Buffer.from([0x80 | opcode, len]);
  } else if (len < 65536) {
    header = Buffer.alloc(4);
    header[0] = 0x80 | opcode;
    header[1] = 126;
    header.writeUInt16BE(len, 2);
  } else {
    header = Buffer.alloc(10);
    header[0] = 0x80 | opcode;
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(len), 2);
  }
  return Buffer.concat([header, payload]);
}

export class FakeUpstream {
  constructor() {
    /** @type {Map<string, {server: http.Server, port: number, sockets: Set<import('node:net').Socket>}>} */
    this.servers = new Map();
    /** 按时间顺序的全部事件：{ i, t, label, type: 'connect'|'frame'|'close', path?, text? } */
    this.events = [];
  }

  /** 事件游标：之后用 since(mark) 取"此后新增"的事件 */
  mark() {
    return this.events.length;
  }

  since(mark, label, type) {
    return this.events
      .slice(mark)
      .filter((e) => (!label || e.label === label) && (!type || e.type === type));
  }

  connects(label) {
    return this.events.filter((e) => e.label === label && e.type === 'connect').length;
  }

  /** 当前仍连着的连接数 */
  active(label) {
    const s = this.servers.get(label);
    return s ? s.sockets.size : 0;
  }

  port(label) {
    return this.servers.get(label).port;
  }

  /** 在 0.0.0.0 上监听一个临时端口，返回端口号 */
  listen(label) {
    return new Promise((resolve, reject) => {
      const sockets = new Set();
      const server = http.createServer((req, res) => {
        res.writeHead(426, { 'content-type': 'text/plain' });
        res.end('websocket only');
      });
      server.on('upgrade', (req, socket) => this.#onUpgrade(label, sockets, req, socket));
      server.once('error', reject);
      server.listen(0, '0.0.0.0', () => {
        const port = server.address().port;
        this.servers.set(label, { server, port, sockets });
        resolve(port);
      });
    });
  }

  /** 掐断某个 label 的全部连接（模拟上游断线，被测进程应触发重连） */
  dropAll(label) {
    const s = this.servers.get(label);
    if (!s) return 0;
    const n = s.sockets.size;
    for (const sock of [...s.sockets]) sock.destroy();
    return n;
  }

  async close() {
    for (const { server, sockets } of this.servers.values()) {
      for (const sock of sockets) sock.destroy();
      await new Promise((r) => server.close(() => r()));
    }
    this.servers.clear();
  }

  #push(label, type, extra = {}) {
    this.events.push({ i: this.events.length, t: Date.now(), label, type, ...extra });
  }

  #onUpgrade(label, sockets, req, socket) {
    const key = req.headers['sec-websocket-key'];
    if (!key) {
      socket.destroy();
      return;
    }
    const accept = crypto.createHash('sha1').update(key + GUID).digest('base64');
    socket.write(
      'HTTP/1.1 101 Switching Protocols\r\n' +
        'Upgrade: websocket\r\n' +
        'Connection: Upgrade\r\n' +
        `Sec-WebSocket-Accept: ${accept}\r\n\r\n`,
    );
    sockets.add(socket);
    this.#push(label, 'connect', { path: req.url });
    socket.on('error', () => {});
    socket.on('close', () => {
      sockets.delete(socket);
      this.#push(label, 'close');
    });

    let buf = Buffer.alloc(0);
    let fragments = null; // 分片文本消息的累积
    const handle = (opcode, fin, payload) => {
      switch (opcode) {
        case 0x1: // text
        case 0x0: {
          // continuation
          if (opcode === 0x1) fragments = [];
          if (fragments) fragments.push(payload);
          if (fin && fragments) {
            const text = Buffer.concat(fragments).toString('utf8');
            fragments = null;
            this.#push(label, 'frame', { text });
            if (text === 'ping') socket.write(encodeFrame(0x1, Buffer.from('pong')));
          }
          break;
        }
        case 0x9: // ping -> pong
          socket.write(encodeFrame(0xa, payload));
          break;
        case 0x8: // close
          socket.write(encodeFrame(0x8, payload));
          socket.end();
          break;
        default:
          break; // pong / binary: 忽略
      }
    };
    socket.on('data', (chunk) => {
      buf = Buffer.concat([buf, chunk]);
      for (;;) {
        if (buf.length < 2) return;
        const fin = (buf[0] & 0x80) !== 0;
        const opcode = buf[0] & 0x0f;
        const masked = (buf[1] & 0x80) !== 0;
        let len = buf[1] & 0x7f;
        let off = 2;
        if (len === 126) {
          if (buf.length < 4) return;
          len = buf.readUInt16BE(2);
          off = 4;
        } else if (len === 127) {
          if (buf.length < 10) return;
          len = Number(buf.readBigUInt64BE(2));
          off = 10;
        }
        let mask = null;
        if (masked) {
          if (buf.length < off + 4) return;
          mask = buf.subarray(off, off + 4);
          off += 4;
        }
        if (buf.length < off + len) return;
        const payload = Buffer.from(buf.subarray(off, off + len));
        if (mask) for (let i = 0; i < payload.length; i++) payload[i] ^= mask[i & 3];
        buf = buf.subarray(off + len);
        handle(opcode, fin, payload);
      }
    });
  }
}
