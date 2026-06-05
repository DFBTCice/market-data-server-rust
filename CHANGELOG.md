# 更新日志（CHANGELOG）

本文件记录 `market-data-server` Rust 版的重要变更。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [1.0.0] - 2026-06-06

首个**可用于生产环境长期在线**的版本。在 1:1 重写 Go 版功能的基础上，完成了
断点重连、可观测性、资源治理等多项加固，并补齐了与 Go 版等价的监控体系与部署能力。

### 新增（Features）

- **多维入库延迟指标**：`md_ingestion_latency_ms` 按 `{exchange, type, symbol, interval}`
  分维度统计，可在 Grafana 任意聚合（交易所 / 单标的 / 周期），完全对齐 Go 版 dashboard。
- **网关内部延迟指标**：`md_gateway_internal_latency_ms{topic}` 统计 publish→发送给客户端
  的耗时，支持按 topic 的推送吞吐 TOP10。
- **WebSocket 连接治理指标**：`md_ws_active_clients`（活跃连接数）、
  `md_ws_kicked_lagged_total`（慢客户端踢出数）、`md_ws_messages_sent_total{kind}`。
- **标准进程指标**：零依赖读取 `/proc` 输出 `process_resident_memory_bytes`、
  `process_cpu_seconds_total`、`process_open_fds`、`process_max_fds`、
  `process_start_time_seconds`、`process_uptime_seconds`。
- **Grafana dashboard 模板** `dashboards/md-server-rust.json`，与 Go 版面板一一对应。
- **慢客户端自动踢出**：连续 3 次 broadcast Lagged 即主动断开，保护整体推送链路。
- **失败 stream 黑名单**：订阅持续失败的 stream 进入黑名单并在重连时跳过，到期自动恢复。
- **可选 `vendored-tls` feature**（`md-connector`，默认关闭）：交叉编译时让 OpenSSL
  随源码静态编译，支持在 macOS 上产出 x86_64 Linux 全静态二进制。
- **`Dockerfile.cn`**：国内/内网镜像加速构建（基础镜像 daocloud、apt 阿里云、crates rsproxy）。

### 修复（Fixes）

- **TCP Keepalive 对 TLS 连接失效**：原实现仅对明文连接生效，现对 `wss://`（NativeTls）
  与明文连接均正确设置 keepalive，使用 `socket2::SockRef` 安全借用，避免 fd 所有权问题。
- **时间戳取值可能 panic**：Binance/OKX 解析中的 `SystemTime` 取值改为 `unwrap_or(0)`，
  规避系统时钟早于 UNIX 纪元时的崩溃。
- **`current_subscriptions` 读锁竞争丢数据**：trait 方法改为 `async`，由 `try_read` 改为
  `read().await`，消除锁竞争下返回空列表的问题。
- **关停时序竞争**：连接器任务改用 `JoinSet` 统一等待，确保在 processor 关停前结束写入。

### 性能（Performance）

- **release 编译优化**：启用 `lto`、`codegen-units=1`、`strip`、`panic=abort`，减小体积、提升运行时性能。
- **订阅分批**：大批量订阅按 `MAX_STREAMS_PER_MSG` 分片并间隔发送，避免单帧过大被交易所拒绝。
- **重连抖动（jitter）**：在重连退避基础上叠加随机抖动，避免大量连接同时重连造成雪崩。

### 重构（Refactor）

- 移除未使用的 `graceful_shutdown_timeout` 死代码。
- `BroadcastEvent` 携带 `Instant` 发布时刻，支撑端到端延迟测量。
- `Histogram` 通用化，支持自定义桶边界。

### 文档（Docs）

- `README.md`：补充交叉编译静态 Linux 二进制、Docker（国内镜像）构建说明。
- `DEPLOYMENT.md`：新增 §2.1.1 交叉编译、§4.5 `Dockerfile.cn`、§4.6 Portainer API
  远程一键替换部署流程；§7 监控章节更新为多维指标与对应告警规则。

### 部署记录（Ops）

- 已通过 Portainer API 在内网 Debian 12（x86_64）主机上原生构建并替换 `md-server`
  容器，验证 `/health`、`/metrics`（多维指标生效）、Binance/OKX 实时数据流入、
  gRPC 50051 / 网关 8081 端口均正常，丢弃数为 0。
