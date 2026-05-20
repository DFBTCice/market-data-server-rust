# Phase 1 TDD 步骤

## 步骤 1: md-domain（已完成 ✅）

### 1.1 serde_helpers（先写 helper，无测试，纯工具）

### 1.2 types（Tick + Kline）
- [x] RED: 写 `tick_zero_fields_omitted_like_go` -- 验证零值字段不输出
- [x] RED: 写 `tick_full_fields_present` -- 验证有值字段正确输出
- [x] RED: 写 `tick_field_order_matches_go` -- 验证字段顺序
- [x] RED: 写 `kline_zero_fields_omitted` -- Kline 零值省略
- [x] RED: 写 `kline_full_fields` -- Kline 全字段
- [x] GREEN: 实现 Tick/Kline struct + serde 属性
- [x] REFACTOR: 提取 serde_helpers 模块

### 1.3 topic
- [x] RED: 写 `parse_tick_topic` / `parse_kline_topic`
- [x] RED: 写 `parse_invalid_format` / `parse_unknown_prefix`
- [x] RED: 写 `format_tick` / `format_kline`
- [x] RED: 写 `normalize_*` 系列
- [x] RED: 写 `tick_cache_key` / `kline_cache_key`
- [x] RED: 写 `parse_then_format_roundtrip`
- [x] GREEN: 实现 Topic enum + parse + format
- [x] REFACTOR: 提取公共 normalize_symbol

---

## 步骤 2: md-config ✅

### 2.1 测试（7 个）
- [x] `load_default_config` -- 从 config.yaml 加载，验证所有字段
- [x] `load_config_from_yaml_string` -- 从 YAML 字符串加载
- [x] `env_var_override` -- 环境变量覆盖生效
- [x] `missing_fields_use_defaults` -- 缺失字段用默认值填充
- [x] `binance_enabled_without_url_fails` -- Binance 启用但无 URL 报错
- [x] `okx_enabled_without_url_fails` -- OKX 启用但无 URL 报错
- [x] `field_order_matches_go` -- 字段顺序与 Go 版一致

### 2.2 实现
- [x] 定义 Config struct（serde Deserialize + humantime_serde 解析 Duration）
- [x] 实现 load_config / load_config_from_str
- [x] 实现 merge_defaults（手动合并默认值，对标 Go 版 viper.SetDefault）
- [x] 实现 apply_env_overrides
- [x] 实现 validate（对标 Go 版 validateConnectorEndpoints）

---

## 步骤 3: md-proto ✅

### 3.1 测试（9 个）
- [x] `proto_compiles_successfully` -- 生成的 Tick/Kline 可实例化
- [x] `kline_type_works` -- Kline 类型字段正确
- [x] `market_data_service_trait_exists` -- MarketDataServiceServer trait 存在
- [x] `admin_service_trait_exists` -- AdminServiceServer trait 存在
- [x] `data_type_enum_variants` -- DataType 枚举值正确
- [x] `subscription_request_fields` -- SubscriptionRequest 字段正确
- [x] `tick_json_serialization_matches_go_format` -- JSON 字段名 snake_case
- [x] `tick_prost_default_is_zero_values` -- Default 实现正确
- [x] `admin_data_type_serde_roundtrip` -- enum serde roundtrip

### 3.2 实现
- [x] 复制 .proto 文件到 crates/md-proto/proto/
- [x] build.rs（tonic_build + serde derive + serde_json）
- [x] src/lib.rs 导出生成的类型和服务

---

## 步骤 4: md-connector ✅

### 4.1 测试（13 个）
- [x] `parse_aggtrade_to_tick` -- aggTrade JSON -> Tick
- [x] `parse_aggtrade_symbol_normalized` -- symbol 归一化为大写
- [x] `parse_kline_to_kline` -- kline JSON -> Kline
- [x] `target_to_streams_tick` -- TICK -> ["<sym>@aggTrade"]
- [x] `target_to_streams_kline` -- KLINE -> ["<sym>@kline_<interval>"]
- [x] `target_to_streams_kline_5m` -- 5m 间隔
- [x] `build_subscribe_message` -- SUBSCRIBE JSON
- [x] `build_unsubscribe_message` -- UNSUBSCRIBE JSON
- [x] `adapter_parse_aggtrade_message` -- 完整 stream 消息解析
- [x] `adapter_parse_kline_message` -- 完整 kline stream 消息
- [x] `adapter_ignores_book_ticker` -- 跳过未实现的消息类型
- [x] `parse_invalid_json_returns_error` -- 错误处理
- [x] `parse_missing_field_returns_error` -- 缺失字段错误

### 4.2 实现
- [x] Connector trait + ExchangeAdapter trait（lib.rs）
- [x] BinanceAdapter（binance.rs）-- 消息解析、stream 映射
- [x] BaseConnector（base.rs）-- WebSocket 管理、重连（指数退避）、PendingSubOps、心跳

---

## 步骤 5: md-server（最小端到端）✅

### 5.1 实现
- [x] main.rs：Clap CLI (--config) -> 加载配置 -> 创建 BinanceAdapter/BaseConnector -> 添加订阅 -> 启动连接循环 -> 打印 Tick/Kline 数据
- [x] 修复 `sync_subscriptions_on_connect` 中 `Message::Ping(vec![]).await` 挂起的问题
- [x] 合并 pending ops 和 desired vs active diff 为单次批量 SUBSCRIBE
- [x] 处理 Binance 订阅确认消息 `{"result":null,"id":N}`（无 `stream` 字段）
- [x] 调整日志级别：connector 内部为 debug，server 层为 info

### 5.2 验证
- [x] `cargo test` 全部 56 个测试通过
- [x] `cargo run -- --config config.yaml` 成功连接 Binance WebSocket
- [x] 实时接收并打印 TICK 数据（BTCUSDT, ETHUSDT）
- [x] 实时接收并打印 KLINE 数据（11 个交易对）

### 5.3 已知问题
- 订阅确认消息触发 JSON 解析失败（已优雅处理：静默忽略）
- 仅支持 Binance（OKX 为 Phase 2）
- 无数据缓存和 gRPC 服务（Phase 2）

---

## 开发顺序总结

```
1. md-domain   ✅ 已完成 (23 tests pass)
2. md-config   ✅ 已完成 (7 tests pass)
3. md-proto    ✅ 已完成 (9 tests pass)
4. md-connector ✅ 已完成 (13 tests pass)
5. md-server   ✅ 已完成 (端到端验证通过)
```

Phase 1 完成！总计 56 个测试，端到端数据通路验证通过。

每个步骤严格遵循：RED -> GREEN -> REFACTOR
