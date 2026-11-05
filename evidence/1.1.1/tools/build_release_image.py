#!/usr/bin/env python3
"""只在 endpoint 3 构建 md-server:1.1.1；不操作现网容器或 latest。"""
import hashlib
import io
import json
import pathlib
import tarfile

from pt_exp import D, call, guard

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / "evidence/1.1.1/final-artifact"
TAG = "md-server:1.1.1"
BASE = "md-server:1.0.0-rollback"
COMMIT = "a06fe2095e8fc58bff1e038a594b25e7002aff1b"
SHA = "3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d"
binary = (ROOT / "dist/md-server-1.1.1-linux-x86_64").read_bytes()
config = (OUT / "config.final.yaml").read_bytes()
assert hashlib.sha256(binary).hexdigest() == SHA
assert b'log_level: "warn"' in config
guard()
images = call("GET", f"{D}/images/json")
assert not any(TAG in (i.get("RepoTags") or []) for i in images), "目标标签已存在，先核对，不覆盖"
base = call("GET", f"{D}/images/{BASE}/json")
assert base["Id"].startswith("sha256:06907bdf7fd8"), "回滚基础镜像与交接不符"
dockerfile = (f"FROM {BASE}\n"
              "COPY md-server /usr/local/bin/md-server\n"
              "COPY config.yaml /etc/md-server/config.yaml\n"
              f'LABEL org.opencontainers.image.revision="{COMMIT}" '
              f'md-server.binary.sha256="{SHA}"\n').encode()
(OUT / "Dockerfile.release").write_bytes(dockerfile)
buf = io.BytesIO()
with tarfile.open(fileobj=buf, mode="w") as archive:
    for name, data, mode in (("Dockerfile", dockerfile, 0o644), ("md-server", binary, 0o755),
                             ("config.yaml", config, 0o644)):
        entry = tarfile.TarInfo(name)
        entry.size, entry.mode = len(data), mode
        archive.addfile(entry, io.BytesIO(data))
raw = call("POST", f"{D}/build?t={TAG}&rm=1&forcerm=1&pull=0", raw_body=buf.getvalue(),
           ctype="application/x-tar", want_raw=True, timeout=120)
(OUT / "image-build.jsonl").write_bytes(raw)
rows = [json.loads(line) for line in raw.splitlines() if line.strip()]
assert not any(row.get("error") for row in rows), raw.decode()[-1500:]
image = call("GET", f"{D}/images/{TAG}/json")
assert image["Config"]["Labels"]["md-server.binary.sha256"] == SHA
record = {"tag": TAG, "id": image["Id"], "base": BASE, "base_id": base["Id"],
          "source_commit": COMMIT, "binary_sha256": SHA,
          "config_sha256": hashlib.sha256(config).hexdigest(),
          "user": image["Config"].get("User"), "cmd": image["Config"].get("Cmd"),
          "healthcheck": image["Config"].get("Healthcheck")}
(OUT / "image-manifest.json").write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record, indent=2))
