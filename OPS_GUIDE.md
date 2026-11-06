# 生产运维部署指南（补充）

> 本文档补充 `DEPLOYMENT.md`，聚焦 Docker 生产部署、热重载真实能力和 CI/CD 实践。

> **NAS 198 现网约束（2026-11-05）**：只使用已验收本机镜像 `md-server:1.1.1`，并核对镜像 ID `sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010` 和二进制 SHA-256 `3ffa127752ae8c13fc7646b5c75f8d3f1fa1c0b915b2bff596c1c93328c6c79d`。不得从任何 `latest` 部署；GitHub Release/GHCR 的同版本 CI 产物也不是验收产物。身份、两次自动发布和 jemalloc 核对见 [DEPLOYMENT §0](DEPLOYMENT.md#0-nas-198-现网产物约束2026-11-05-更正)。本次只更正文档，不访问或操作 198。

---

## 一、热重载（SIGHUP）真实能力清单

从代码 `crates/md-server/src/reload.rs`（重载逻辑）和 `main.rs` 的 `wait_for_signal()` 分析，**SIGHUP 热重载的实际能力如下**：

> **规则一句话**：SIGHUP **只热更订阅列表、日志级别**（`RUST_LOG` 优先）**和 `runtime_subscriptions` 整段**（1.2.0）；其余配置项只在进程启动时读取，文件里改了不会生效，必须重启。
>
> 进程内部把「文件里的新配置」和「实际已应用配置」分开记录。每次 SIGHUP 都拿文件和**实际在运行的配置**对比：
> 只要还有没生效的差异，**每一次** SIGHUP 都会继续点名提示（不会因为发过一次就"记成已应用"）；把文件改回原值，提示自动消失。
> **只有日志里出现 `config reloaded and applied successfully`，才表示文件内容已全部生效。**
> （1.1.0 及早期 1.1.1 候选的缺陷：只改 OKX Business 地址会误报成功；第二次 SIGHUP 会把未生效的地址当成已应用。修复已并入 1.1.1，见 `CHANGELOG.md`。）

### ✅ 真正支持热更（无需重启）

| 配置项 | 是否热更 | 说明 |
|--------|---------|------|
| **订阅列表** | ✅ | `subscribe_ticks`、`subscribe_klines` 的增删会调用 `add_subscriptions` / `remove_subscriptions`，实时生效 |
| **quote 订阅列表**（1.1.0） | ✅（有前提） | `connectors.{binance,okx}.subscribe_quotes` 的增删实时生效，**前提**是对应 quote 连接器已在运行（启动时 `quotes_enabled=true` 且列表非空） |
| **`log_level`（日志级别）** | ✅（有前提） | SIGHUP 立即切换日志过滤器，**不重连、不重启**。**前提**：进程环境里没有设置 `RUST_LOG`——`RUST_LOG` 优先于 YAML，设了它 `log_level` 就不起作用（见下面"日志级别"一节） |
| **`runtime_subscriptions`**（1.2.0） | ✅ | 开关与参数立即生效，不重连、不丢运行期订阅；运行中打开时保护期从打开时刻算。日志 `runtime_subscriptions changed: {...} -> {...} (applied)`。见第八节 |

订阅列表热更的**共同前提**是对应连接器启动时已创建：Binance 需 `binance.enabled=true`；OKX tick 需 `okx.enabled` 且 `subscribe_ticks` 非空；
OKX K 线需 `okx.enabled` 且 `subscribe_klines` 非空；quote 需 `quotes_enabled=true` 且列表非空。
不满足时变更**不会被静默吞掉**：日志点名 `... connector is not running ... NOT applied, requires restart`，且之后**每次** SIGHUP 都会继续提示，直到重启或把列表改回原值。

### ❌ 不支持热更（修改后提示需要重启）

除订阅列表、日志级别和 `runtime_subscriptions` 外，**其余配置项**都归此类（新增配置项默认也归此类，代码里用穷尽解构强制分类）。常见项：

| 配置项 | 说明 |
|--------|------|
| `connectors.{binance,okx}.stream_base_url*`（`binance.stream_base_url`、`binance.stream_base_url_public`、`okx.stream_base_url_public`、**`okx.stream_base_url_business`**） | 地址变更需要重建连接，不支持热更。**自动重连也不会换地址**：连接器只持有启动时的地址，等断线重连不会重新读 YAML，必须重启 |
| `connectors.{binance,okx}.enabled`、`quotes_enabled`（1.1.0） | 连接器启停需要重建 |
| `connectors.{binance,okx}.reconnect_delay`、`ping_interval`、`rest_base_url` | 只在启动时读取（此前 SIGHUP 完全不检测，现在会点名提示） |
| `grpc_server.*`、`api_gateway.*`（含 `listen_address`、`ws_write_wait`）、`admin_server.*`、`ws_server.*` | 端口已绑定 / 参数启动时读取 |
| `processor.{tick,kline,quote}_channel_buffer`、`processor.broadcast_capacity` | 通道已创建，无法 resize |

> `--port-offset` 与环境变量覆盖（如 `API_GATEWAY_LISTEN_ADDRESS`）：启动与 SIGHUP 使用同一套"生效配置"推导，
> 配置文件没改时不会再误报"监听地址变更"（此前带 `--port-offset` 的实例每次 SIGHUP 都会误报）。

### 日志级别（`log_level`）怎么生效

1.1.0 及早期 1.1.1 候选里 YAML 的 `log_level` 从未生效（只打印，过滤器由 `RUST_LOG` 决定，没设就是 `info`）。最终 1.1.1 已修复：

| 优先级 | 来源 | 说明 |
|--------|------|------|
| 1（最高） | 环境变量 `RUST_LOG`（已设置、非空、写法合法） | 整个过滤器由它决定，可以写 `warn,md_connector=debug` 这类逐模块指令。**此时 YAML 的 `log_level` 不起作用**，SIGHUP 改它也不会生效（会明确提示） |
| 2 | YAML `log_level`（环境变量 `LOG_LEVEL` 可覆盖它） | 启动时按它初始化；SIGHUP 热更 |
| 3 | 默认 | `info` |

- **合法值**：`trace` / `debug` / `info` / `warn`（也接受 `warning`）/ `error`，不区分大小写。写成别的（`fatal`、`verbose`、拼写错误）**启动时直接报错退出，SIGHUP 时拒绝并保持运行配置**——不会悄悄当成别的级别。
- **为什么 `RUST_LOG` 优先**：环境变量优先于配置文件是通行约定；现网（198、compose、systemd 样例）都设了 `RUST_LOG=warn`，这样升级后日志量不会被悄悄改变。
- **两者不一致时不会装作生效**：启动时有一行 WARN，例如 `config log_level "info" is overridden by RUST_LOG=warn: ...`；SIGHUP 改 `log_level` 时也会每次提示 `log_level changed: ... (NOT applied, requires restart; RUST_LOG=warn takes precedence ...)`。
- **想让 YAML + SIGHUP 接管日志级别**：去掉容器 / 服务环境里的 `RUST_LOG`，在 YAML 里写想要的级别（线上想保持安静就写 `log_level: "warn"`），重启一次；之后改级别只需编辑 YAML 再发 SIGHUP。
- 热更不会断开任何连接，也不会清空订阅；日志输出格式不变。
- **`debug` / `trace` 很吵**：连接器会为收到的**每一条** WebSocket 消息写一行 debug 日志，行情高峰期日志量很大，生产环境不要长期开。
  临时排查可以把 `log_level` 改成 `debug` 再发 SIGHUP（不断连），查完改回去再发一次 SIGHUP。

### 使用方式

```bash
# 1. 修改 config.yaml（比如新增一个订阅币对）
vim /opt/md-server/config.yaml

# 2. 发送 SIGHUP 信号
kill -HUP $(docker inspect --format='{{.State.Pid}}' md-server)
# 或 systemd 方式：
sudo systemctl kill -s HUP md-server

# 3. 查看日志确认（以下为日志级别 info 时的预期输出）
# 订阅变更热更成功：
#   subscription lists changed, applying...
#   binance: adding 1 subscriptions
#   config reloaded and applied successfully
# 日志级别热更成功（没设 RUST_LOG 时；"applying..."在旧级别下输出，"(applied)"在新级别下输出）：
#   log_level changed: info -> warn, applying...
#   log_level changed: info -> warn (applied)
# 设了 RUST_LOG 时 log_level 的改动不会生效（WARN，之后每次 SIGHUP 重复提示）：
#   WARN log_level changed: info -> debug (NOT applied, requires restart; RUST_LOG=warn takes precedence over log_level; ...)
# 有需要重启才生效的变更（点名配置路径、旧值与文件值；WARN 级别，之后每次 SIGHUP 重复提示）：
#   WARN connectors.okx.stream_base_url_business changed: "wss://old" -> "wss://new" (NOT applied, requires restart)
#   WARN some config changes require connector restart -- please restart the service (1 pending: connectors.okx.stream_base_url_business)
# 配置文件读不出来 / YAML 解析失败 / 校验失败（ERROR 级别；运行配置与订阅均保持不变）：
#   ERROR failed to reload config: ..., keeping current running config (nothing applied)
```

**线上 198 使用 `RUST_LOG=warn`**：`config reloaded and applied successfully` 是 INFO，在该级别下看不到——**成功时日志是静默的**；
只要出现任何 WARN / ERROR（`some config changes require connector restart ...`、`failed to reload config ...`），就说明有事要处理。
198 的 1.1.0 镜像 YAML 写 `log_level: "info"`，实际由 `RUST_LOG=warn` 控制。1.1.1 换版**保留 `RUST_LOG=warn`**，
镜像内 YAML 同步改为 `log_level: "warn"`；本次不切换日志控制方式，也不会增加启动时的覆盖告警。
（1.1.0 在该级别下，只改 OKX Business 地址再发 SIGHUP 完全没有输出；最终 1.1.1 会持续 WARN。）

### 与 Go 版热重载对比

| 能力 | Go 原版 | Rust 版 |
|------|---------|---------|
| 订阅列表热更 | ✅（通过连接器重建实现） | ✅（直接 add/remove，更轻量） |
| 连接器重建 | ✅（Stop旧→新建→Start新） | ❌（不支持） |
| log_level | ✅（viper 支持运行时改） | ✅（SIGHUP 热更；`RUST_LOG` 优先，设了它则 YAML 不起作用） |
| 监听地址 | ❌ | ❌ |
| 缓冲区大小 | ❌ | ❌ |

**结论**：Rust 版的热重载在订阅列表变更上比 Go 版更轻量（不需要重建连接），日志级别也能热更；Go 版则支持完整的连接器重建。Rust 版 README 的描述过于保守，实际上订阅热更已经做得很好了。

---

## 二、Docker 生产部署

### 2.1 构建生产镜像

现网只复用已验收的本机镜像，按上方不可变 ID 核对；本机缺失或 ID 不匹配即停止。默认 `Dockerfile` 只执行 `cargo build --release`，没有启用 jemalloc，不能替换现网 1.1.1。
复现候选构建必须使用 `--features md-connector/vendored-tls,md-server/jemalloc-stats` 并重新独立验收；版本号相同不表示产物相同。

### 2.2 快速启动（单容器）

仅用于已获批准、运行参数另行核对的部署；禁止自动拉取。本次文档更正不执行以下命令。

```bash
docker run --pull=never -d \
  --name md-server \
  --restart unless-stopped \
  -p 8081:8081 \
  -p 50051:50051 \
  -v $(pwd)/config.yaml:/etc/md-server/config.yaml:ro \
  -e RUST_LOG=warn \
  -e TZ=Asia/Shanghai \
  --memory=512m \
  --cpus=2 \
  sha256:3250e021d152ad38b5b873325eae5f42364252881a1925b2a4ea6ca1b10cf010
```

> `-e RUST_LOG=warn` 会把日志级别固定为 warn，YAML 的 `log_level` 与 SIGHUP 对它不起作用（见第一节"日志级别"）。
> 想用 YAML + SIGHUP 控制日志级别：去掉这一行，在 `config.yaml` 里写 `log_level: "warn"`。

### 2.3 生产编排（Docker Compose + 监控）

仓库 `docker-compose.prod.yml` 当前引用 GHCR `latest`，**不能直接用于 NAS 198 现网，也不要执行其 `pull` / `up` 作为升级方式**。本次不修改该模板。
今后如另行采用 Compose，须先固定为已验收本机镜像 ID、禁止拉取/重建并独立验收整套编排；当前现网仍按 [已验收换版计划](evidence/1.1.1/nas-198/SWAP_PLAN_1.1.1.md) 复制运行参数、留备份和即时补订阅。

启动后访问：
- REST API: `http://<host>:8081`
- Prometheus: `http://<host>:9090`
- Grafana: `http://<host>:3000` (默认账号 admin/admin)

### 2.4 自动化部署脚本

现有 `deploy.sh` 默认拉取 GHCR `latest`，指定版本时也会把镜像重标为 `latest`；`--local` 分支同样不核对验收镜像 ID。**该脚本不是 NAS 198 的现网部署入口，不能直接执行。** 本次只说明现有行为，不修改脚本或生产配置。

---

## 三、生产版本编译优化

当前 `Cargo.toml`（workspace root）没有自定义 release profile。建议在根 `Cargo.toml` 末尾追加：

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

效果：
- `lto = "fat"` + `codegen-units = 1`: 最大链接优化，二进制更小、更快
- `panic = "abort"`:  panic 时不展开栈，减小二进制体积
- `strip = true`:  移除调试符号，减小体积

优化后二进制大小预期从 ~20MB 降至 ~12-15MB。

---

## 四、CI/CD 流水线（GitHub Actions）

已创建 `.github/workflows/` 下的流水线：

### 4.1 CI 流水线 (`ci.yml`)

**触发条件**：每次 push 到 main / release/* 分支，或 PR 到 main

**阶段**：
1. **Check & Lint**: `cargo fmt` + `cargo clippy` + `cargo check`
2. **Test**: `cargo test --all-features --workspace`（debug + release 模式）
3. **Build Release**: 仅在 main 分支 push 时执行，生成 release 二进制并上传 artifact

### 4.2 Release 流水线 (`release.yml`)

**触发条件**：推送 `v*.*.*` 标签

**阶段**：
1. **Build & Push Docker Image**: 构建 amd64 镜像，推送至 GHCR (`ghcr.io/dfbtcice/market-data-server-rust`)
   - 自动写入去掉 `v` 的版本标签（如 `1.1.1`）、`1.1`、`1`、源码短 SHA 和 `latest`；不同版本的运行会覆盖共享别名
   - 使用 Docker layer cache (`type=gha`) 加速
2. **Build Release Binary**: Ubuntu runner 上执行 `cargo build --release`，打包 `md-server-linux-amd64.tar.gz`；当前不是双架构构建
3. **Create GitHub Release**: 自动创建 Release，附带二进制压缩包

2026-11-05 的 [42017971662（v1.1.1）](https://github.com/DFBTCice/market-data-server-rust/actions/runs/42017971662) 与 [42017972062（v1.1.0）](https://github.com/DFBTCice/market-data-server-rust/actions/runs/42017972062) 均已完成以上发布；业务方决定保留，不删除或覆盖。两次正式二进制及默认 Dockerfile 构建都没有启用 jemalloc。测试步骤的 `--all-features` 不会被后续 `cargo build --release` 继承，不能据此把 CI 产物认作已验收现网二进制。

### 4.3 使用流程

```bash
# 1. 开发完成，合并到 main
# CI 自动跑测试和检查

# 2. 打标签即触发发布，必须事先明确批准 CI 的外部发布副作用
git tag v1.0.0
git push origin v1.0.0

# 3. GitHub Actions 自动:
#    - 构建 Docker 镜像并推送到 ghcr.io
#    - 构建 amd64 二进制并创建 Release、上传资产

# 4. 发布完成不等于产物已通过现网验收，不自动部署
#    NAS 198 只使用已验收的本机镜像 ID；不从 latest 或同版本 CI 标签部署
```

### 4.4 回滚流程

NAS 198 回滚使用已保留、已核对身份的本机旧镜像/停止备份容器，不从 GHCR 拉取并重标 `latest`。按 [SWAP_PLAN §5](evidence/1.1.1/nas-198/SWAP_PLAN_1.1.1.md#5-回滚已授权换版的恢复步骤) 先保存即时非空订阅、保留失败现场，切回后立即补订阅并核验缺失0、健康和消费者恢复；本次不执行回滚。

---

## 五、服务器部署检查清单

```bash
# 1. 系统要求检查
ulimit -n                    # 应 >= 65536
free -h                      # 应 >= 512MB 空闲
nproc                        # 应 >= 2 核

# 2. 网络检查
ping -c 3 fstream.binance.com
ping -c 3 ws.okx.com

# 3. 防火墙
sudo ufw allow 8081/tcp      # REST API
sudo ufw allow 50051/tcp     # gRPC
sudo ufw allow 9090/tcp      # Prometheus（如需公网访问，建议加认证）
sudo ufw allow 3000/tcp      # Grafana（同上）

# 4. 时区
sudo timedatectl set-timezone Asia/Shanghai

# 5. 日志持久化
# docker-compose.prod.yml 已配置 volumes，日志自动持久化
# 如需额外备份:
docker exec md-server tar -czf - /var/log/md-server > md-logs-backup.tar.gz
```

---

## 六、常见问题排查

| 问题 | 诊断命令 | 解决 |
|------|---------|------|
| 容器启动后退出 | `docker logs md-server` | 检查 config.yaml 格式是否正确 |
| 健康检查失败 | `curl -v http://localhost:8081/health` | 检查端口映射和防火墙 |
| 无数据流入 | `docker logs md-server \| grep connector` | 检查网络连通性，Binance/OKX 是否可达 |
| 内存过高 | `docker stats md-server` | 先看 [七、内存监控与客户端卡顿](#七内存监控与客户端卡顿111-起)：1.1.0调查的主因是 musl 分配器高水位，消费者卡顿 / 行情突发会推高水位；1.1.1发布必须使用jemalloc构建。调小 `broadcast_capacity` 不是根治 |
| SIGHUP 无效 | `docker kill --signal=HUP md-server` | Docker 容器中 kill -HUP 可能需要 docker kill |

---

## 七、内存监控与客户端卡顿（1.1.1 起）

### 7.1 一句话结论（详见 `evidence/1.1.1/INVESTIGATION_REPORT.md`）

此前1.1.0现网 RSS"只升不降、呈台阶状"（存活堆只有 7~10 MiB），主因是 **musl 自带分配器的高水位**；Gateway退订泄漏是另一个已修复的真实缺陷：
内存一旦被申请过就不再还给系统，行情突发、连接器重连、消费者卡顿都会把水位推高，之后不回落（旁路等价负载下撤掉全部消费者后仍会缓慢上涨）。
同样的代码、同样的负载，换 jemalloc 分配器后 RSS 持平（6 小时斜率 ≈ 0），卡顿结束后内存会回落。

北京时间2026-10-24 03:53:15已换为最终jemalloc版1.1.1，立即补订阅12=12，十分钟及换后240秒兼容验收通过；完整24小时观察仍在进行。生产初始RSS约57MiB，状态以[`换版记录`](evidence/1.1.1/nas-198/post-swap-24h/DEPLOYMENT_RECORD.md)的实际采样为准。

其中"消费者卡顿"这一类放大器有明确机制：客户端停止读 socket → 服务端该连接的写操作永久阻塞 → 处理协程不再 `recv`
→ 各主题 broadcast 环（容量 `broadcast_capacity`，默认 4096，单环满载约 2 MiB）里的事件无法被释放。
1.1.1 起 Gateway（`/ws/v1/data`）按已有配置项 `api_gateway.ws_write_wait`（默认 `10s`）给每次写入加超时，
卡顿超过该时长的客户端被断开并释放其全部订阅（计数 `md_ws_kicked_write_timeout_total`）。
**注意：这只封顶了"卡顿造成的滞留"，不能让 musl 分配器的 RSS 变平；要让 RSS 平稳需要 jemalloc 构建**
（`--features md-connector/vendored-tls,md-server/jemalloc-stats`，见 DEPLOYMENT §12.2 的候选 A；旁路等价负载 6 小时斜率 +0.08 MiB/h，默认分配器候选为 +16.7 MiB/h）。

> **行为变化（仅此一项，需知会消费者）**：1.1.0 及以前该配置项被解析但从未生效；1.1.1 起，Gateway 客户端若 `ws_write_wait`
> 内读不动数据（进程挂起 / 半开连接 / 严重卡顿）会被服务端断开，需自行重连并重新订阅。想保持旧行为：`ws_write_wait: "0s"`。
> Legacy `/ws` 与 gRPC 流不受影响（本版未改动，同样存在"停读客户端钉住连接"的风险）。

### 7.2 退订语义（1.1.1 修正）

| 操作 | 1.1.0 及以前（Gateway） | 1.1.1 |
|---|---|---|
| `unsubscribe` 一个 stream | 只从"已订阅集合"移除，接收端仍留在连接内（泄漏）；之后重新 `subscribe` 会**重复推送**（现网探针实测 100% 重复） | 立刻释放该 stream 的接收端；`md_ws_subscriptions_active` 同步回落 |
| 重复 `subscribe` 同一 stream | 已订阅则忽略 | 不变（不再新增接收端、不再重复 ack） |
| 对未订阅的 stream `unsubscribe` | 返回 success ack | 不变 |
| 连接断开 | 连接内全部接收端随连接释放 | 不变 |
| 推送字节 | — | 与 1.1.0 逐字节相同（`compat_1_0_0` + `compare.mjs` 验证） |

Legacy `/ws` 的订阅/退订行为未改动。

### 7.3 内存与连接监控

| 指标 | 怎么看 |
|---|---|
| `process_resident_memory_bytes` | RSS。**建议阈值：≥ 400 MiB（容器限额 512 MiB 的约 78%）持续 10 分钟 → 在低峰期主动重建容器**（清零 musl 高水位；重建后补订阅，见 DEPLOYMENT §12.2）；≥ 480 MiB 视为紧急 |
| `md_ws_subscriptions_active` | 所有 WS 连接持有的接收端总数。退订/断线后必须回落；与 `md_ws_active_clients` 对照（一个连接订 N 个主题就贡献 N）。只增不减 = 泄漏 |
| `md_ws_kicked_write_timeout_total` | 发送等待超过阈值而被踢的 Gateway 连接累计数。增长须单独核对客户端、链路及服务端发送回压；不能仅凭计数或固定文案确认消费者卡顿 |
| `md_ws_kicked_lagged_total` / `md_broadcast_lagged_total` | 消费者跟不上被踢 / 环溢出丢事件 |
| `md_ticks_dropped` / `md_klines_dropped` / `md_quotes_dropped` | 入口队列满丢弃。1.1.1 起日志里会限频输出 WARN（含时间与队列深度） |
| `md_tokio_alive_tasks` | 重连前后应回到同一水平（22 左右），只增不减 = task 泄漏 |

Prometheus 告警示例（与 DEPLOYMENT §7 的 500MB 提示规则并存，这条更早）：

```yaml
- alert: MdServerMemoryHigh
  expr: process_resident_memory_bytes{job="md-server"} >= 400 * 1024 * 1024
  for: 10m
  labels: { severity: warning }
  annotations:
    summary: "md-server RSS ≥ 400MiB，请安排低峰换版或重建容器（限额 512MiB）"
- alert: MdWsClientStalled
  expr: increase(md_ws_kicked_write_timeout_total{job="md-server"}[10m]) > 0
  labels: { severity: info }
  annotations:
    summary: "有 Gateway 消费者卡顿被踢出（写超时），请检查消费者"
- alert: MdWsSubscriptionsLeak
  expr: md_ws_subscriptions_active{job="md-server"} > 10 * md_ws_active_clients{job="md-server"} + 100
  for: 15m
  labels: { severity: warning }
```

### 7.4 排障顺序（RSS 异常时）

1. `curl :8081/metrics | grep -E 'md_ws_|process_resident|md_tokio'`：`md_ws_subscriptions_active` 是否异常大？`md_ws_kicked_write_timeout_total` 是否在涨？
2. 看日志里 `kicked: write blocked longer than ws_write_wait`：谁在卡、多久一次。
3. RSS 在"平台 + 台阶"形态时结合allocated与负载核对分配器高水位，不能只凭形状排除泄漏。1.1.1发布必须使用jemalloc构建；消费者及时读取与写超时保护可减少滞留。
4. 需要区分"逻辑滞留 vs 分配器"时，用 jemalloc 构建（`--features md-connector/vendored-tls,md-server/jemalloc-stats`）看 `md_jemalloc_allocated_bytes`（存活堆，应持平在 7~12 MiB）与 `md_jemalloc_resident_bytes`（随负载起落、会回落）。默认构建不输出 `md_jemalloc_*`。
5. 现网 cgroup 里 `anon_thp`/`thp_collapse_alloc` 非零（只读 `docker stats` 的 memory_stats.stats）是一个**未证实**的线索：内核把稀疏堆折叠成大页可能造成 RSS 单分钟跳升（调查报告 §D4）。
6. `MD_DEBUG_FORCE_RECONNECT_SECS` / `MD_DEBUG_FORCE_RECONNECT_ONLY` 是**仅限旁路实验**的调试开关（强制周期性重连），不要在生产设置。

---

## 八、运行期订阅闲置退订与按需补订（1.2.0 起，默认关闭）

### 8.1 它做什么、不做什么

- **做**：运行期订阅（`POST /api/v1/subscriptions`、gRPC Admin 登记的，以及本功能按需补订的）**连续 `idle_minutes` 没人接收**就退掉上游；
  有连接或 gRPC 订阅某话题而**上游没有**对应订阅时，**自动补订**。两件事同一个开关。
- **不做**：不碰配置名单（`subscribe_ticks` / `subscribe_klines` / `subscribe_quotes`）里的订阅；不持久化运行期订阅（重启仍丢，换版后仍要 `restore_subs.mjs`）；
  不改推送内容、旧 `/ws`、REST/gRPC 既有字段。
- **"有人在接收"**（任一条）：该目标的规范话题（交易所小写、币对大写，如 `tick.okx.BTC-USDT-SWAP`、`kline.5m.binance.BTCUSDT`）上有 `/ws/v1/data`、旧 `/ws`
  或 gRPC 流的接收端；最近 `idle_minutes` 内有 `GET /api/v1/data/latest/...` 查询过它；最近 `idle_minutes` 内有新接收端订阅过它。

### 8.2 开关与参数

```yaml
runtime_subscriptions:            # 整段可选；不写 = 关闭，行为与 1.1.1 相同
  idle_unsubscribe: false         # 总开关
  idle_minutes: 10                # 连续多少分钟没人接收才退订（≥1）
  check_interval_seconds: 60      # 检查周期（≥1）
  startup_grace_minutes: 10       # 启动 / 运行中打开后多少分钟内不退订（可为 0）
  max_runtime_subscriptions: 200  # 运行期订阅达到该数后不再按需补订（≥1）；POST 不受限
```

- 键名写错（如 `idle_unsubcribe`）启动时报错退出、SIGHUP 时拒绝并保持运行配置，不会让开关静默失效。
- 实际退订时刻在"最后一个接收方离开后 `idle_minutes`"到"再加一个 `check_interval_seconds`"之间，不会提前。
- 内部常量（不可配）：按需补订等待 200ms 合批；上游拒绝后退避 5 分钟起、翻倍、封顶 1 小时；每个连接器订阅变更消息突发 10 条、每 15 秒补 1 条。
- 币安 K 线按需补订只接受 `1m 3m 5m 15m 30m 1h 2h 4h 6h 8h 12h 1d 3d 1w 1M`（币安对错误级别和不存在的币对都应答成功，没法靠拒绝识别）。

### 8.3 日志串（target 固定为 `md_runtime_subs`）

| 级别 | 日志 | 含义 |
|---|---|---|
| INFO | `runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED (idle_minutes=…, check_interval_seconds=…, startup_grace_minutes=…, max_runtime_subscriptions=…)` | 开关打开（启动时或 SIGHUP） |
| INFO | `runtime subscriptions: idle unsubscribe and on-demand subscribe DISABLED (current subscriptions are kept as they are)` | 运行中关闭 |
| INFO | `runtime subscriptions: settings changed (…)` | 打开状态下改了参数 |
| INFO | `runtime subscription idle-unsubscribed: exchange=okx type=TICK symbol=SOL-USDT-SWAP interval=- idle=10m05s connector=okx` | 闲置退订（交易所、类型、币对、级别、闲置时长） |
| INFO | `runtime subscription added on demand: exchange=okx type=TICK symbol=SOL-USDT-SWAP interval=- receivers=1 connector=okx` | 按需补订 |
| WARN | `runtime subscription on-demand refused: limit reached (runtime=200, max_runtime_subscriptions=200) exchange=… type=… symbol=… interval=…` | 达到上限不补订（同一目标每个检查周期最多一行） |
| WARN | `runtime subscription rejected by upstream: exchange=… code=60018 msg=…; on-demand subscription withdrawn, no retry for 5m00s` | 按需补订被交易所拒绝，已撤回、退避 |
| WARN | `runtime subscription rejected by upstream: …; registered via REST/gRPC so it is kept, on-demand will not retry it for …` | POST 登记的目标被拒：保留（与以前一样），按需补订退避 |
| WARN | `config subscription rejected by upstream: … (config subscriptions are never changed automatically)` | 配置名单里的目标被拒：不自动处理，请检查配置 |
| INFO（target `md_server`） | `runtime_subscriptions changed: {…} -> {…} (applied)` | SIGHUP 热更了这一段 |

**198 用 `RUST_LOG=warn`，上面的 INFO 都看不到**（WARN 照常可见）。想看退订 / 补订明细：重建容器时把环境变量改成
`RUST_LOG=warn,md_runtime_subs=info`（只打开本模块的 INFO，量很小：每次退订 / 补订一行）。不想动容器就看下面的指标。

### 8.4 怎么看指标

| 指标 | 类型 | 说明 |
|---|---|---|
| `md_runtime_subscriptions_enabled` | gauge | 开关当前状态 0/1。改配置后先看它 |
| `md_runtime_subscriptions` | gauge | 当前运行期订阅数（不在配置名单里的上游订阅）；开关关闭时不统计、为 0 |
| `md_runtime_subscriptions_idle_unsubscribed_total` | counter | 闲置退订累计 |
| `md_runtime_subscriptions_demand_subscribed_total` | counter | 按需补订累计 |
| `md_runtime_subscriptions_demand_failed_total` | counter | 按需补订被上游拒绝 / 发送失败累计 |
| `md_runtime_subscriptions_limit_rejected_total` | counter | 因达到上限而拒绝补订的次数 |

```bash
curl -s http://192.168.1.198:8081/metrics | grep -E '^md_runtime_subscriptions'
# 每条订阅的来源与最近使用时间（仅开关打开时出现这两个字段）
curl -s http://192.168.1.198:8081/api/v1/subscriptions | jq -c '.[] | {exchange,data_type,symbol,kline_interval,source,last_active_at}'
```

PromQL：`md_runtime_subscriptions`（数量曲线）；`increase(md_runtime_subscriptions_idle_unsubscribed_total[1h])`、
`increase(md_runtime_subscriptions_demand_subscribed_total[1h])`（每小时退订 / 补订次数）。同一话题来回"退订 → 补订"很频繁，说明 `idle_minutes` 偏短或消费者订阅抖动。
`increase(md_runtime_subscriptions_limit_rejected_total[10m]) > 0` 应告警：有消费者订阅了却补订不上。

### 8.5 与 POST / DELETE / GET 的关系

- POST 照旧登记，视为运行期订阅，计入闲置判定（登记后没人接收，同样会在闲置到期后退订）。
- DELETE 照旧可用；对已被自动退订的目标返回 `{"success":true}`。**开关打开时，DELETE 一个仍有接收方的目标会被按需补订立刻补回**——
  这是为了不切断其他使用方（以前 DELETE 是全局的，会切断 OMS）。
- GET 照旧列出当前订阅；开关打开时每条追加 `source`（`config` / `runtime`）和 `last_active_at`（Unix 毫秒，当前有接收端为当前时间，开关打开以来没人用过为 `null`）。
  `compare.mjs subs` 会把追加字段算作差异，所以兼容对比要在开关关闭时做。

### 8.6 怎么打开 / 临时关闭

配置烧在镜像里（`/etc/md-server/config.yaml`，root 所有、容器以 `md-user` 运行），两种方式：

1. **正式变更（推荐，需业务方批准）**：用打开（或关闭）开关的配置构建新镜像，按 SWAP_PLAN 换容器，换后立即 `restore_subs.mjs`。重启会断开所有消费者一次、清空运行期订阅。
2. **不重启的临时开关（紧急关闭用）**：改容器内的文件后发 SIGHUP，立即生效，不断连、不丢运行期订阅：

   ```bash
   # 关闭：把 idle_unsubscribe 改成 false（以 root 身份在容器内改文件）
   docker exec -u root md-server sed -i 's/^\(\s*idle_unsubscribe:\s*\)true/\1false/' /etc/md-server/config.yaml
   docker kill --signal=HUP md-server
   curl -s http://127.0.0.1:8081/metrics | grep '^md_runtime_subscriptions_enabled'   # 应为 0
   ```

   **注意**：容器重建或重启（含 `unless-stopped` 自动拉起）后回到镜像里的配置；临时关闭后要尽快出一个关闭开关的镜像。
   关闭后已经被退掉的订阅不会自动恢复（有接收方的话题需要 POST 或换版后 `restore_subs.mjs` 补回），按需补订加上的订阅也保持原样。

### 8.7 排障

1. **消费者说某币对没数据**：`GET /api/v1/subscriptions` 里有没有该目标？没有 → 看 `md_runtime_subscriptions_limit_rejected_total` 是否在涨（上限）、
   WARN 日志里有没有 `rejected by upstream`（交易所拒绝，退避中）；话题写法是否规范（交易所小写、币对大写，K 线级别与交易所一致，如 OKX `1H`）。
2. **运行期订阅只增不减**：`md_runtime_subscriptions_enabled` 是否为 1；是否还在保护期；有没有消费者一直订着（`last_active_at` 接近当前时间）或一直在用 REST 轮询。
3. **订阅来回抖动**：`increase(..._idle_unsubscribed_total[1h])` 与 `increase(..._demand_subscribed_total[1h])` 都很高 → 调大 `idle_minutes`。

