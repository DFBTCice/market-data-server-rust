# ADR-0001: JSON omitempty 兼容策略

## 状态

已采纳

## 背景

Go 版使用 `encoding/json` 序列化 protobuf struct，所有字段带有 `omitempty` 标签。这意味着零值字段（`false`、`0`、`""`、空数组）不出现在 JSON 输出中。Rust 的 serde 默认会输出这些零值。为了字节级兼容，必须在 Rust 侧复刻此行为。

## 决策

使用自定义 helper 模块 `serde_helpers` 提供 `skip_*` 函数，所有对外结构体统一使用 `#[serde(skip_serializing_if = "...")]` 属性。

## 理由

- Go 的 `omitempty` 是**字段级**行为，不是类型级的。不同字段可能有不同的"零值"语义。
- 使用 helper 函数而非 derive macro，因为行为需要逐字段控制。
- 与 Go 版的 struct tags 一一对应，降低回归风险。

## 影响

- 所有 REST/WS 响应结构体必须使用这些 helper
- 测试必须覆盖零值和非零值两种情况
- 如果 Go 版修改 struct tags（去掉 omitempty），Rust 侧需同步更新
