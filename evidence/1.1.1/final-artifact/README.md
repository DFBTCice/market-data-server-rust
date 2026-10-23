# 1.1.1 最终产物复验（2026-10-24）

任务来源：主审 docs/31、交接 docs/33。当前结果与未完成项见 [REPORT.md](REPORT.md)。

| 路径 | 内容 |
|---|---|
| `build-manifest.txt`、`../build-logs/dist-sha256.txt` | a06fe20 干净树、固定时间与路径、两次从零构建相同 SHA |
| `../build-logs/final-jemalloc-build-{1,2}.log` | 两次完整编译输出；二进制只在 gitignore 的 dist/ |
| `verify/` | fmt/clippy/248 个工作区测试、快照 11、兼容回放 5、对比器正对照 25 原文 |
| `sighup-final/`、`sighup-final.log` | 最终 jemalloc 二进制 Docker E2E（129/129） |
| `sighup-control-1.1.0/`、`sighup-control-1.1.0.log` | 同脚本检测旧版，验证缺陷可被检出 |
| `config.final.yaml`、`config.diff` | 从现网只读导出，仅 log_level info → warn；RUST_LOG=warn 保留 |
| `Dockerfile.release`、`image-build.jsonl`、`image-manifest.json` | endpoint 3、基于 1.0.0-rollback 的最终镜像及身份 |
| `baseline/` | 现网只读基线、限定字段 inspect、10-21 重启事件查询（0 条）及原因未确认记录 |
| `canary/` | 原版与最终镜像同订阅、同窗口 240s，对比日志与门禁退出码；原始 WS 抓取留本机 |
| `memory-2h/REPORT.md`、`analysis.json`、`steady-load-analysis.json` | 实际7261秒、每侧122有效样本，持续负载完整1h new RSS +0.0724、old +10.0034MiB/h；两小时门禁已通过 |
| `anomalies/REPORT.md`、`assessment.json`、`live-probe/` | 用户追加的断线/写超时与粗键重复审计；最终镜像严格重复0、旧版正对照3715；历史客户端进程停顿未证实 |
| `../nas-198/pre-swap-observe/` | 等待期间现网只读监测；自行重启只记录，不主动处理 |
| `../nas-198/post-swap-24h/DEPLOYMENT_RECORD.md` | 03:53:15实际换版、即时12=12订阅恢复、换后240s兼容复验与真实24h逐分钟观察；最终观察尚未完成 |

复现：仓库根目录 `bash evidence/1.1.1/tools/build_final.sh`、`bash evidence/1.1.1/tools/verify_final.sh`；Docker E2E 命令见 REPORT。
NAS 工具只读取 `/tmp/pt_jwt`（600）。`build_release_image.py` 若目标标签已存在会拒绝覆盖；不要为复跑删除现网或回滚资源。
`final_canary.py` 起两只 256MiB/2CPU 旁路，完成 240s + 7260s 采样后自动删除；分析用 `analyze_final_memory.py`。

历史报告保留其当时结论。历史 fa7be337/a130e411/exp-v5 都不等于本次最终产物，不能代替本目录复验。
