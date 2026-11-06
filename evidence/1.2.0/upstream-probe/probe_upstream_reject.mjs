// 只读探测：交易所对不存在币对 / 错误 K 线级别的订阅如何应答（公共行情，无鉴权）
const cases = [
  { name: 'okx-public', url: 'wss://ws.okx.com:8443/ws/v5/public', msgs: [
    { op: 'subscribe', args: [{ channel: 'trades', instId: 'NOSUCH-USDT-SWAP' }] },
    { op: 'subscribe', args: [{ channel: 'tickers', instId: 'NOSUCH-USDT-SWAP' }] },
    { op: 'subscribe', args: [{ channel: 'trades', instId: 'BTC-USDT-SWAP' }, { channel: 'trades', instId: 'NOSUCH2-USDT-SWAP' }] },
  ]},
  { name: 'okx-business', url: 'wss://ws.okx.com:8443/ws/v5/business', msgs: [
    { op: 'subscribe', args: [{ channel: 'candle7m', instId: 'BTC-USDT-SWAP' }] },
    { op: 'subscribe', args: [{ channel: 'candle1m', instId: 'NOSUCH-USDT-SWAP' }] },
  ]},
  { name: 'binance-market', url: 'wss://fstream.binance.com/market/stream', msgs: [
    { method: 'SUBSCRIBE', params: ['nosuchusdt@aggTrade'], id: 1 },
    { method: 'SUBSCRIBE', params: ['btcusdt@kline_7m'], id: 2 },
  ]},
];
for (const c of cases) {
  await new Promise((resolve) => {
    const ws = new WebSocket(c.url);
    const out = [];
    const t = setTimeout(() => { ws.close(); console.log(JSON.stringify({ case: c.name, frames: out }, null, 1)); resolve(); }, 6000);
    ws.onopen = () => c.msgs.forEach((m, i) => setTimeout(() => ws.send(JSON.stringify(m)), 300 * i));
    ws.onmessage = (e) => { const s = String(e.data); if (!s.includes('"data"')) out.push(s.slice(0, 400)); };
    ws.onerror = (e) => { out.push('ERROR ' + (e.message || 'ws error')); };
    ws.onclose = (e) => { out.push(`CLOSE ${e.code}`); };
  });
}
