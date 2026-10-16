#!/usr/bin/env python3
"""198 上"旁路实验容器"管理工具（1.1.1 内存调查）。只通过 Portainer 的 Docker API 操作 endpoint 3（local = 198）。

  pt_exp.py build <binary> <image-tag> [--base md-server:1.1.0-quote-bnb] [--config path/to/config.yaml]
        以 base 镜像为底、仅替换 /usr/local/bin/md-server 二进制（可选替换 /etc/md-server/config.yaml），构建实验镜像（OS 层与现网同款）
  pt_exp.py run <name> <image> <http-port> <grpc-port> [--env K=V ...] [--cpus N]   # 默认 1 核；现网为 2 核
        创建并启动实验容器：内存限额 256MiB（swap 关闭）、CPU 限额 1 核、restart=no、label purpose=md-mem-1.1.1
  pt_exp.py ps
  pt_exp.py stats <name>
  pt_exp.py logs <name> [--tail N]
  pt_exp.py rm <name>            # 只允许删除带 purpose=md-mem-1.1.1 标签的容器
  pt_exp.py rm-all               # 删除全部实验容器
  pt_exp.py rmi <image-tag>      # 只允许删除 md-server:exp-* 镜像

安全约束：endpoint 固定为 3 并断言其名称为 "local"；绝不触碰名为 md-server / md-server-*-bak 等现网/回滚容器；
JWT 只从 /tmp/pt_jwt 读取（权限 600），不读写任何密码。
"""
import io, json, sys, tarfile, time, urllib.request, urllib.error

P = "http://192.168.1.198:9000/api"
EP = 3
D = f"/endpoints/{EP}/docker"
JWT = open("/tmp/pt_jwt").read().strip()
LABEL = "md-mem-1.1.1"
MEM = 256 * 1024 * 1024


def call(method, path, body=None, raw_body=None, ctype="application/json", ok=(200, 201, 204), timeout=120, want_raw=False):
    url = f"{P}{path}"
    data = raw_body if raw_body is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {JWT}")
    if data is not None:
        req.add_header("Content-Type", ctype)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            txt, code = r.read(), r.status
    except urllib.error.HTTPError as e:
        txt, code = e.read(), e.code
    if code not in ok:
        raise SystemExit(f"HTTP {code} {method} {path}: {txt[:400]!r}")
    if want_raw:
        return txt
    return json.loads(txt) if txt else None


def guard():
    ep = call("GET", f"/endpoints/{EP}")
    assert ep["Name"] == "local", f"endpoint {EP} is {ep['Name']!r}, refuse"


def is_exp(name):
    c = call("GET", f"{D}/containers/{name}/json")
    return (c.get("Config", {}).get("Labels") or {}).get("purpose") == LABEL


def cmd_build(binary, tag, base="md-server:1.1.0-quote-bnb", config=None):
    assert tag.startswith("md-server:exp-"), "实验镜像标签必须形如 md-server:exp-*"
    cfg_line = "COPY config.yaml /etc/md-server/config.yaml\n" if config else ""
    dockerfile = (
        f"FROM {base}\nUSER root\nCOPY md-server /usr/local/bin/md-server\n{cfg_line}"
        "RUN chmod +x /usr/local/bin/md-server\nUSER md-user\n"
    ).encode()
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as t:
        ti = tarfile.TarInfo("Dockerfile"); ti.size = len(dockerfile); t.addfile(ti, io.BytesIO(dockerfile))
        bin_bytes = open(binary, "rb").read()
        ti = tarfile.TarInfo("md-server"); ti.size = len(bin_bytes); ti.mode = 0o755; t.addfile(ti, io.BytesIO(bin_bytes))
        if config:
            cfg = open(config, "rb").read()
            ti = tarfile.TarInfo("config.yaml"); ti.size = len(cfg); t.addfile(ti, io.BytesIO(cfg))
    out = call("POST", f"{D}/build?t={tag}&rm=1&forcerm=1", raw_body=buf.getvalue(), ctype="application/x-tar", want_raw=True, timeout=600)
    text = out.decode("utf8", "replace")
    if "Successfully tagged" not in text:
        raise SystemExit("build failed:\n" + text[-1500:])
    print("built", tag)


def cmd_run(name, image, hport, gport, envs, cpus=1.0):
    assert name.startswith("md-exp-"), "实验容器名必须以 md-exp- 开头"
    body = {
        "Image": image,
        "Env": ["RUST_LOG=warn", "TZ=Asia/Shanghai"] + envs,
        "ExposedPorts": {"8081/tcp": {}, "50051/tcp": {}},
        "Labels": {"purpose": LABEL, "variant": image},
        "HostConfig": {
            "PortBindings": {"8081/tcp": [{"HostPort": str(hport)}], "50051/tcp": [{"HostPort": str(gport)}]},
            "RestartPolicy": {"Name": "no"},
            "Memory": MEM, "MemorySwap": MEM, "NanoCpus": int(cpus * 1_000_000_000),
            "LogConfig": {"Type": "json-file", "Config": {"max-size": "20m", "max-file": "3"}},
        },
    }
    r = call("POST", f"{D}/containers/create?name={name}", body)
    cid = r["Id"]
    call("POST", f"{D}/containers/{cid}/start")
    print("started", name, cid[:12], image, f"http={hport}", f"cpus={cpus}", envs)


def demux(raw):
    import struct
    out, i = [], 0
    while i + 8 <= len(raw):
        _, _, _, _, n = struct.unpack(">BBBBI", raw[i:i + 8]); i += 8
        out.append(raw[i:i + n].decode("utf8", "replace")); i += n
    return "".join(out)


def main():
    guard()
    a = sys.argv[1:]
    if not a:
        raise SystemExit(__doc__)
    c = a[0]
    if c == "build":
        base = a[a.index("--base") + 1] if "--base" in a else "md-server:1.1.0-quote-bnb"
        cfg = a[a.index("--config") + 1] if "--config" in a else None
        cmd_build(a[1], a[2], base, cfg)
    elif c == "run":
        envs = [a[i + 1] for i, x in enumerate(a) if x == "--env"]
        cpus = float(a[a.index("--cpus") + 1]) if "--cpus" in a else 1.0
        cmd_run(a[1], a[2], a[3], a[4], envs, cpus)
    elif c == "ps":
        for x in call("GET", f"{D}/containers/json?all=1&filters=" + urllib.request.quote(json.dumps({"label": [f"purpose={LABEL}"]}))):
            print(x["Names"][0], x["Image"], x["State"], x["Status"])
    elif c == "stats":
        s = call("GET", f"{D}/containers/{a[1]}/stats?stream=false", timeout=30)
        mem = s["memory_stats"]; print(a[1], "mem_usage_MiB=%.1f limit_MiB=%.0f" % (mem["usage"] / 2**20, mem["limit"] / 2**20))
    elif c == "logs":
        tail = a[a.index("--tail") + 1] if "--tail" in a else "200"
        raw = call("GET", f"{D}/containers/{a[1]}/logs?stdout=1&stderr=1&timestamps=1&tail={tail}", want_raw=True)
        import re
        print(re.sub(r"\x1b\[[0-9;]*m", "", demux(raw)))
    elif c == "rm":
        assert is_exp(a[1]), f"{a[1]} 不是实验容器，拒绝"
        call("DELETE", f"{D}/containers/{a[1]}?force=1&v=1"); print("removed", a[1])
    elif c == "rm-all":
        for x in call("GET", f"{D}/containers/json?all=1&filters=" + urllib.request.quote(json.dumps({"label": [f"purpose={LABEL}"]}))):
            n = x["Names"][0].lstrip("/")
            call("DELETE", f"{D}/containers/{n}?force=1&v=1"); print("removed", n)
    elif c == "rmi":
        assert a[1].startswith("md-server:exp-")
        call("DELETE", f"{D}/images/{a[1]}"); print("removed image", a[1])
    else:
        raise SystemExit(__doc__)


main()
