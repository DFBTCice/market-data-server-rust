#!/usr/bin/env python3
"""按原负载的粗键审计已有 240s 原文；不把不同 payload 的碰撞算作重复投递。"""
import collections
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[3]
CAP = ROOT / "evidence/1.1.1/final-artifact/canary"
OUT = ROOT / "evidence/1.1.1/final-artifact/anomalies/captured-key-audit.json"


def key(o):
    d, topic = o.get("data", {}), o.get("topic", "")
    if o.get("type") == "tick":
        return topic, d.get("trade_id")
    if o.get("type") == "kline":
        return topic, *(d.get(k) for k in ("open_time", "volume", "number_of_trades", "close", "exchange_event_ts"))
    return topic, *(d.get(k) for k in ("timestamp", "bid_price", "ask_price"))


result = {}
for side in ("old", "new"):
    rows = []
    for path in sorted((CAP / f"{side}-gateway").glob("*.raw.jsonl")):
        if path.name.startswith("_"):
            continue
        seen, samples = {}, {}
        counts = collections.Counter()
        total = 0
        for line in path.read_text().splitlines():
            o = json.loads(line)
            k = key(o)
            if k in seen:
                previous = seen[k]
                fields = sorted(f for f in set(previous["data"]) | set(o["data"])
                                if previous["data"].get(f) != o["data"].get(f))
                reason = ",".join(fields) or "identical_payload_and_ingest"
                counts[reason] += 1
                samples.setdefault(reason, {"before": previous, "after": o})
            seen[k] = o
            total += 1
        if counts:
            rows.append({"file": str(path.relative_to(CAP)), "messages": total,
                         "coarse_key_duplicates": sum(counts.values()), "changed_fields": dict(counts),
                         "samples": samples})
    result[side] = rows
OUT.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
for side, rows in result.items():
    for row in rows:
        print(side, row["file"], row["coarse_key_duplicates"], row["changed_fields"])
