#!/usr/bin/env python3
"""只读：周期读取容器 cgroup 内存统计（Docker stats API，仅 GET），关注 THP 相关计数。

背景（1.1.1 调查 D1/D4）：现网 md-server 的 cgroup 统计显示 anon_thp=30MiB、thp_collapse_alloc=15
（旁路容器均为 0）。现网 RSS 曾在 1 分钟内跳升 +23.8MiB（VSZ 不变、吞吐无异常），怀疑与内核 khugepaged
把稀疏堆区折叠成 2MiB 大页有关。本脚本用来看"RSS 跳升的那一刻 thp_collapse_alloc 是否同时增加"。

用法：python3 thp_sampler.py <out.csv> <seconds> <interval-seconds> <container> [<container> ...]
只对指定名称的容器发 GET；JWT 从 /tmp/pt_jwt 读取（不读写任何密码）。
"""
import csv, json, sys, time, urllib.request
from datetime import datetime, timezone

out, secs, every = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
names = sys.argv[4:]
jwt = open("/tmp/pt_jwt").read().strip()
BASE = "http://192.168.1.198:9000/api/endpoints/3/docker"
COLS = ["ts_utc", "container", "usage", "anon", "anon_thp", "file", "thp_fault_alloc", "thp_collapse_alloc", "pgfault", "pgmajfault"]


def stats(name):
    req = urllib.request.Request(f"{BASE}/containers/{name}/stats?stream=false&one-shot=true", headers={"Authorization": "Bearer " + jwt})
    s = json.load(urllib.request.urlopen(req, timeout=20))
    m = s["memory_stats"]
    st = m.get("stats", {})
    return [m.get("usage", 0), st.get("anon", 0), st.get("anon_thp", 0), st.get("file", 0), st.get("thp_fault_alloc", 0),
            st.get("thp_collapse_alloc", 0), st.get("pgfault", 0), st.get("pgmajfault", 0)]


with open(out, "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(COLS)
    f.flush()
    end = time.time() + secs
    while time.time() < end:
        t0 = time.time()
        now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        for n in names:
            try:
                w.writerow([now, n] + stats(n))
            except Exception as e:  # 单次失败不终止
                w.writerow([now, n, "ERR", str(e)[:60], "", "", "", "", "", ""])
        f.flush()
        time.sleep(max(0.0, every - (time.time() - t0)))
