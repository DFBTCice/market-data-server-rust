# 198 换版计划：1.1.1 → 1.2.0（开关打开、报价补订关闭）

依据：主审复核意见（2026-11-06）§四、§五与业务方决定。**金丝雀两部分通过、业务方批准后才执行**；执行前把时间告诉业务方（转主审）。

## 1. 产物与镜像

| 项 | 值 |
|---|---|
| 源码提交 / 产物提交 | 见 `evidence/1.2.0/REPORT.md` 首段与 `build-logs/dist-sha256.txt` |
| 构建特性 | `--features md-connector/vendored-tls,md-server/jemalloc-stats`，两次从零构建 sha 一致 |
| 镜像 | `md-server:1.2.0` = `md-server:1.0.0-rollback`（`06907bdf7fd8`）+ 产物 + `config.release.yaml`；`build_release_image.py` 构建并写 `image-manifest.json` |
| 配置 | `config.release.yaml` = 现网 `config.final.yaml` + `runtime_subscriptions`（开关打开、闲置 10 分钟、保护 10 分钟、检查 60 秒、上限 200、`on_demand_quote: false`）；diff 见 `config.release.diff` |
| 容器环境 | `RUST_LOG=warn,md_runtime_subs=info`、`TZ=Asia/Shanghai`；端口、内存 512 MiB、CPU、重启策略、日志配置沿用旧容器 |

## 2. 前置条件（`execute_swap.py` 逐条断言）

1. `canary/gate-results.json`：第 1 部分 push / subs / metrics / noquote / selftest 全部 0（quote 只允许已裁定噪声）。
2. `canary/short/summary.json`、`canary/prod/summary.json`：`pass: true`（含报价话题未被补订）。
3. 现网容器 `md-server` 镜像 ID = `sha256:3250e021…f010`（1.1.1）且在运行；`md-server-1.1.1-bak` 不存在。
4. `md-server:1.2.0` 镜像 ID 与 `image-manifest.json` 一致，标签里的二进制 sha 一致。
5. 环境变量 `APPROVED` 写明业务方批准人与时间。

## 3. 步骤（低峰）

```bash
# 0. 业务方提供 Portainer 会话（/tmp/pt_jwt），确认窗口并通知主审
python3 evidence/1.2.0/tools/build_release_image.py dist/<产物> <sha256> <源码提交>
APPROVED="<批准人 时间>" python3 evidence/1.2.0/tools/execute_swap.py
#   记录换前订阅 → 旧容器改名 md-server-1.1.1-bak 并停止（保留作回退）→ 创建新 md-server → 健康后立刻 restore_subs.mjs
#   → 即时检查（健康、两个消费者回来、镜像/环境/资源参数、容器内二进制与配置逐字节、开关=1、jemalloc 指标、写超时 0）
bash evidence/1.2.0/tools/observe_24h.sh                 # 立即开始 24 小时每分钟采样
sleep 600 && python3 evidence/1.2.0/tools/check_after_swap.py   # 10 分钟验收
```

## 4. 回退（两级）

1. **不重启**：SIGHUP 关闭开关，或只关报价补订（OPS_GUIDE §8.6：`docker exec -u root md-server sed -i ...` + `docker kill --signal=HUP md-server`；看 `md_runtime_subscriptions_enabled`）。
2. **切回 1.1.1 旧容器**：`python3 evidence/1.1.0/nas-198/tools/pt_swap.py rollback md-server-1.1.1-bak`，随后立即 `restore_subs.mjs`。

触发条件：健康 60 秒不恢复（脚本自动回退）；消费者 90 秒未回到 2 个（脚本自动回退）；10 分钟验收不通过；24 小时内出现 OMS / botd 数据中断、重连风暴、RSS 异常增长或配置订阅被退。

## 5. 24 小时采样（`post-swap-24h/prod.csv`，每分钟）

RSS（1 小时预热后斜率）、`md_runtime_subscriptions` 曲线、闲置退订 / 按需补订 / 失败 / 上限拒绝累计、各 quote 入站速率（`quote_counts` 相邻行相减）、
`md_ticks_dropped` 与 `md_quotes_dropped`、OKX / 币安各连接器重连次数；订阅增减事件写 `prod.events.log`。botd 侧行情年龄与重连计数由主审核对。
