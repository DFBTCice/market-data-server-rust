/// Go `omitempty` 兼容 helper：零值字段不序列化。
///
/// Go 的 `encoding/json` 对 `omitempty` 的定义：
/// - string: 空字符串 `""` 跳过
/// - int64: `0` 跳过
/// - bool: `false` 跳过
/// - slice: nil 或空 slice 跳过
pub fn is_empty_string(s: &str) -> bool {
    s.is_empty()
}

pub fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

pub fn is_false(b: &bool) -> bool {
    !*b
}

pub fn is_empty_vec<T>(v: &[T]) -> bool {
    v.is_empty()
}
