# 生产运维部署指南（补充）

> 本文档补充 `DEPLOYMENT.md`，聚焦 Docker 生产部署、热重载真实能力和 CI/CD 实践。

---

## 一、热重载（SIGHUP）真实能力清单

从代码 `crates/md-server/src/main.rs` 的 `wait_for_signal()` 和 `apply_subscription_changes()` 函数分析，**SIGHUP 热重载的实际能力如下**：

### ✅ 真正支持热更（无需重启）

| 配置项 | 是否热更 | 说明 |
|--------|---------|------|
| **订阅列表** | ✅ | `subscribe_ticks`、`subscribe_klines` 的增删会调用 `add_subscriptions` / `remove_subscriptions`，实时生效 |
| **quote 订阅列表**（1.1.0） | ✅（有前提） | `connectors.{binance,okx}.subscribe_quotes` 的增删实时生效，**前提**是对应 quote 连接器已在运行（启动时 `quotes_enabled=true` 且列表非空）；否则日志提示"quote connector is not running ... requires restart" |
| `log_level` | ⚠️ **只能检测，无法真正生效** | tracing 不支持运行时改 level，代码里只记录了 `"log_level: info -> debug (applied)"` 日志，实际日志级别不会变 |

### ❌ 不支持热更（修改后提示需要重启）

| 配置项 | 说明 |
|--------|------|
| `grpc_server.listen_address` | 端口已绑定，无法热更 |
| `api_gateway.listen_address` | 同上 |
| `connectors.{binance,okx}.enabled` | 连接器启停需要重建 |
| `connectors.{binance,okx}.stream_base_url` | URL 变更需要重建连接 |
| `connectors.{binance,okx}.stream_base_url_public` | 同上 |
| `connectors.{binance,okx}.quotes_enabled`（1.1.0） | quote 连接器启停需要重建；变更时日志会提示 `quotes_enabled changed ... (requires restart)` |
| `processor.quote_channel_buffer`（1.1.0） | 同 tick/kline 的通道 buffer，无法 resize |
| `processor.tick_channel_buffer` | 通道 buffer 已创建，无法 resize |
| `processor.kline_channel_buffer` | 同上 |
| `processor.broadcast_capacity` | broadcast channel 已创建，无法 resize |
| `reconnect_delay`、`ping_interval` | 当前实现不读取运行时变更 |

### 使用方式

```bash
# 1. 修改 config.yaml（比如新增一个订阅币对）
vim /opt/md-server/config.yaml

# 2. 发送 SIGHUP 信号
kill -HUP $(docker inspect --format='{{.State.Pid}}' md-server)
# 或 systemd 方式：
sudo systemctl kill -s HUP md-server

# 3. 查看日志确认
# 预期输出：
# "subscription lists changed, applying..."
# "binance: adding 1 subscriptions"
# "config reloaded and applied successfully"
```

### 与 Go 版热重载对比

| 能力 | Go 原版 | Rust 版 |
|------|---------|---------|
| 订阅列表热更 | ✅（通过连接器重建实现） | ✅（直接 add/remove，更轻量） |
| 连接器重建 | ✅（Stop旧→新建→Start新） | ❌（不支持） |
| log_level | ✅（viper 支持运行时改） | ⚠️（检测但不生效） |
| 监听地址 | ❌ | ❌ |
| 缓冲区大小 | ❌ | ❌ |

**结论**：Rust 版的热重载在订阅列表变更上比 Go 版更轻量（不需要重建连接），但 Go 版支持完整的连接器重建和日志级别热更。Rust 版 README 的描述过于保守，实际上订阅热更已经做得很好了。

---

## 二、Docker 生产部署

### 2.1 构建生产镜像

项目已包含 `Dockerfile`（多阶段构建）：

```bash
# 本地构建
docker build -t market-data-server-rust:latest .

# 验证镜像大小
docker images market-data-server-rust:latest
# 预期: ~80-120MB（debian:bookworm-slim 基础）
```

### 2.2 快速启动（单容器）

```bash
docker run -d \
  --name md-server \
  --restart unless-stopped \
  -p 8081:8081 \
  -p 50051:50051 \
  -v $(pwd)/config.yaml:/etc/md-server/config.yaml:ro \
  -e RUST_LOG=warn \
  -e TZ=Asia/Shanghai \
  --memory=512m \
  --cpus=2 \
  ghcr.io/dfbtcice/market-data-server-rust:latest
```

### 2.3 生产编排（Docker Compose + 监控）

使用 `docker-compose.prod.yml`：

```bash
# 1. 准备目录
mkdir -p /opt/md-server && cd /opt/md-server

# 2. 拉取 compose 文件和配置
curl -L -o docker-compose.prod.yml \
  https://raw.githubusercontent.com/DFBTCice/market-data-server-rust/main/docker-compose.prod.yml

# 3. 创建配置文件（按需修改）
cp /path/to/your/config.yaml ./config.yaml

# 4. 拉取最新镜像并启动
docker-compose -f docker-compose.prod.yml pull
docker-compose -f docker-compose.prod.yml up -d

# 5. 验证
curl http://localhost:8081/health
curl http://localhost:8081/metrics | head -20
```

启动后访问：
- REST API: `http://<host>:8081`
- Prometheus: `http://<host>:9090`
- Grafana: `http://<host>:3000` (默认账号 admin/admin)

### 2.4 自动化部署脚本

使用 `deploy.sh`：

```bash
# 部署最新版本
chmod +x deploy.sh
./deploy.sh latest

# 部署指定版本
./deploy.sh v1.2.3

# 使用本地构建的镜像
./deploy.sh latest --local
```

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
   - 自动打标签：`v1.2.3`、`1.2`、`1`、`<sha>`
   - 使用 Docker layer cache (`type=gha`) 加速
2. **Build Release Binary**: 构建 `x86_64-unknown-linux-gnu` 和 `aarch64-unknown-linux-gnu` 两个架构的二进制
3. **Create GitHub Release**: 自动创建 Release，附带二进制压缩包

### 4.3 使用流程

```bash
# 1. 开发完成，合并到 main
# CI 自动跑测试和检查

# 2. 打标签触发发布
git tag v1.0.0
git push origin v1.0.0

# 3. GitHub Actions 自动:
#    - 构建 Docker 镜像并推送到 ghcr.io
#    - 构建多架构二进制并创建 Release

# 4. 服务器上部署最新版本
ssh your-server
./deploy.sh v1.0.0
```

### 4.4 回滚流程

```bash
# 紧急回滚到上一个版本
./deploy.sh v0.9.0

# 或直接用 Docker
docker-compose -f docker-compose.prod.yml down
docker pull ghcr.io/dfbtcice/market-data-server-rust:v0.9.0
docker tag ghcr.io/dfbtcice/market-data-server-rust:v0.9.0 \
            ghcr.io/dfbtcice/market-data-server-rust:latest
docker-compose -f docker-compose.prod.yml up -d
```

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
| 内存过高 | `docker stats md-server` | 先看 [七、内存监控与客户端卡顿](#七内存监控与客户端卡顿111-起)：多数是消费者卡顿后 musl 分配器不归还内存（RSS 只升不降）；重建容器可清零。调小 `broadcast_capacity` 只能降低单次卡顿的上限，不是根治 |
| SIGHUP 无效 | `docker kill --signal=HUP md-server` | Docker 容器中 kill -HUP 可能需要 docker kill |

---

## 七、内存监控与客户端卡顿（1.1.1 起）

### 7.1 一句话结论（详见 `evidence/1.1.1/INVESTIGATION_REPORT.md`）

现网 RSS"只升不降、呈台阶状"的主因不是慢速泄漏，而是：**消费者端卡顿（停止读 socket）→ 服务端该连接的写操作永久阻塞
→ 各主题 broadcast 环（容量 `broadcast_capacity`，默认 4096）里的事件无法被释放而滞留 → 滞留期间申请的内存在
musl 分配器下不会归还给系统，RSS 就停在"历史高水位"**。同样的代码换 jemalloc 分配器，卡顿结束后内存会回落。
1.1.1 起 Gateway（`/ws/v1/data`）按已有配置项 `api_gateway.ws_write_wait`（默认 `10s`）给每次写入加超时，
卡顿超过该时长的客户端被断开并释放其全部订阅（计数 `md_ws_kicked_write_timeout_total`）。

> **行为变化（仅此一项，需知会消费者）**：1.1.0 及以前该配置项被解析但从未生效；1.1.1 起，Gateway 客户端若 `ws_write_wait`
> 内读不动数据（进程挂起 / 半开连接 / 严重卡顿）会被服务端断开，需自行重连并重新订阅。想保持旧行为：`ws_write_wait: "0s"`。
> Legacy `/ws` 不受影响（本版未改动，同样存在"停读客户端钉住连接"的风险）。

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
| `md_ws_kicked_write_timeout_total` | 因写超时被踢的 Gateway 客户端累计数。**增长说明有消费者卡顿**——去查消费者（GC/阻塞/网络），不是服务端问题 |
| `md_ws_kicked_lagged_total` / `md_broadcast_lagged_total` | 消费者跟不上被踢 / 环溢出丢事件 |
| `md_ticks_dropped` / `md_klines_dropped` / `md_quotes_dropped` | 入口队列满丢弃。1.1.1 起日志里会限频输出 WARN（含时间与队列深度） |
| `md_tokio_alive_tasks` | 重连前后应回到同一水平（22 左右），只增不减 = task 泄漏 |

Prometheus 告警示例（与 DEPLOYMENT §7 的 500MB 提示规则并存，这条更早）：

```yaml
- alert: MdServerMemoryHigh
  expr: process_resident_memory_bytes{job="md-server"} > 400 * 1024 * 1024
  for: 10m
  labels: { severity: warning }
  annotations:
    summary: "md-server RSS > 400MiB，请在低峰期重建容器（限额 512MiB）"
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
3. RSS 在"平台 + 台阶"形态（不是持续斜线）→ 高水位，不是泄漏；重建容器可清零；根治靠消费者别卡 + 1.1.1 的写超时。
4. 需要区分"逻辑滞留 vs 分配器"时，用诊断构建（`--features md-server/jemalloc-stats`，**不用于生产镜像**）看 `md_jemalloc_allocated_bytes`（持平）与 `md_jemalloc_resident_bytes`。
5. `MD_DEBUG_FORCE_RECONNECT_SECS` / `MD_DEBUG_FORCE_RECONNECT_ONLY` 是**仅限旁路实验**的调试开关（强制周期性重连），不要在生产设置。

