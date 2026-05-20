# 风险优先级排序：Top 5 兼容性风险

## 风险 1：JSON omitempty 行为不一致（严重度：🔴 高）

**问题**：Go 的 `omitempty` 是零值跳过，Rust serde 默认输出零值。如果任何字段遗漏 `skip_serializing_if`，REST 响应会多出字段。

**影响范围**：所有 REST API 响应、WebSocket 推送

**规避措施**：
- ✅ 已实现：`serde_helpers` 模块 + `skip_serializing_if` 属性（见 md-domain/types.rs）
- ✅ 已实现：`tick_zero_fields_omitted_like_go` 测试专门验证此行为
- 🔲 待做：从 Go 版抓取真实快照，用 `json_compat.rs` 做 byte-level 回归

**验证命令**：
```bash
cargo test -p md-domain -- tick_zero_fields_omitted
cargo test -p md-domain -- kline_zero_fields_omitted
```

---

## 风险 2：JSON 字段顺序不一致（严重度：🟡 中）

**问题**：Go 的 `encoding/json` 按 struct 字段定义顺序输出。Rust serde 的 struct serializer 也按定义顺序输出，但如果有 `#[serde(rename)]` 或使用 map serializer 可能改变顺序。

**影响范围**：如果下游系统依赖字段顺序（如字节级比较、签名验证）

**规避措施**：
- ✅ 已实现：`tick_field_order_matches_go` 测试验证顺序
- 🔲 待做：快照对比自然覆盖
- ⚠️ 注意：serde_json 的 struct serializer 确实按字段定义顺序输出，与 Go 一致

**无需额外处理**，但需持续监控。

---

## 风险 3：tokio 调度差异导致消息乱序（严重度：🟡 中）

**问题**：Go goroutine 调度是协作式的，tokio 是工作窃取调度器。在高并发下：
- 多个 connector 同时写入 channel 时，消息到达顺序可能不同
- WebSocket 重连后，新旧连接的消息可能短暂交错

**影响范围**：Tick/Kline 消息的时间顺序、缓存中的最新值

**规避措施**：
- 🔲 待做：Processor 使用 `tokio::sync::mpsc`（有界 channel），保证单消费者有序处理
- 🔲 待做：每个 connector 使用独立的 sender，Processor 端按到达顺序处理
- 🔲 待做：为 connector_receive_ts 添加单调递增校验，检测乱序
- 🔲 待做：压力测试中对比 Go 和 Rust 的消息顺序

**关键设计决策**：Processor 内部用单 task 从 mpsc receiver 读取并处理，保证全局有序。

---

## 风险 4：WebSocket 重连时序差异（严重度：🟡 中）

**问题**：Go 版重连是固定延迟（无指数退避、无抖动）。Rust 版如果引入不同的重连策略，会导致：
- 重连频率不同
- 订阅恢复时间不同
- 在交易所限流下行为不同

**影响范围**：数据连续性、订阅状态

**规避措施**：
- 🔲 待做：BaseConnector 严格复刻 Go 版重连逻辑：
  1. 连接失败 -> sleep(ReconnectDelay) -> 重试
  2. 连接成功 -> syncSubscriptions
  3. sync 失败 -> close -> sleep(ReconnectDelay) -> 重试
  4. 运行中断开 -> 清空 activeSubs -> sleep(ReconnectDelay) -> 重试
  5. 无最大重试次数，无限循环直到 shutdown
- 🔲 待做：用 `tokio::time::sleep` 而非 `tokio::time::interval`，与 Go 版 `time.Sleep` 语义一致
- 🔲 待做：写重连逻辑的单元测试，用 mock WebSocket server 模拟断连

---

## 风险 5：热重载期间数据丢失窗口（严重度：🟢 低）

**问题**：Go 版热重载时，旧 connector 停止到新 connector 启动之间有数据空窗。Rust 版需要精确复刻这个行为，包括：
- 15 秒超时
- 原子替换 connector map
- 启动失败回滚

**影响范围**：热重载期间的数据连续性

**规避措施**：
- 🔲 待做：ConnectorRegistry 使用 `Arc<RwLock<HashMap<String, Box<dyn Connector>>>>`
- 🔲 待做：热重载流程严格复刻 Go 版：
  1. 创建新 connector
  2. 原子替换 map entry（返回旧 connector）
  3. 停止旧 connector（15 秒超时）
  4. 启动新 connector
  5. 失败则回滚（移除新 connector）
- 🔲 待做：写热重载集成测试，验证替换前后订阅状态一致

---

## 风险优先级总结

| 排名 | 风险 | 严重度 | 当前状态 |
|------|------|--------|----------|
| 1 | JSON omitempty 不一致 | 🔴 高 | ✅ 已有防御 + 测试 |
| 2 | JSON 字段顺序 | 🟡 中 | ✅ 已有测试覆盖 |
| 3 | tokio 调度乱序 | 🟡 中 | 🔲 需在 Processor 设计中处理 |
| 4 | 重连时序差异 | 🟡 中 | 🔲 需严格复刻 Go 逻辑 |
| 5 | 热重载数据窗口 | 🟢 低 | 🔲 Phase 4 实现时处理 |
