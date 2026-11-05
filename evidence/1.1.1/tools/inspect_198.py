#!/usr/bin/env python3
"""只读导出现网配置与 2026-10-21 重启窗口；不输出凭据或其它容器资料。"""
import difflib
import datetime
import io
import json
import pathlib
import tarfile
import urllib.parse

from pt_exp import D, call, demux, guard

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "evidence/1.1.1/final-artifact/baseline"
OUT.mkdir(parents=True, exist_ok=True)
guard()
raw = call("GET", f"{D}/containers/md-server/archive?path=/etc/md-server/config.yaml", want_raw=True)
with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
    member = next(m for m in archive.getmembers() if m.isfile() and m.name.endswith("config.yaml"))
    config = archive.extractfile(member).read().decode()
(OUT / "config.running.yaml").write_text(config)
assert config.count('log_level: "info"') == 1, "核对实际 log_level 后再构建镜像"
final = config.replace('log_level: "info"', 'log_level: "warn"')
(OUT.parent / "config.final.yaml").write_text(final)
diff = "".join(difflib.unified_diff(config.splitlines(True), final.splitlines(True),
                                    fromfile="config.running.yaml", tofile="config.final.yaml"))
(OUT.parent / "config.diff").write_text(diff)
print(diff)
container = call("GET", f"{D}/containers/md-server/json")
filters = urllib.parse.quote(json.dumps({"container": [container["Id"]]}))
epoch = lambda text: int(datetime.datetime.fromisoformat(text.replace("Z", "+00:00")).timestamp())
window = f"since={epoch('2026-10-21T13:32:00Z')}&until={epoch('2026-10-21T13:42:00Z')}"
events = call("GET", f"{D}/events?{window}&filters={filters}", want_raw=True, timeout=30)
(OUT / "restart-20261021.events.jsonl").write_bytes(events)
logs = call("GET", f"{D}/containers/md-server/logs?stdout=1&stderr=1&timestamps=1&{window}",
            want_raw=True, timeout=30)
(OUT / "restart-20261021.container.log").write_text(demux(logs))
summary = {"RestartCount": container["RestartCount"], "State.OOMKilled": container["State"]["OOMKilled"],
           "State.StartedAt": container["State"]["StartedAt"],
           "State.FinishedAt": container["State"]["FinishedAt"],
           "event_lines": len(events.splitlines()),
           "note": "当前进程 OOMKilled=false 不能排除前次 OOM；历史事件无证据时不能确认重启原因。"}
(OUT / "restart-20261021.summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(summary, ensure_ascii=False))
