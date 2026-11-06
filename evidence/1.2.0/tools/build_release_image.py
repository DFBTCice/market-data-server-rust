#!/usr/bin/env python3
"""在 198（endpoint 3）构建换版镜像 md-server:1.2.0：md-server:1.0.0-rollback + 最终产物 + config.release.yaml。
不操作现网容器、不动 latest。用法：python3 evidence/1.2.0/tools/build_release_image.py <二进制> <sha256> <源码提交>"""
import hashlib
import io
import json
import pathlib
import sys
import tarfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "evidence/1.1.1/tools"))
from pt_exp import D, call, guard  # noqa: E402

OUT = ROOT / "evidence/1.2.0/nas-198"
TAG = "md-server:1.2.0"
BASE = "md-server:1.0.0-rollback"
binary_path, sha, commit = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
binary = binary_path.read_bytes()
config = (OUT / "config.release.yaml").read_bytes()
assert hashlib.sha256(binary).hexdigest() == sha, "产物 sha 不符"
for needle in (b'log_level: "warn"', b"idle_unsubscribe: true", b"idle_minutes: 10", b"check_interval_seconds: 60",
               b"startup_grace_minutes: 10", b"max_runtime_subscriptions: 200", b"on_demand_quote: false"):
    assert needle in config, f"config.release.yaml 缺少 {needle!r}"
guard()
images = call("GET", f"{D}/images/json")
assert not any(TAG in (i.get("RepoTags") or []) for i in images), "目标标签已存在，先核对，不覆盖"
base = call("GET", f"{D}/images/{BASE}/json")
assert base["Id"].startswith("sha256:06907bdf7fd8"), "回滚基础镜像与交接不符"
dockerfile = (f"FROM {BASE}\n"
              "COPY md-server /usr/local/bin/md-server\n"
              "COPY config.yaml /etc/md-server/config.yaml\n"
              f'LABEL org.opencontainers.image.revision="{commit}" '
              f'md-server.binary.sha256="{sha}" '
              f'md-server.config.sha256="{hashlib.sha256(config).hexdigest()}"\n').encode()
(OUT / "Dockerfile.release").write_bytes(dockerfile)
buf = io.BytesIO()
with tarfile.open(fileobj=buf, mode="w") as archive:
    for name, data, mode in (("Dockerfile", dockerfile, 0o644), ("md-server", binary, 0o755),
                             ("config.yaml", config, 0o644)):
        entry = tarfile.TarInfo(name)
        entry.size, entry.mode = len(data), mode
        archive.addfile(entry, io.BytesIO(data))
raw = call("POST", f"{D}/build?t={TAG}&rm=1&forcerm=1&pull=0", raw_body=buf.getvalue(),
           ctype="application/x-tar", want_raw=True, timeout=300)
(OUT / "image-build.jsonl").write_bytes(raw)
rows = [json.loads(line) for line in raw.splitlines() if line.strip()]
assert not any(row.get("error") for row in rows), raw.decode()[-1500:]
image = call("GET", f"{D}/images/{TAG}/json")
assert image["Config"]["Labels"]["md-server.binary.sha256"] == sha
record = {"tag": TAG, "id": image["Id"], "base": BASE, "base_id": base["Id"], "source_commit": commit,
          "binary_sha256": sha, "config_sha256": hashlib.sha256(config).hexdigest(),
          "user": image["Config"].get("User"), "cmd": image["Config"].get("Cmd"),
          "healthcheck": image["Config"].get("Healthcheck")}
(OUT / "image-manifest.json").write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record, indent=2))
