#!/usr/bin/env python3
"""变异验证（mutation testing）：证明测试真的能抓住回归，而不是"怎么改都是绿的"。

做法：每次只改一处源码 -> 跑测试 -> 立刻还原；全部跑完后核对文件与备份逐字节一致。

- A 类：单元测试应当抓住的变异（`cargo test -p <crate>`）。
- B 类：日志过滤器的"接线"类变异。全局 tracing subscriber 在一个进程里只能安装一次，
  单元测试覆盖不到 `logging::init` 与 `main` 的接线，所以由真实进程 + 真实 SIGHUP 的端到端场景来抓；
  同时如实记录"单元测试对这类变异仍然是绿的"。

用法（仓库根目录或任意目录均可）：
    python3 evidence/sighup-reload-fix/verify/run_mutations.py [输出目录] [--allow-dirty]
默认要求这几个源文件在 git 里没有未提交修改（结束时再核对 git diff 为空）；--allow-dirty 只在调试脚本时用。
输出：<输出目录>/mutation-tests.md、mutation-tests.full.log。退出码：没被抓住的变异个数（0 = 全部抓住）。
"""
import atexit
import datetime
import filecmp
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
os.chdir(ROOT)

args = [a for a in sys.argv[1:] if not a.startswith("--")]
ALLOW_DIRTY = "--allow-dirty" in sys.argv
OUT = os.path.abspath(args[0]) if args else os.path.join(ROOT, "evidence", "sighup-reload-fix", "verify")
os.makedirs(OUT, exist_ok=True)

RELOAD = "crates/md-server/src/reload.rs"
MAIN = "crates/md-server/src/main.rs"
LOGGING = "crates/md-server/src/logging.rs"
CONFIG = "crates/md-config/src/lib.rs"
FILES = [RELOAD, MAIN, LOGGING, CONFIG]

BAK_DIR = tempfile.mkdtemp(prefix="mut-bak-")
BAK = {f: os.path.join(BAK_DIR, f.replace("/", "__")) for f in FILES}


def restore_all():
    for f in FILES:
        if os.path.exists(BAK[f]):
            shutil.copyfile(BAK[f], f)


def sh(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


# --------------------------------------------------------------------------
# 变异清单：(编号, 标题, 文件, 原文, 替换为, 被测 crate)
# 每条的"原文"必须在文件里恰好出现一次，否则直接报错（防止变异悄悄没改到东西）。
# --------------------------------------------------------------------------

OKX_BIZ_ENTRY = (
    "        (\n"
    '            "connectors.okx.stream_base_url_business",\n'
    "            show(okx_stream_url_business),\n"
    "        ),\n"
)

A_MUTATIONS = [
    # ---- SIGHUP 配置重载（地址类字段 / 订阅热更）----
    ("M1", "把文件配置整体记为已应用（即原 `current_cfg = new_cfg;`）", RELOAD,
     "        self.applied = next;\n        report\n",
     "        self.applied = file;\n        report\n", "md-server"),
    ("M2", "漏掉 OKX Business 地址检测", RELOAD, OKX_BIZ_ENTRY, "", "md-server"),
    ("M3", "连接器缺失时仍视为已应用（静默吞掉）", RELOAD,
     "        return GroupOutcome::NotRunning;", "        return GroupOutcome::Applied;", "md-server"),
    ("M4", "订阅下发失败仍视为已应用（报成功、不重试）", RELOAD,
     "        Some(e) => GroupOutcome::Failed(e),", "        Some(_e) => GroupOutcome::Applied,", "md-server"),
    ("M5", "订阅热更成功后不推进已应用列表（每次 SIGHUP 重复下发）", RELOAD,
     "GroupOutcome::Applied => group.adopt(&mut next, &file),", "GroupOutcome::Applied => {}", "md-server"),
    ("M6", "启动接线错误：OKX Business 连接器被接到 Public 地址", MAIN,
     "            stream_base_url_business: cfg.connectors.okx.stream_base_url_business.clone(),",
     "            stream_base_url_business: cfg.connectors.okx.stream_base_url_public.clone(),", "md-server"),
    # ---- log_level：SIGHUP 热更逻辑（reload.rs）----
    ("L1", "RUST_LOG 覆盖时仍把文件里的 log_level 记为已应用（提示一次就消失）", RELOAD,
     "                Some(rust_log) => {\n                    log_pending = Some(PendingChange {",
     "                Some(rust_log) => {\n                    next.log_level = file.log_level.clone();\n"
     "                    log_pending = Some(PendingChange {", "md-server"),
    ("L2", "log_level 变更时不真正切换过滤器，却记为已应用", RELOAD,
     "                    match self.log.apply(&file.log_level) {",
     "                    match LevelOutcome::Applied {", "md-server"),
    ("L3", "过滤器切换失败仍记为已应用（不再重试）", RELOAD,
     "                        LevelOutcome::Rejected(error) => {\n                            log_pending = Some(PendingChange {",
     "                        LevelOutcome::Rejected(error) => {\n                            next.log_level = file.log_level.clone();\n"
     "                            log_pending = Some(PendingChange {", "md-server"),
    ("L4", "log_level 没变也每次 SIGHUP 都去切换过滤器", RELOAD,
     "        if file.log_level != self.applied.log_level {", "        if true {", "md-server"),
    ("L5", "不检查 RUST_LOG 是否覆盖，直接尝试切换过滤器", RELOAD,
     "            match self.log.overridden_by() {", "            match None::<String> {", "md-server"),
    ("L6", "生效配置里的 log_level 不做规范化（INFO / warning 等写法与运行值比较出假变更）", RELOAD,
     "    canonicalize_log_level(&mut cfg)?;\n    Ok(cfg)", "    Ok(cfg)", "md-server"),
    # ---- log_level：启动方案与过滤器句柄（logging.rs）----
    ("L7", "启动时忽略 RUST_LOG（环境变量不再优先）", LOGGING,
     "    let Some(raw) = rust_log.map(str::trim).filter(|s| !s.is_empty()) else {",
     "    let Some(raw) = None::<&str> else {", "md-server"),
    ("L8", "启动时忽略 YAML 的 log_level（永远用 info）", LOGGING,
     "        directives: config_level.to_string(),\n        source: FilterSource::Config,",
     '        directives: "info".to_string(),\n        source: FilterSource::Config,', "md-server"),
    ("L9", "RUST_LOG 与 YAML 的 log_level 不一致时不再告警", LOGGING,
     "        if plain != config_level {", "        if false {", "md-server"),
    ("L10", "热更时不检查 RUST_LOG 覆盖（会改动用户用环境变量固定的过滤器）", LOGGING,
     "        if let Some(raw) = self.overridden_by() {\n            return LevelOutcome::Rejected(",
     "        if let Some(raw) = None::<String> {\n            return LevelOutcome::Rejected(", "md-server"),
    ("L11", "热更时只返回成功、不真正重载过滤器", LOGGING,
     "        match self.handle.reload(filter) {",
     "        match { let _ = filter; Ok::<(), reload::Error>(()) } {", "md-server"),
    # ---- log_level：配置校验（md-config）----
    ("L12", "YAML 里的非法 log_level 不再被校验拒绝", CONFIG,
     "    if normalize_log_level(&cfg.log_level).is_none() {",
     "    if false && normalize_log_level(&cfg.log_level).is_none() {", "md-config"),
    ("L13", "任何拼写都被当成合法级别（`fatal`、`verbose` 悄悄变成 info）", CONFIG,
     '        "error" => Some("error"),\n        _ => None,',
     '        "error" => Some("error"),\n        _ => Some("info"),', "md-config"),
]

# B 类：(编号, 标题, 文件, 原文, 替换为, 抓它的端到端场景)
E2E_SCENARIOS = [
    "s9a-log-level-hot-apply",
    "s9b-log-level-from-config-at-startup",
    "s9c-rust-log-takes-precedence",
    "s9d-invalid-log-level-rejected",
    "s9e-log-level-env-override",
    "s9f-prod-198-combination",
]
B_MUTATIONS = [
    ("B1", "`logging::init` 不使用启动方案，过滤器永远是 info（等于旧版本行为）", LOGGING,
     '    let filter = EnvFilter::try_new(&plan.directives).unwrap_or_else(|_| EnvFilter::new("info"));',
     '    let filter = EnvFilter::new("info");', E2E_SCENARIOS),
    ("B2", "`main` 初始化日志时传错级别（不用配置里的 log_level）", MAIN,
     "logging::init(&cfg.log_level)", 'logging::init("info")', E2E_SCENARIOS),
]


def totals(txt):
    res = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed", txt)
    return sum(int(p) for _, p, _ in res), sum(int(f) for _, _, f in res)


def failed_tests(txt):
    return sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", txt, flags=re.M)))


def compiled(txt):
    # rustc 编译失败时一定会打印 "error: could not compile ..."
    return "could not compile" not in txt


def apply_mutation(path, old, new, title):
    src = open(path, encoding="utf-8").read()
    n = src.count(old)
    assert n == 1, f"[{title}] 原文在 {path} 里出现 {n} 次（必须恰好 1 次）：{old[:80]!r}"
    open(path, "w", encoding="utf-8").write(src.replace(old, new, 1))


def show_change(old, new):
    def block(prefix, s):
        return "".join(f"{prefix} {line}\n" for line in s.rstrip("\n").split("\n")) if s.strip() else f"{prefix} （删除）\n"
    return "```diff\n" + block("-", old) + block("+", new) + "```\n"


def e2e_run(scenarios, outdir):
    shutil.rmtree(outdir, ignore_errors=True)
    rc, txt = sh([
        "node", "scripts/compat/sighup_reload_e2e.mjs", "--runner", "native", "--bin", "target/debug/md-server",
        "--out", outdir, "--only", ",".join(scenarios), "--report-only",
    ])
    res = json.load(open(os.path.join(outdir, "results.json"), encoding="utf-8"))
    bad = [f"{r['scenario']}：{r['name']}" for r in res if not r["ok"]]
    return len(res), bad, txt


def main():
    if not ALLOW_DIRTY:
        rc, txt = sh(["git", "status", "--porcelain", "--"] + FILES)
        if txt.strip():
            sys.exit(f"这几个源文件有未提交的修改，先提交（或加 --allow-dirty 调试）：\n{txt}")
    for f in FILES:
        shutil.copyfile(f, BAK[f])
    atexit.register(restore_all)

    out = ["# 变异验证记录\n\n"]
    out.append(
        f"生成时间：{datetime.datetime.now().isoformat(timespec='seconds')}  "
        f"脚本：`evidence/sighup-reload-fix/verify/run_mutations.py`（每次只改一处，跑完立即还原）\n\n"
        "- A 类：`cargo test -p <crate>`，期望至少一个测试失败。\n"
        "- B 类：日志过滤器接线类变异，单元测试覆盖不到（全局 subscriber 只能安装一次），"
        "期望端到端场景（真实进程 + 真实 SIGHUP）失败；同时如实记录单元测试结果。\n\n"
    )
    full = []
    not_caught = []

    # ---------------- A 类 ----------------
    print("== A 类：单元测试 ==", flush=True)
    base = {}
    for pkg in ("md-server", "md-config"):
        rc, txt = sh(["cargo", "test", "-p", pkg])
        base[pkg] = (rc, totals(txt))
        print(f"对照 {pkg}: rc={rc} passed/failed={totals(txt)}", flush=True)
    out.append("## 对照：未改动\n\n")
    for pkg, (rc, (p, f)) in base.items():
        out.append(f"- `cargo test -p {pkg}`：退出码 {rc}；通过 {p}，失败 {f}\n")
    out.append("\n")

    for mid, title, path, old, new, pkg in A_MUTATIONS:
        apply_mutation(path, old, new, mid)
        try:
            rc, txt = sh(["cargo", "test", "-p", pkg])
        finally:
            shutil.copyfile(BAK[path], path)
        ok_compile = compiled(txt)
        failed = failed_tests(txt)
        p, f = totals(txt)
        caught = ok_compile and rc != 0 and len(failed) > 0
        if not caught:
            not_caught.append(mid)
        print(f"{mid} {title}: compiled={ok_compile} rc={rc} passed={p} failed={f} -> {'CAUGHT' if caught else 'NOT CAUGHT'}", flush=True)
        out.append(f"## {mid} {title}\n\n文件：`{path}`（被测：`-p {pkg}`）\n\n{show_change(old, new)}\n")
        out.append(
            f"- 编译：{'通过' if ok_compile else '失败（变异无效）'}；退出码 {rc}；通过 {p}，失败 {f}；"
            f"**{'已被测试抓住' if caught else '没有被抓住'}**\n"
        )
        if failed:
            out.append(f"- 失败的测试（{len(failed)} 个）：\n" + "".join(f"  - `{t}`\n" for t in failed))
        out.append("\n")
        full.append(f"\n\n=============== {mid} {title} ===============\n{txt}")

    # ---------------- B 类 ----------------
    print("== B 类：端到端 ==", flush=True)
    rc, txt = sh(["cargo", "build", "-p", "md-server"])
    assert rc == 0, txt
    n, bad, _ = e2e_run(E2E_SCENARIOS, os.path.join(BAK_DIR, "e2e-control"))
    print(f"对照 E2E: {n} 条断言，失败 {len(bad)}", flush=True)
    out.append(f"## 对照：未改动（端到端）\n\n- 场景 {', '.join(E2E_SCENARIOS)}：{n} 条断言，失败 {len(bad)} 条\n\n")
    assert not bad, f"未改动的代码上端到端就有失败：{bad}"

    for mid, title, path, old, new, scenarios in B_MUTATIONS:
        apply_mutation(path, old, new, mid)
        try:
            rc_b, txt_b = sh(["cargo", "build", "-p", "md-server"])
            rc_u, txt_u = sh(["cargo", "test", "-p", "md-server"])
            if rc_b == 0:
                n, bad, e2e_txt = e2e_run(scenarios, os.path.join(BAK_DIR, f"e2e-{mid}"))
            else:
                n, bad, e2e_txt = 0, [], ""
        finally:
            shutil.copyfile(BAK[path], path)
        up, uf = totals(txt_u)
        caught = rc_b == 0 and len(bad) > 0
        if not caught:
            not_caught.append(mid)
        print(f"{mid} {title}: build_rc={rc_b} unit passed={up} failed={uf}; E2E {n} 条，失败 {len(bad)} -> {'CAUGHT' if caught else 'NOT CAUGHT'}", flush=True)
        out.append(f"## {mid} {title}\n\n文件：`{path}`\n\n{show_change(old, new)}\n")
        out.append(
            f"- 编译：{'通过' if rc_b == 0 else '失败（变异无效）'}\n"
            f"- 单元测试（`cargo test -p md-server`）：通过 {up}，失败 {uf}"
            f"{'（单元测试抓不住这类接线变异，符合预期）' if uf == 0 else ''}\n"
            f"- 端到端：{n} 条断言，失败 {len(bad)} 条；**{'已被端到端抓住' if caught else '没有被抓住'}**\n"
        )
        if bad:
            out.append("  失败的断言：\n" + "".join(f"  - {b}\n" for b in bad))
        out.append("\n")
        full.append(f"\n\n=============== {mid} {title} ===============\n[build]\n{txt_b}\n[unit]\n{txt_u}\n[e2e]\n{e2e_txt}")

    # 还原 + 重新编译，保证 target/debug/md-server 对应未改动的源码
    restore_all()
    sh(["cargo", "build", "-p", "md-server"])

    # ---------------- 还原核对 ----------------
    same = all(filecmp.cmp(f, BAK[f], shallow=False) for f in FILES)
    rc, dirty = sh(["git", "status", "--porcelain", "--"] + FILES)
    out.append("## 还原核对\n\n")
    out.append(f"- 与备份逐字节一致：{'是' if same else '否'}\n")
    if not ALLOW_DIRTY:
        out.append(f"- `git status --porcelain`（这几个源文件）为空：{'是' if not dirty.strip() else '否'}\n")
    total = len(A_MUTATIONS) + len(B_MUTATIONS)
    out.append(f"\n## 结论\n\n共 {total} 个变异，抓住 {total - len(not_caught)} 个"
               f"{'，全部被抓住。' if not not_caught else '；没被抓住：' + ', '.join(not_caught)}\n")

    open(os.path.join(OUT, "mutation-tests.md"), "w", encoding="utf-8").write("".join(out))
    open(os.path.join(OUT, "mutation-tests.full.log"), "w", encoding="utf-8").write("".join(full))
    print(f"完成：{total} 个变异，没被抓住 {len(not_caught)} 个；已还原一致={same}", flush=True)
    sys.exit(len(not_caught) if same else 99)


if __name__ == "__main__":
    main()
