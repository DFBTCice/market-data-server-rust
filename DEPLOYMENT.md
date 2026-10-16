# 生产环境部署指南

## 目录

- [1. 系统要求](#1-系统要求)
- [2. 快速部署](#2-快速部署)
- [3. systemd 服务部署（推荐）](#3-systemd-服务部署推荐)
- [4. Docker 部署](#4-docker-部署)
- [5. 日志管理](#5-日志管理)
- [6. 配置说明](#6-配置说明)
- [7. 监控与健康检查](#7-监控与健康检查)
- [8. 故障排查](#8-故障排查)
- [9. 性能优化](#9-性能优化)

---

## 1. 系统要求

### 最低配置

| 资源 | 要求 |
|------|------|
| CPU | 2 核 |
| 内存 | 512 MB |
| 磁盘 | 1 GB |
| 网络 | 稳定的互联网连接（访问 Binance/OKX WebSocket） |

### 推荐配置

| 资源 | 要求 |
|------|------|
| CPU | 4 核 |
| 内存 | 2 GB |
| 磁盘 | 10 GB（日志存储） |
| 网络 | 低延迟专线（亚洲节点优先） |

### 操作系统

- Linux (推荐 Ubuntu 22.04+, CentOS 8+, Debian 11+)
- macOS (开发环境)
- Windows (不推荐)

---

## 2. 快速部署

### 2.1 编译

```bash
# 克隆项目
git clone <repository-url>
cd rest重构

# 编译 release 版本
cargo build --release

# 二进制文件位置
ls -lh target/release/md-server
```

#### 2.1.1 交叉编译静态二进制（macOS → x86_64 Linux，无需在 Linux 上编译）

适用于"开发机是 macOS、目标是 x86_64 Linux 服务器，且不想用 Docker 编译"的场景。
产物为**全静态（musl）、零运行期依赖**的 ELF，可直接拷到任意 x86_64 Linux 运行：

```bash
# 一次性准备工具链
brew install zig                       # 提供跨平台 C 链接器
cargo install cargo-zigbuild
rustup target add x86_64-unknown-linux-musl

# 编译；vendored-tls 让 OpenSSL 随源码静态编译（md-connector 的可选 feature，默认关闭）
cargo zigbuild -p md-server --release \
  --target x86_64-unknown-linux-musl \
  --features md-connector/vendored-tls

# 产物：target/x86_64-unknown-linux-musl/release/md-server
file target/x86_64-unknown-linux-musl/release/md-server
#   ELF 64-bit LSB executable, x86-64, statically linked, stripped
```

> 拷到服务器后即可按 §2.2 / §3 部署，目标机**无需安装** Rust、glibc 版本、libssl 等任何依赖。

### 2.2 部署到服务器

```bash
# 创建目录
sudo mkdir -p /opt/md-server/bin
sudo mkdir -p /etc/md-server
sudo mkdir -p /var/log/md-server

# 复制文件
sudo cp target/release/md-server /opt/md-server/bin/
sudo cp config.yaml /etc/md-server/

# 创建用户
sudo useradd -r -s /bin/false md-user
sudo chown -R md-user:md-user /opt/md-server
sudo chown -R md-user:md-user /var/log/md-server
```

### 2.3 启动

```bash
# 直接启动
/opt/md-server/bin/md-server --config /etc/md-server/config.yaml

# 后台启动
nohup /opt/md-server/bin/md-server --config /etc/md-server/config.yaml > /var/log/md-server/output.log 2>&1 &
```

---

## 3. systemd 服务部署（推荐）

### 3.1 创建服务文件

```bash
sudo nano /etc/systemd/system/md-server.service
```

```ini
[Unit]
Description=Market Data Server - Binance & OKX
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=md-user
Group=md-user
WorkingDirectory=/opt/md-server
ExecStart=/opt/md-server/bin/md-server --config /etc/md-server/config.yaml

# 环境变量
Environment=RUST_LOG=warn
Environment=TZ=Asia/Shanghai

# 日志输出到 journald
StandardOutput=journal
StandardError=journal
SyslogIdentifier=md-server

# 重启策略
Restart=always
RestartSec=5
StartLimitIntervalSec=60
StartLimitBurst=3

# 资源限制
LimitNOFILE=65536
LimitNPROC=4096

# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/log/md-server

[Install]
WantedBy=multi-user.target
```

### 3.2 启动服务

```bash
# 重新加载 systemd 配置
sudo systemctl daemon-reload

# 启动服务
sudo systemctl start md-server

# 设置开机自启
sudo systemctl enable md-server

# 查看状态
sudo systemctl status md-server
```

### 3.3 管理命令

```bash
# 启动
sudo systemctl start md-server

# 停止
sudo systemctl stop md-server

# 重启
sudo systemctl restart md-server

# 查看状态
sudo systemctl status md-server

# 查看日志
sudo journalctl -u md-server -f              # 实时查看
sudo journalctl -u md-server -n 100          # 最近 100 行
sudo journalctl -u md-server --since today   # 今天的日志
sudo journalctl -u md-server --since "2026-05-05 10:00" --until "2026-05-05 12:00"  # 时间范围
```

---

## 4. Docker 部署

### 4.1 Dockerfile

```dockerfile
# 多阶段构建
FROM rust:1.75-slim as builder

WORKDIR /app
COPY . .
RUN cargo build --release

# 运行阶段
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 创建用户
RUN useradd -r -s /bin/false md-user

# 复制二进制
COPY --from=builder /app/target/release/md-server /usr/local/bin/

# 创建目录
RUN mkdir -p /etc/md-server /var/log/md-server \
    && chown -R md-user:md-user /var/log/md-server

# 复制配置
COPY config.yaml /etc/md-server/

# 切换用户
USER md-user

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

EXPOSE 8081 50051

CMD ["md-server", "--config", "/etc/md-server/config.yaml"]
```

### 4.2 构建镜像

```bash
docker build -t md-server:latest .
```

### 4.3 运行容器

```bash
# 基本运行
docker run -d \
  --name md-server \
  -p 8081:8081 \
  -p 50051:50051 \
  -v /path/to/config.yaml:/etc/md-server/config.yaml:ro \
  -v /var/log/md-server:/var/log/md-server \
  md-server:latest

# 带环境变量
docker run -d \
  --name md-server \
  -p 8081:8081 \
  -p 50051:50051 \
  -e RUST_LOG=warn \
  -e TZ=Asia/Shanghai \
  -v /path/to/config.yaml:/etc/md-server/config.yaml:ro \
  md-server:latest

# 限制资源
docker run -d \
  --name md-server \
  --memory=512m \
  --cpus=2 \
  -p 8081:8081 \
  -p 50051:50051 \
  -v /path/to/config.yaml:/etc/md-server/config.yaml:ro \
  md-server:latest
```

### 4.4 Docker Compose

```yaml
version: '3.8'

services:
  md-server:
    build: .
    container_name: md-server
    restart: unless-stopped
    ports:
      - "8081:8081"
      - "50051:50051"
    volumes:
      - ./config.yaml:/etc/md-server/config.yaml:ro
      - md-logs:/var/log/md-server
    environment:
      - RUST_LOG=warn
      - TZ=Asia/Shanghai
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 512M
        reservations:
          cpus: '1'
          memory: 256M

volumes:
  md-logs:
```

```bash
# 启动
docker-compose up -d

# 查看日志
docker-compose logs -f md-server

# 停止
docker-compose down
```

### 4.5 国内/内网镜像加速构建（Dockerfile.cn）

当 Docker 守护进程所在主机**无法稳定访问 Docker Hub / crates.io**（如国内服务器）时，
直接用标准 `Dockerfile` 会在 `FROM rust:...` 或下载 crates 时超时。仓库内附带等价的
`Dockerfile.cn`，仅把外部源替换为国内镜像，逻辑与 `Dockerfile` 完全一致：

| 资源 | 标准 Dockerfile | Dockerfile.cn |
|------|-----------------|---------------|
| 基础镜像 | `rust` / `debian`（Docker Hub） | `docker.m.daocloud.io/library/...` |
| apt 源 | `deb.debian.org` | `mirrors.aliyun.com` |
| crates 源 | `crates.io` | `rsproxy.cn` 稀疏索引 |

```bash
docker build -t md-server:latest -f Dockerfile.cn .
```

### 4.6 通过 Portainer API 远程一键替换部署

无法直接登录目标主机、但有 Portainer 时，可用其代理的 Docker API 远程构建并替换容器。
以下为本项目实际使用、可复制的"零停机回滚友好"流程（`EP` 为端点 ID，本机部署一般为 `local`）：

```bash
PORTAINER=http://<portainer-host>:9000
EP=3   # GET /api/endpoints 查询目标端点 ID

# 1. 认证拿 JWT
JWT=$(curl -s -X POST "$PORTAINER/api/auth" \
  -H 'Content-Type: application/json' \
  -d '{"Username":"admin","Password":"<password>"}' | python3 -c "import sys,json;print(json.load(sys.stdin)['jwt'])")
B="$PORTAINER/api/endpoints/$EP/docker"

# 2. 打包构建上下文（macOS 必须去掉扩展属性，否则远端解包报 xattr 错误）
COPYFILE_DISABLE=1 tar --no-xattrs --no-mac-metadata \
  --exclude='./target' --exclude='./.git' --exclude='./.DS_Store' -cf /tmp/ctx.tar .

# 3. 在远端原生构建镜像（架构与目标主机一致，避免跨架构）
curl -s -X POST "$B/build?t=md-server:latest&dockerfile=Dockerfile.cn" \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/x-tar' \
  --data-binary @/tmp/ctx.tar

# 4. 旧容器改名+停止（保留作回滚），再用新镜像创建同名容器
curl -s -X POST "$B/containers/md-server/rename?name=md-server-bak" -H "Authorization: Bearer $JWT"
curl -s -X POST "$B/containers/md-server-bak/stop"                  -H "Authorization: Bearer $JWT"
curl -s -X POST "$B/containers/create?name=md-server" -H "Authorization: Bearer $JWT" \
  -H 'Content-Type: application/json' -d '{
    "Image":"md-server:latest",
    "Env":["RUST_LOG=warn","TZ=Asia/Shanghai"],
    "ExposedPorts":{"8081/tcp":{},"50051/tcp":{}},
    "HostConfig":{
      "PortBindings":{"8081/tcp":[{"HostPort":"8081"}],"50051/tcp":[{"HostPort":"50051"}]},
      "RestartPolicy":{"Name":"unless-stopped"},
      "Memory":536870912,"NanoCpus":2000000000,"NetworkMode":"bridge"
    }}'
# 取返回的 Id 后启动
curl -s -X POST "$B/containers/<new-id>/start" -H "Authorization: Bearer $JWT"

# 5. 验证 healthy 后删除回滚备份
curl -s http://<portainer-host>:8081/health        # {"status":"ok"}
curl -s -X DELETE "$B/containers/md-server-bak?force=true" -H "Authorization: Bearer $JWT"
```

> 配置 `config.yaml` 已在镜像构建阶段 `COPY` 进 `/etc/md-server/`，无需额外挂载卷。

---

## 5. 日志管理

### 5.1 日志级别

| 级别 | 说明 | 使用场景 |
|------|------|----------|
| `error` | 错误信息 | 生产环境（最小日志量） |
| `warn` | 警告信息 | 生产环境（推荐） |
| `info` | 一般信息 | 测试环境 |
| `debug` | 调试信息 | 开发环境 |

### 5.2 配置日志级别

**Go 版本** (config.yaml):
```yaml
log_level: "warn"
```

**Rust 版本**:
```bash
# 环境变量（优先级最高）
export RUST_LOG=warn

# 或在 config.yaml 中
log_level: "warn"
```

### 5.3 日志轮转

#### 使用 logrotate

```bash
# 创建 logrotate 配置
sudo nano /etc/logrotate.d/md-server
```

```
/var/log/md-server/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 0644 md-user md-user
    sharedscripts
    postrotate
        systemctl reload md-server > /dev/null 2>&1 || true
    endscript
}
```

#### 使用 journald 持久化

```bash
# 编辑 journald 配置
sudo nano /etc/systemd/journald.conf
```

```ini
[Journal]
Storage=persistent
SystemMaxUse=2G
SystemMaxFileSize=100M
MaxRetentionSec=30day
```

```bash
# 重启 journald
sudo systemctl restart systemd-journald
```

### 5.4 集中式日志收集

#### ELK Stack (Elasticsearch + Logstash + Kibana)

```yaml
# filebeat.yml
filebeat.inputs:
  - type: journald
    id: md-server
    paths:
      - /var/log/journal

output.elasticsearch:
  hosts: ["elasticsearch:9200"]
  index: "md-server-%{+yyyy.MM.dd}"
```

#### Loki + Grafana

```yaml
# promtail-config.yml
server:
  http_listen_port: 9080

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://loki:3100/loki/api/v1/push

scrape_configs:
  - job_name: md-server
    journal:
      json: false
      max_age: 12h
      labels:
        job: md-server
    relabel_configs:
      - source_labels: ['__journal__systemd_unit']
        target_label: 'unit'
```

---

## 6. 配置说明

### 6.1 完整配置示例

```yaml
# 日志级别: "debug", "info", "warn", "error"
log_level: "warn"

# gRPC 服务器配置
grpc_server:
  enabled: true
  listen_address: ":50051"
  read_timeout: "10s"
  write_timeout: "10s"

# 数据处理器配置
processor:
  tick_channel_buffer: 1000
  kline_channel_buffer: 1000

# 连接器配置
connectors:
  binance:
    enabled: true
    stream_base_url: "wss://fstream.binance.com/market/stream"
    rest_base_url: "https://fapi.binance.com"

    subscribe_ticks:
      - "BTCUSDT"
      - "ETHUSDT"

    subscribe_klines:
      "5m":
        - "BTCUSDT"
        - "ETHUSDT"

    reconnect_delay: "5s"
    ping_interval: "3m"

  okx:
    enabled: true
    stream_base_url_public: "wss://ws.okx.com:8443/ws/v5/public"
    stream_base_url_business: "wss://ws.okx.com:8443/ws/v5/business"
    rest_base_url: "https://www.okx.com"

    subscribe_ticks:
      - "BTC-USDT-SWAP"
      - "ETH-USDT-SWAP"

    subscribe_klines:
      "5m":
        - "BTC-USDT-SWAP"
        - "ETH-USDT-SWAP"

    reconnect_delay: "10s"
    ping_interval: "25s"

# API 网关配置
api_gateway:
  enabled: true
  listen_address: ":8081"
  market_data_grpc_target: "localhost:50051"
  admin_grpc_target: "localhost:50052"
  ws_ping_period: "30s"
  ws_write_wait: "10s"
  ws_max_message_size: 1024
```

### 6.2 环境变量覆盖

```bash
# 覆盖日志级别（只能写 trace / debug / info / warn / error；设置了 RUST_LOG 时以 RUST_LOG 为准）
export LOG_LEVEL=debug

# 覆盖端口
export GRPC_LISTEN_ADDRESS=:50052
export API_GATEWAY_LISTEN_ADDRESS=:8082

# 覆盖连接器配置
export CONNECTORS_BINANCE_ENABLED=true
export CONNECTORS_OKX_ENABLED=false
```

### 6.3 多实例部署

```bash
# 实例 1 - Binance 数据
./md-server --config config-binance.yaml --port-offset 0

# 实例 2 - OKX 数据
./md-server --config config-okx.yaml --port-offset 100

# 端口自动偏移：
# 实例 1: gRPC :50051, REST :8081
# 实例 2: gRPC :50151, REST :8181
```

---

## 7. 监控与健康检查

### 7.1 健康检查端点

```bash
# 健康检查
curl http://localhost:8081/health

# 响应示例
{
  "status": "ok",
  "service": "md-server",
  "timestamp": "2026-05-05T10:30:00Z"
}
```

### 7.2 Prometheus 指标全集

服务在 `:8081/metrics` 暴露 Prometheus exposition 格式。完整指标矩阵（与 Go 版 dashboard 一一对应）：

```bash
curl http://localhost:8081/metrics
```

#### 7.2.1 数据吞吐与处理

| 指标名 | 类型 | 标签 | 含义 | 对应 Go 面板 |
|---|---|---|---|---|
| `md_ticks_processed` | counter | — | 入库 Tick 累计（全局，顶部 stat 备用） | 数据吞吐量 |
| `md_klines_processed` | counter | — | 入库 Kline 累计（全局，顶部 stat 备用） | 数据吞吐量 |
| `md_ticks_dropped` | counter | — | mpsc 满丢弃的 Tick | 消息丢弃率 |
| `md_klines_dropped` | counter | — | mpsc 满丢弃的 Kline | 消息丢弃率 |

> 吞吐量推荐直接用核心多维指标的 `_count`（见 7.2.2），可任意按交易所/标的/周期聚合。

#### 7.2.2 入库延迟 / 吞吐（核心多维 histogram）

**一个指标，多维聚合**，完全对标 Go 版 `marketdata_processor_ingestion_latency_ms`：

| 指标名 | 标签 | 含义 |
|---|---|---|
| `md_ingestion_latency_ms_*` | `exchange, type, symbol, interval` | 入库延迟（exchange_event_ts → connector_receive_ts）。`type`=tick/kline；Tick 的 `interval` 为空 |

由它聚合出 Go 版所有延迟/吞吐面板：

```promql
# 按交易所（面板：Tick/Kline 采集延迟）
histogram_quantile(0.99, sum by (exchange,le) (rate(md_ingestion_latency_ms_bucket{type="tick"}[5m])))

# 按交易对（标的）（面板：Tick 采集延迟 — 按交易对）
histogram_quantile(0.99, sum by (exchange,symbol,le) (rate(md_ingestion_latency_ms_bucket{type="tick"}[5m])))

# 按交易对 + 周期（面板：Kline 采集延迟 — 按交易对 + 周期）
histogram_quantile(0.99, sum by (exchange,symbol,interval,le) (rate(md_ingestion_latency_ms_bucket{type="kline"}[5m])))

# 吞吐量（用 _count）：按交易所+类型 / 按标的
sum by (exchange,type) (rate(md_ingestion_latency_ms_count[1m]))
topk(10, sum by (exchange,symbol,type) (rate(md_ingestion_latency_ms_count[1m])))
```

#### 7.2.3 网关内部延迟（按 topic）

| 指标名 | 标签 | 含义 |
|---|---|---|
| `md_gateway_internal_latency_ms_*` | `topic` | 网关内部转发延迟（publish→client send 完成），对标 Go `marketdata_gateway_internal_latency_ms` |

```promql
# 总体延迟（面板：网关内部延迟）
histogram_quantile(0.99, sum by (le) (rate(md_gateway_internal_latency_ms_bucket[5m])))
# 按 topic 推送吞吐 TOP10（面板：网关推送吞吐量 TOP10）
topk(10, sum by (topic) (rate(md_gateway_internal_latency_ms_count[1m])))
```

桶边界：入库延迟 1ms~5s（11 桶），网关延迟 1ms~500ms（9 桶）。

> 基数说明：本系统只订阅自有策略所需的少量标的，`md_ingestion_latency_ms` 与 `md_gateway_internal_latency_ms` 的序列数 = 交易所 × 标的 × 周期，通常数十条，基数完全可控。

#### 7.2.4 连接器健康

| 指标名 | 类型 | 标签 | 含义 |
|---|---|---|---|
| `md_connector_connected` | gauge (0/1) | `exchange` | 连接状态 |
| `md_connector_reconnect_total` | counter | `exchange` | 累计重连次数 |
| `md_connector_subscribe_failed_total` | counter | `exchange` | 累计订阅失败次数 |

#### 7.2.5 网关 / WebSocket

| 指标名 | 类型 | 标签 | 含义 | 对应 Go 面板 |
|---|---|---|---|---|
| `md_ws_active_clients` | gauge | — | 当前活跃 WS 连接数（gateway+legacy 合计） | WebSocket 活跃连接数变化 |
| `md_ws_kicked_lagged_total` | counter | — | 因连续 lagged 被踢出的客户端累计数 | 慢客户端踢出事件 |
| `md_ws_messages_sent_total` | counter | `kind` | 成功推送给客户端的消息累计 | 网关推送吞吐量 |
| `md_broadcast_lagged_total` | counter | `topic_kind` | broadcast 订阅者跟不上的次数 | 消息丢弃率（订阅者缓冲满） |
| `md_ws_subscriptions_active` | gauge | — | 所有 WS 连接（gateway+legacy）当前持有的 broadcast 接收端总数（**1.1.1 新增**）。退订/断线后必须回落；与 `md_ws_active_clients` 对照：单个连接订阅 N 个主题就贡献 N | —（新增） |
| `md_ws_kicked_write_timeout_total` | counter | — | Gateway 客户端因写入超过 `api_gateway.ws_write_wait`（默认 10s）被断开的累计数（**1.1.1 新增**）。增长 = 有消费者卡顿（停读/半开连接），去查消费者 | —（新增） |

> 说明：当某个 WS 客户端连续 3 次出现 broadcast lagged，会被自动踢出（`md_ws_kicked_lagged_total +1`），防止僵尸客户端拖累 broadcast channel。

#### 7.2.5b 运行时诊断（1.1.1 新增，只增不改）

| 指标名 | 类型 | 含义 |
|---|---|---|
| `md_tokio_alive_tasks` | gauge | tokio 当前存活 task 数。重连前后应回到同一水平，只增不减说明有 task 泄漏 |
| `md_tokio_workers` | gauge | tokio 工作线程数（受容器 CPU 限额影响：`--cpus=2` ⇒ 2） |
| `md_tokio_global_queue_depth` | gauge | tokio 全局注入队列深度（持续 > 0 说明运行时过载） |
| `md_jemalloc_{allocated,active,resident,retained,mapped,metadata}_bytes` | gauge | **仅在 `--features md-server/jemalloc-stats` 的构建中存在**（1.1.1 候选 A 的发布二进制含；默认构建不含）。`allocated` = 存活堆，应持平；`allocated` 持平而 `resident` 上升 = 分配器碎片；`allocated` 上升 = 逻辑滞留/泄漏 |

#### 7.2.6 进程级（标准 Prometheus，与 Go process_exporter 等价）

| 指标名 | 类型 | 含义 |
|---|---|---|
| `process_resident_memory_bytes` | gauge | RSS（常驻内存） |
| `process_virtual_memory_bytes` | gauge | VMS（虚拟内存） |
| `process_cpu_seconds_total` | counter | 累计 CPU 时间（user+sys） |
| `process_open_fds` | gauge | 当前打开 fd |
| `process_max_fds` | gauge | fd 上限 |
| `process_start_time_seconds` | gauge | 启动 UNIX 时间 |
| `process_uptime_seconds` | gauge | 运行时长（秒） |

> Linux 上从 `/proc/self/{stat,limits,fd}` 读取，零外部依赖；macOS 开发环境只暴露 `process_start_time_seconds` 和 `process_uptime_seconds`。

> Rust 无 GC，因此 Go 的 `Goroutine 数 / Heap / GC 耗时` 三项不再适用；如需 Tokio 任务数监控，可后续接入 `tokio-metrics`（实验性）。

### 7.3 Prometheus 配置

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'md-server'
    static_configs:
      - targets: ['localhost:8081']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

### 7.4 Grafana Dashboard

仓库已附带与 Go 版完全等价的 dashboard 模板：

```bash
# 路径
rest重构/dashboards/md-server-rust.json

# 导入步骤
# 1. 打开 Grafana → Dashboards → New → Import
# 2. 上传 md-server-rust.json
# 3. 选择 Prometheus 数据源
```

dashboard 包含以下分组（对照 Go 原 dashboard，含交易所 + 标的多维度）：

1. **🩺 系统健康总览** — 数据流状态 / 活跃 WS 连接数 / 慢客户端踢出 / 消息丢弃 / 广播 lagged / 数据吞吐量
2. **⏱ 延迟分析** — Tick / Kline / 网关 三视图，按交易所 P50/P99
3. **🔍 延迟分位数明细（按交易对 / 标的）** — Tick 按交易对、Kline 按交易对+周期 P50/P99
4. **📈 吞吐量分析** — 按交易所+类型、按交易对 TOP10、网关推送 TOP10（按 topic）
5. **🚨 异常事件监控** — 消息丢弃率 / 广播 lagged 速率 / 慢客户端踢出速率
6. **🔌 连接器健康** — 连接状态 / 重连次数 / 订阅失败
7. **🦀 Rust 进程指标** — CPU 使用率 / RSS+VMS / fd 数量 / Uptime

dashboard 顶部提供 `交易所` 和 `交易对（标的）` 两个下拉变量，可按标的过滤所有延迟/吞吐面板。

### 7.5 告警规则（生产建议）

```yaml
# alerting-rules.yml
groups:
  - name: md-server
    rules:
      # ---- P0 严重告警（影响数据流） ----
      - alert: ServerDown
        expr: up{job="md-server"} == 0
        for: 1m
        labels: { severity: critical }
        annotations:
          summary: "md-server 进程不可达"

      - alert: ConnectorDisconnected
        expr: md_connector_connected{job="md-server"} == 0
        for: 30s
        labels: { severity: critical }
        annotations:
          summary: "{{ $labels.exchange }} 连接器断开"

      - alert: NoDataReceived
        expr: rate(md_ticks_processed{job="md-server"}[5m]) == 0
        for: 2m
        labels: { severity: critical }
        annotations:
          summary: "5 分钟无 Tick 数据入库"

      - alert: TicksDropped
        expr: rate(md_ticks_dropped{job="md-server"}[1m]) > 0
        for: 1m
        labels: { severity: critical }
        annotations:
          summary: "Tick 因 mpsc 满被丢弃 -- 处理瓶颈"

      # ---- P1 警告（性能下降） ----
      - alert: HighIngestionLatency
        expr: histogram_quantile(0.99, sum by (exchange,type,le) (rate(md_ingestion_latency_ms_bucket[5m]))) > 500
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "{{ $labels.exchange }} {{ $labels.type }} P99 延迟 > 500ms"

      - alert: BroadcastLagged
        expr: rate(md_broadcast_lagged_total{job="md-server"}[5m]) > 0.1
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "broadcast 订阅者持续跟不上（topic_kind={{ $labels.topic_kind }}）"

      - alert: WsClientsKicked
        expr: rate(md_ws_kicked_lagged_total{job="md-server"}[5m]) > 0
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "WebSocket 客户端被踢出（持续 lagged）"

      - alert: HighReconnectRate
        expr: rate(md_connector_reconnect_total{job="md-server"}[10m]) > 0.05
        for: 10m
        labels: { severity: warning }
        annotations:
          summary: "{{ $labels.exchange }} 重连频繁（10m 内 > 30 次）"

      # ---- P2 提示（资源使用） ----
      - alert: MemoryApproachingLimit      # 1.1.1 新增：容器限额 512MiB 时的"提前量"告警（见 OPS_GUIDE §七）
        expr: process_resident_memory_bytes{job="md-server"} > 400 * 1024 * 1024
        for: 10m
        labels: { severity: warning }
        annotations:
          summary: "RSS > 400MiB：请在低峰期重建容器（musl 下 RSS 为历史高水位，不会自行回落）"

      - alert: HighMemoryUsage
        expr: process_resident_memory_bytes{job="md-server"} > 500000000
        for: 10m
        labels: { severity: info }
        annotations:
          summary: "RSS > 500MB（典型值应 < 100MB）"

      - alert: HighFdUsage
        expr: process_open_fds{job="md-server"} / process_max_fds{job="md-server"} > 0.8
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "fd 使用率 > 80%"

      - alert: GatewayForwardLatencyHigh
        expr: histogram_quantile(0.99, sum by (le) (rate(md_gateway_internal_latency_ms_bucket[5m]))) > 50
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "网关内部转发 P99 > 50ms（典型应 < 5ms）"
```

---

## 8. 故障排查

### 8.1 常见问题

#### 问题 1: 无法连接到交易所

```bash
# 检查网络
ping fstream.binance.com
ping ws.okx.com

# 检查 WebSocket 连接
curl -v "wss://fstream.binance.com/market/stream" 2>&1 | grep -i "upgrade"

# 检查防火墙
sudo iptables -L -n | grep -E "80|443|50051|8081"

# 检查 DNS
nslookup fstream.binance.com
```

#### 问题 2: 服务启动失败

```bash
# 查看详细错误
journalctl -u md-server -n 50 --no-pager

# 检查配置文件
md-server --config /etc/md-server/config.yaml --check

# 检查端口占用
sudo netstat -tlnp | grep -E "8081|50051"
sudo lsof -i :8081
```

#### 问题 3: 内存占用过高

```bash
# 查看内存使用
ps aux | grep md-server
top -p $(pgrep md-server)

# 检查连接数
curl http://localhost:8081/metrics | grep active_connections

# 调整缓冲区大小
# 在 config.yaml 中减小 processor.tick_channel_buffer
```

#### 问题 4: 数据延迟

```bash
# 检查交易所 WebSocket 延迟
curl http://localhost:8081/api/v1/data/latest/tick/binance/BTCUSDT | jq '.exchange_event_ts, .connector_receive_ts'

# 计算延迟
# connector_receive_ts - exchange_event_ts = 延迟（毫秒）
```

### 8.2 调试模式

```bash
# 启用调试日志
RUST_LOG=debug ./md-server --config config.yaml

# 只看特定模块
RUST_LOG=md_connector=debug ./md-server

# 看多个模块
RUST_LOG=md_connector=debug,md_processor=info ./md-server
```

### 8.3 性能分析

```bash
# CPU 分析
perf record -g ./md-server --config config.yaml
perf report

# 内存分析
valgrind --tool=massif ./md-server --config config.yaml

# Go pprof (Go 版本)
go tool pprof http://localhost:6060/debug/pprof/profile
```

---

## 9. 性能优化

### 9.1 系统优化

```bash
# 增加文件描述符限制
echo "* soft nofile 65536" | sudo tee -a /etc/security/limits.conf
echo "* hard nofile 65536" | sudo tee -a /etc/security/limits.conf

# 优化网络参数
sudo sysctl -w net.core.somaxconn=65535
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=65535
sudo sysctl -w net.core.netdev_max_backlog=65535

# 持久化配置
echo "net.core.somaxconn=65535" | sudo tee -a /etc/sysctl.conf
echo "net.ipv4.tcp_max_syn_backlog=65535" | sudo tee -a /etc/sysctl.conf
```

### 9.2 应用优化

```yaml
# config.yaml 优化
processor:
  tick_channel_buffer: 2000    # 增大缓冲区
  kline_channel_buffer: 2000

connectors:
  binance:
    reconnect_delay: "3s"      # 更快重连
    ping_interval: "2m"        # 更频繁心跳
```

### 9.3 编译优化

```toml
# Cargo.toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

```bash
# 编译优化版本
cargo build --release
```

---

## 10. 安全建议

### 10.1 网络安全

```bash
# 只允许本地访问 gRPC
listen_address: "127.0.0.1:50051"

# 使用防火墙限制访问
sudo ufw allow from 127.0.0.1 to any port 50051
sudo ufw allow from 127.0.0.1 to any port 8081
```

### 10.2 进程安全

```bash
# 使用非 root 用户运行
useradd -r -s /bin/false md-user

# systemd 安全配置
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
```

### 10.3 API 密钥安全

```bash
# 使用环境变量（不写入配置文件）
export CONNECTORS_BINANCE_API_KEY="your_api_key"
export CONNECTORS_BINANCE_SECRET_KEY="your_secret_key"

# 或使用密钥管理服务
# AWS Secrets Manager, HashiCorp Vault, etc.
```

---

## 11. 备份与恢复

### 11.1 备份

```bash
# 备份配置
tar -czf md-server-config-$(date +%Y%m%d).tar.gz /etc/md-server/

# 备份日志（可选）
tar -czf md-server-logs-$(date +%Y%m%d).tar.gz /var/log/md-server/
```

### 11.2 恢复

```bash
# 恢复配置
tar -xzf md-server-config-20260505.tar.gz -C /

# 重启服务
sudo systemctl restart md-server
```

---

## 12. 更新流程

```bash
# 1. 停止服务
sudo systemctl stop md-server

# 2. 备份当前版本
cp /opt/md-server/bin/md-server /opt/md-server/bin/md-server.bak

# 3. 编译新版本
cargo build --release

# 4. 部署新版本
sudo cp target/release/md-server /opt/md-server/bin/

# 5. 启动服务
sudo systemctl start md-server

# 6. 验证
curl http://localhost:8081/health

# 7. 回滚（如果需要）
sudo systemctl stop md-server
sudo cp /opt/md-server/bin/md-server.bak /opt/md-server/bin/md-server
sudo systemctl start md-server
```

### 12.1 升级到 1.1.0 并启用 quote（分阶段，可回滚）

1.1.0 新增 `quote.*` 报价主题（说明见 [README「Quote 报价主题」](README.md#quote-报价主题110-新增)），
默认关闭，**只增不改**：不改配置升级，行为与 1.0.0 一致。建议分三步，每步都可回滚。

**阶段 0：留基线（升级前）**

```bash
# 记录当前版本/配置、保留 1.0.0 的二进制或镜像（回滚用）
cp /opt/md-server/bin/md-server /opt/md-server/bin/md-server.1.0.0        # systemd 方式
cp /opt/md-server/config.yaml    /opt/md-server/config.yaml.1.0.0.bak
# Docker 方式：记下当前镜像 tag / digest，不要删旧镜像

# 抓 60 秒推送基线 + 订阅列表 + /metrics（升级后对比用）
node scripts/compat/capture_ws.mjs gateway ws://HOST:8081/ws/v1/data 60 baseline/ws-gateway tick.binance.BNBUSDT tick.okx.BNB-USDT-SWAP
node scripts/compat/capture_ws.mjs legacy  ws://HOST:8081/ws         60 baseline/ws-legacy  tick.binance.BNBUSDT
curl -s http://HOST:8081/api/v1/subscriptions > baseline/subscriptions.json
curl -s http://HOST:8081/metrics               > baseline/metrics.txt
```

**阶段 1：只升级、不改配置**

1. 部署 1.1.0（二进制或镜像），**沿用现有 `config.yaml`，不加任何新键**；
2. `curl http://HOST:8081/health`；
3. 对比：`/api/v1/subscriptions` 与基线一致（注意：通过 REST/gRPC 动态加的订阅不会写回配置，**重启后丢失**，以 `config.yaml` 为准）；
   再抓 60 秒推送，用 `node scripts/compat/compare.mjs push|subs|metrics|noquote` 对比，要求：
   tick/kline 去掉 `connector_receive_ts` 后逐字节一致、`/metrics` 只增不减；
4. 观察 10 分钟 `process_resident_memory_bytes` / `process_cpu_seconds_total` / `process_open_fds` 与升级前同量级；
5. 任何不一致：立即回滚到阶段 0 保留的 1.0.0。

**阶段 2：小范围启用 quote**

```yaml
connectors:
  binance: { quotes_enabled: true, subscribe_quotes: ["BNBUSDT"] }
  okx:     { quotes_enabled: true, subscribe_quotes: ["BNB-USDT-SWAP"] }
```

`quotes_enabled` 从 false→true 必须**重启**才能创建 quote 连接器（SIGHUP 只会提示需重启）。
验证：订阅 `quote.okx.BNB-USDT-SWAP` 抓 60 秒，相邻推送间隔应 ≤2 秒、`bid/ask` 非空；
同时再次对比 tick/kline（逐字节一致）、旧 `/ws` 抓 30 秒无 `"type":"quote"`。

**阶段 3：扩展品种**——只改 `subscribe_quotes` 后 `kill -HUP` 即可热更（前提：quote 连接器已运行）。

**回滚**：恢复阶段 0 保留的 1.0.0 二进制/镜像与原 `config.yaml`，重启后重复阶段 0 的 60 秒对比确认回到基线。

#### Docker 方式的具体做法（NAS 198 实际采用）

配置烧在镜像里（`/etc/md-server/config.yaml`），因此"升级二进制"与"开 quote"各是一次换镜像：

```dockerfile
# 阶段 1：在现网镜像上只换二进制（OS 层 / config / HEALTHCHECK / CMD 全沿用）
FROM md-server:1.0.0-rollback           # = 升级前现网镜像（先 docker tag 留底，勿删）
USER root
COPY md-server /usr/local/bin/md-server  # 1.1.0 静态 musl 二进制
RUN chmod +x /usr/local/bin/md-server
USER md-user
# 阶段 2：在 1.1.0 镜像上只换 config.yaml（相对现网仅多 quotes_enabled / subscribe_quotes 两组键）
```

1. 升级前 `docker tag <现网镜像ID> md-server:1.0.0-rollback`，并把当前容器的运行参数（端口、内存/CPU 限额、重启策略、日志策略）完整复制到新容器；
2. 同名替换：旧容器改名 `md-server-1.0.0-bak` 并停止（重启策略改 `no`，避免宿主机重启后抢端口），再用新镜像创建同名 `md-server` 并启动；中断窗口约 6~8 秒，WS 消费者（如 OMS）自动重连；
3. **动态订阅重启后丢失**：换容器后用 `node scripts/compat/restore_subs.mjs <升级前订阅.json> http://HOST:8081 --apply` 把升级前存在、现在缺少的 TICK/KLINE 订阅补回（不带 `--apply` 为 dry-run）；
4. 先在旁路端口起一只同镜像"金丝雀"（如 `28081`），与现网同窗口抓取、逐字节对比通过后再换现网；
5. 回滚：停止并改名新容器（保留现场），把 `md-server-1.0.0-bak` 改回 `md-server`、重启策略改回 `unless-stopped` 并启动，然后再跑 `restore_subs.mjs` 与 60 秒对比。

验收辅助脚本：`scripts/compat/analyze_quote.mjs`（quote 间隔 / 节流 / 非空 / 字段顺序 / 正对照）、
`scripts/compat/sample_process.sh`（10 分钟进程指标采样）、`scripts/compat/restore_subs.mjs`（补回动态订阅）。

### 12.2 升级到 1.1.1（内存调查 + Gateway 退订泄漏 + 写超时；分阶段、可回滚）

1.1.1 的内容与结论见 [CHANGELOG](CHANGELOG.md) 与 `evidence/1.1.1/INVESTIGATION_REPORT.md`。要点：

- 修 Gateway `unsubscribe` 不释放接收端（A1）、改用 `StreamMap` 接收路径（A2，输出字节不变）；
- **Gateway 写超时**：生效已有配置 `api_gateway.ws_write_wait`（默认 `10s`，`0s` 关闭）。这是 1.1.1 **唯一的行为变化**：
  停读 ≥10 秒的 Gateway 客户端会被断开，需自行重连并重新订阅；Legacy `/ws` 不变。**换版前请知会 1stbotv3 等消费者**；
- 新增指标 `md_ws_subscriptions_active`、`md_ws_kicked_write_timeout_total`、`md_tokio_*`；
- 配置文件**不需要改**（沿用 1.1.0 的 `config.yaml`，无新键）；
- **两个发布候选（需业务方选择，详见 `evidence/1.1.1/nas-198/SWAP_PLAN_1.1.1.md`）**：
  - **候选 A（推荐）`md-server:1.1.1`**：A1+A2+A3+A5 + **jemalloc 分配器**。构建：
    `cargo zigbuild --release --target x86_64-unknown-linux-musl --features md-connector/vendored-tls,md-server/jemalloc-stats -p md-server`
    → `dist/md-server-1.1.1-linux-x86_64`。旁路等价负载 6 小时 RSS 斜率 +0.08 MiB/h；
  - 候选 B `md-server:1.1.1-default-alloc`：同上但**默认分配器**（构建去掉 `md-server/jemalloc-stats`）。旁路同样负载下斜率 +16.7 MiB/h，
    **不能解决 RSS 只升不降**，只适合"不愿改分配器"时配合 400MiB 告警 + 定期低峰重建使用。
  sha256 见 `evidence/1.1.1/build-logs/dist-sha256.txt`。

**为什么 `md-server:latest` 仍指向 1.0.0（刻意不动）**：NAS 198 上 `md-server:latest` 与 `md-server:1.0.0-rollback` 是同一个镜像
（`06907bdf7fd8`）。它是整条升级链（1.0.0 → 1.1.0 → 1.1.1）**最后的回滚依据**，所以：

1. 现网容器一律用**显式版本标签**（`md-server:1.1.0-quote-bnb`、`md-server:1.1.1`），从不依赖 `latest`；
2. 若把 `latest` 挪到新版本，任何仍写着 `md-server:latest` 的 compose / 脚本会在下次重建时**悄悄升级**，且回滚依据只剩一个标签，容易被误清理；
3. 只有在业务方确认新版本稳定运行 ≥7 天、且另有一份独立的回滚标签后，才考虑移动 `latest`（并在 CHANGELOG 记一笔）。

**阶段 0：留基线**（升级前）

```bash
# 记录现网容器参数与订阅（含 1stbotv3 运行期动态加的订阅）——换版后要补回
docker inspect md-server > baseline/md-server.inspect.json
curl -s http://HOST:8081/api/v1/subscriptions > baseline/subscriptions.json
curl -s http://HOST:8081/metrics               > baseline/metrics.txt
node scripts/compat/capture_ws.mjs gateway ws://HOST:8081/ws/v1/data 60 baseline/ws-gateway tick.binance.BTCUSDT
```

**阶段 1：旁路验证（不碰现网）**——另起一个 ≤256MiB 的临时容器（不同端口），用 1.1.1 镜像 + 同一份配置，
`restore_subs.mjs` 补入现网订阅，与现网/基线用 `compare.mjs push|subs|metrics` 做同窗口对比；通过后删除临时容器。

**阶段 2：低峰换版**（业务方批准的窗口；约 6 秒中断）

```bash
# 复用 1.1.0 的换版工具：旧容器改名为 md-server-1.1.0-quote-bnb-bak 并停止，按旧容器参数用新镜像重建同名 md-server
# （注意：md-server-1.1.0-bak / md-server-1.0.0-bak 已存在，不要复用这两个名字）
python3 evidence/1.1.0/nas-198/tools/pt_swap.py swap md-server:1.1.1 md-server-1.1.0-quote-bnb-bak
# 立刻补回订阅（Docker 方式：订阅不写回配置，重启即丢）
node scripts/compat/restore_subs.mjs baseline/subscriptions.json http://HOST:8081 --apply
```

**回滚**（任一验收项不通过）：`python3 evidence/1.1.0/nas-198/tools/pt_swap.py rollback md-server-1.1.0-quote-bnb-bak`，随后同样 `restore_subs.mjs` 补订阅。

**换版后 24 小时观察项**：`process_resident_memory_bytes`（逐小时；告警阈值 400MiB）、`md_ws_subscriptions_active`（退订后回落）、
`md_ws_kicked_write_timeout_total`（应≈0；增长 = 有消费者卡顿）、`md_ticks_dropped`（日志同时有限频 WARN）、
`md_connector_reconnect_total`、`md_tokio_alive_tasks`（重连前后同水平）、1stbotv3 与 OMS 消费正常。

---

## 附录

### A. 端口列表

| 端口 | 协议 | 服务 | 说明 |
|------|------|------|------|
| 8081 | HTTP | REST API | 数据查询、健康检查 |
| 50051 | gRPC | gRPC 服务 | 高性能数据接口 |
| 8080 | WebSocket | WebSocket | 实时数据推送 |

### B. API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/metrics` | GET | Prometheus 指标 |
| `/api/v1/subscriptions` | GET | 获取订阅列表 |
| `/api/v1/data/latest/tick/{exchange}/{symbol}` | GET | 获取最新 Tick |
| `/api/v1/data/latest/kline/{exchange}/{symbol}/{interval}` | GET | 获取最新 Kline |
| `/api/v1/data/latest/quote/{exchange}/{symbol}` | GET | 获取最新 Quote（1.1.0 新增，需开启 quote 开关） |
| `/api/v1/subscriptions` | POST / DELETE | 动态增删订阅，`type` = `TICK` / `KLINE` / `QUOTE`（`QUOTE` 为 1.1.0 新增） |
| `/ws/v1/data` | WebSocket | 实时数据推送（Gateway 模式） |
| `/ws` | WebSocket | 实时数据推送（Legacy 模式） |

### C. 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `RUST_LOG` | 日志过滤器（可写 `warn` 或 `warn,md_connector=debug`）。**设置后优先于 YAML 的 `log_level`**，此时 `log_level` 与 SIGHUP 都不能改日志级别 | 未设置 |
| `LOG_LEVEL` | 覆盖 YAML 的 `log_level`（只能写 `trace` / `debug` / `info` / `warn` / `error`；仍低于 `RUST_LOG`） | 未设置 |
| `TZ` | 时区 | 系统时区 |

日志级别的优先级：`RUST_LOG`（已设置且合法）＞ YAML `log_level`（含 `LOG_LEVEL` 覆盖）＞ `info`。
YAML 的 `log_level` 可以用 SIGHUP 热更（不重连）；已设置 `RUST_LOG` 时则不起作用，启动日志会 WARN 说明。
详见 `OPS_GUIDE.md` 第一节"日志级别"。

---

**文档版本**: 1.1.0
**最后更新**: 2026-10-09
**维护者**: DevOps Team
