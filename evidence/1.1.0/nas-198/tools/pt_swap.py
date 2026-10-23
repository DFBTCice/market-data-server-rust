#!/usr/bin/env python3
"""通过 Portainer 的 Docker API 在 198 上替换 md-server 容器（只动 endpoint 3 = local = 198 本机）。

  pt_swap.py swap     <new-image> <bak-name>   # 旧容器改名为 <bak-name>（形如 md-server-1.0.0-bak）并停止；用新镜像、按旧容器的运行参数创建同名 md-server 并启动
  pt_swap.py rollback <bak-name>    # 停掉并改名当前 md-server（保留现场），把 <bak-name> 改回 md-server 并启动

安全约束：
  * endpoint 固定为 3，且运行前断言其名称为 "local"（避免误操作 153 / AWS 的 endpoint）；
  * 只允许操作名为 md-server / md-server-*-bak / md-server-*-failed 的容器；
  * 不读写任何密码；JWT 从 /tmp/pt_jwt 读取（权限 600，由调用方事先登录获得）。
"""
import json, sys, time, urllib.request, urllib.error, urllib.parse

P = "http://192.168.1.198:9000/api"
EP = 3
JWT = open("/tmp/pt_jwt").read().strip()
HEALTH = "http://192.168.1.198:8081/health"


def call(method, path, body=None, ok=(200, 201, 204), raw=False):
    url = f"{P}{path}"
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {JWT}")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            txt = r.read()
            code = r.status
    except urllib.error.HTTPError as e:
        txt, code = e.read(), e.code
    if code not in ok:
        raise SystemExit(f"HTTP {code} {method} {path}: {txt[:300]!r}")
    if raw:
        return txt
    return json.loads(txt) if txt else None


D = f"/endpoints/{EP}/docker"


def guard():
    ep = call("GET", f"/endpoints/{EP}")
    assert ep["Name"] == "local", f"endpoint {EP} is {ep['Name']!r}, refuse"
    assert ep["URL"].startswith("unix://"), ep["URL"]


def allowed(name):
    return name == "md-server" or (name.startswith("md-server-") and (name.endswith("-bak") or name.endswith("-failed")))


def health(timeout=2):
    try:
        with urllib.request.urlopen(HEALTH, timeout=timeout) as r:
            return r.status == 200
    except Exception:
        return False


def wait_health(limit=60):
    t0 = time.time()
    while time.time() - t0 < limit:
        if health():
            return time.time() - t0
        time.sleep(0.5)
    return None


def swap(new_image, bak):
    guard()
    assert allowed(bak) and bak != "md-server", bak
    old = call("GET", f"{D}/containers/md-server/json")
    img_tag = new_image.split(":")[-1].replace("/", "_")
    hc, cfg = old["HostConfig"], old["Config"]
    spec = {
        "Image": new_image,
        "Env": cfg["Env"],
        "Cmd": cfg["Cmd"],
        "Entrypoint": cfg.get("Entrypoint"),
        "User": cfg.get("User"),
        "ExposedPorts": cfg["ExposedPorts"],
        "Labels": {**(cfg.get("Labels") or {}), "md-server.version": img_tag, "md-server.replaced": old["Id"][:12]},
        "HostConfig": {
            k: hc[k]
            for k in ("PortBindings", "RestartPolicy", "Memory", "MemorySwap", "NanoCpus", "NetworkMode", "LogConfig")
        },
    }
    json.dump(spec, open("/tmp/new_container_spec.json", "w"), indent=1)
    print("旧容器:", old["Id"][:12], old["Config"]["Image"], "->", old["Image"][:19])
    print("新镜像:", new_image, "| 旧容器改名为", bak)

    t_stop = time.time()
    call("POST", f"{D}/containers/md-server/rename?name={bak}")
    call("POST", f"{D}/containers/{old['Id']}/update", {"RestartPolicy": {"Name": "no"}})
    call("POST", f"{D}/containers/{old['Id']}/stop?t=15")
    created = call("POST", f"{D}/containers/create?name=md-server", spec, ok=(200, 201))
    call("POST", f"{D}/containers/{created['Id']}/start")
    up = wait_health(60)
    print(f"新容器 {created['Id'][:12]} 已启动；health 就绪耗时 {up if up is None else round(up,1)}s；"
          f"从发出停止旧容器到新容器健康共 {round(time.time()-t_stop,1)}s（服务中断窗口）")
    if up is None:
        raise SystemExit("新容器 60s 内未就绪 -> 请立即执行 rollback")
    return created["Id"], bak


def rollback(bak):
    guard()
    assert allowed(bak), bak
    cur = call("GET", f"{D}/containers/md-server/json")
    failed = f"md-server-{int(time.time())}-failed"
    t0 = time.time()
    call("POST", f"{D}/containers/{cur['Id']}/update", {"RestartPolicy": {"Name": "no"}})
    call("POST", f"{D}/containers/{cur['Id']}/stop?t=15")
    call("POST", f"{D}/containers/{cur['Id']}/rename?name={failed}")
    call("POST", f"{D}/containers/{bak}/rename?name=md-server")
    old = call("GET", f"{D}/containers/md-server/json")
    call("POST", f"{D}/containers/{old['Id']}/update", {"RestartPolicy": {"Name": "unless-stopped"}})
    call("POST", f"{D}/containers/{old['Id']}/start")
    up = wait_health(60)
    print(f"回滚完成：现场保留为 {failed}；health 就绪 {up if up is None else round(up,1)}s；总耗时 {round(time.time()-t0,1)}s")


if __name__ == "__main__":
    if len(sys.argv) >= 4 and sys.argv[1] == "swap":
        swap(sys.argv[2], sys.argv[3])
    elif len(sys.argv) >= 3 and sys.argv[1] == "rollback":
        rollback(sys.argv[2])
    else:
        print(__doc__)
        sys.exit(2)
