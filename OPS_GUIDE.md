# 生产运维部署指南（补充）

> 本文档补充 `DEPLOYMENT.md`，聚焦 Docker 生产部署、热重载真实能力和 CI/CD 实践。

---

## 一、热重载（SIGHUP）真实能力清单

从代码 `crates/md-server/src/main.rs` 的 `wait_for_signal()` 和 `apply_subscription_changes()` 函数分析，**SIGHUP 热重载的实际能力如下**：

### ✅ 真正支持热更（无需重启）

| 配置项 | 是否热更 | 说明 |
|--------|---------|------|
| **订阅列表** | ✅ | `subscribe_ticks`、`subscribe_klines` 的增删会调用 `add_subscriptions` / `remove_subscriptions`，实时生效 |
| `log_level` | ⚠️ **只能检测，无法真正生效** | tracing 不支持运行时改 level，代码里只记录了 `"log_level: info -> debug (applied)"` 日志，实际日志级别不会变 |

### ❌ 不支持热更（修改后提示需要重启）

| 配置项 | 说明 |
|--------|------|
| `grpc_server.listen_address` | 端口已绑定，无法热更 |
| `api_gateway.listen_address` | 同上 |
| `connectors.{binance,okx}.enabled` | 连接器启停需要重建 |
| `connectors.{binance,okx}.stream_base_url` | URL 变更需要重建连接 |
| `connectors.{binance,okx}.stream_base_url_public` | 同上 |
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
| 内存过高 | `docker stats md-server` | 减小 `broadcast_capacity` 和 channel buffer |
| SIGHUP 无效 | `docker kill --signal=HUP md-server` | Docker 容器中 kill -HUP 可能需要 docker kill |
