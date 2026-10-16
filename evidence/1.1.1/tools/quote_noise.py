#!/usr/bin/env python3
"""量化 compare.mjs 对 quote.binance 的"逐字节"项噪声：两条独立连接/两个独立进程之间，同一键 (E,bid,ask) 的数量字段会不同。
对每一组 (A 侧, B 侧) 原始抓取：
  - 用与 compare.mjs 相同的键 exchange|symbol|timestamp|bid_price|ask_price 做 join（去掉 connector_receive_ts）；
  - 统计 对应条数 / 不同条数 / 不同字段直方图 / 各侧"同键多条"的组数；
  - 额外检查：不同的记录里，价格与 timestamp 是否一致（键内字段，必然一致），差异是否只在 bid_qty/ask_qty。
用法: quote_noise.py <label>=<A_dir>:<B_dir> ...   (目录里需有 quote.*.raw.jsonl)
"""
import json, sys, os, glob, collections

def load(path):
    out = []
    for l in open(path):
        l = l.strip()
        if not l: continue
        d = json.loads(l)['data']; out.append(d)
    return out

def key(d): return (d['exchange'], d['symbol'], d['timestamp'], d['bid_price'], d['ask_price'])
def strip(d): return {k: v for k, v in d.items() if k != 'connector_receive_ts'}

def analyze(a_dir, b_dir):
    rows = []
    for fa in sorted(glob.glob(os.path.join(a_dir, 'quote.*.raw.jsonl'))):
        topic = os.path.basename(fa)[:-len('.raw.jsonl')]
        fb = os.path.join(b_dir, os.path.basename(fa))
        if not os.path.exists(fb): continue
        A, B = load(fa), load(fb)
        am, bm = {}, {}
        for d in A: am[key(d)] = strip(d)      # 与 compare.mjs 一致：同键后到者覆盖
        for d in B: bm[key(d)] = strip(d)
        joined = [k for k in am if k in bm]
        diff = [k for k in joined if am[k] != bm[k]]
        fields = collections.Counter()
        for k in diff:
            for f in am[k]:
                if am[k][f] != bm[k].get(f): fields[f] += 1
        dupa = len(A) - len(am); dupb = len(B) - len(bm)
        rows.append((topic, len(A), len(B), len(joined), len(diff), dict(fields), dupa, dupb))
    return rows

tot_j = tot_d = 0
for arg in sys.argv[1:]:
    label, pair = arg.split('=', 1)
    a, b = pair.split(':', 1)
    print(f'\n### {label}')
    for topic, na, nb, j, d, f, dupa, dupb in analyze(a, b):
        print(f'  {topic:28s} A={na:5d} B={nb:5d} 对应={j:5d} 不同={d:2d} ({(100.0*d/j if j else 0):.2f}%) 不同字段={f} 同键多条(A侧/B侧 多出条数)={dupa}/{dupb}')
