#!/usr/bin/env python3
"""Export full-window telemetry; retain all steps, drops and connection excursions."""
import csv
import datetime as dt
import json
import os
import pathlib

os.environ.setdefault("MPLCONFIGDIR", "/tmp/md-111-matplotlib")
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = ROOT / "evidence/1.1.1/nas-198/post-swap-24h"
with (BASE / "prod.csv").open() as source:
    rows = list(csv.DictReader(source))
analysis = json.loads((BASE / "analysis.json").read_text())
stamp = lambda s: dt.datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()
x = [(stamp(row["ts_utc"]) - stamp(rows[0]["ts_utc"])) / 3600 for row in rows]
fig, axes = plt.subplots(4, 1, figsize=(11, 10), sharex=True, layout="constrained")
fig.suptitle("md-server 1.1.1 | Complete 24h observation | Overall acceptance: NOT PASSED", fontsize=13)
axes[0].plot(x, [float(row["rss_bytes"]) / 2**20 for row in rows], color="#245ea8", linewidth=1, label="Process RSS (all samples)")
warm_x = [value for value in x if value >= 1]
rss = analysis["rss_post_warmup"]
axes[0].plot(warm_x, [rss["intercept_mib"] + rss["slope_mib_h"] * (value - warm_x[0]) for value in warm_x],
             color="#bc6400", linestyle="--", linewidth=1.3, label=f"Post-warmup OLS: +{rss['slope_mib_h']:.4f} MiB/h")
axes[0].set_ylabel("RSS (MiB)")
axes[0].legend(loc="upper left", fontsize=8)
axes[1].plot(x, [float(row["jemalloc_allocated"]) / 2**20 for row in rows], color="#20805b", linewidth=0.9)
axes[1].set_ylabel("Allocated (MiB)")
axes[1].set_title("Post-warmup allocated: 4.189-4.361 MiB; nearly flat", fontsize=9, loc="left")
axes[2].step(x, [float(row["ticks_dropped"]) for row in rows], where="post", color="#ba3b3b", linewidth=1.2, label="Ingress ticks dropped")
axes[2].set_ylabel("Ticks dropped")
timeout_axis = axes[2].twinx()
timeout_axis.step(x, [float(row["ws_kicked_write_timeout"]) for row in rows], where="post", color="#7c4aa3", linestyle="--", label="WS write timeouts")
timeout_axis.set_ylabel("Write timeouts")
timeout_axis.set_yticks([0, 1])
axes[2].legend(loc="upper left", fontsize=8)
timeout_axis.legend(loc="upper right", fontsize=8)
axes[3].step(x, [float(row["ws_clients"]) for row in rows], where="post", color="#245ea8", linewidth=1, label="WS clients")
axes[3].set_ylabel("WS clients")
axes[3].set_yticks([1, 2, 3, 4])
axes[3].legend(loc="upper left", fontsize=8)
axes[3].set_xlabel("Hours since 2026-10-23 19:54:13 UTC (Beijing 2026-10-24 03:54:13)")
for axis in axes:
    axis.axvspan(0, 1, color="#aaaaaa", alpha=0.18)
    axis.grid(alpha=0.2)
    axis.set_xlim(0, x[-1])
axes[3].set_xticks(list(range(0, 25, 2)))
fig.savefig(BASE / "observation-24h.png", dpi=160)
fig.savefig(BASE / "observation-24h.svg")
svg = BASE / "observation-24h.svg"
svg.write_text("\n".join(line.rstrip() for line in svg.read_text().splitlines()) + "\n")
print("Exported full-window PNG and SVG")
