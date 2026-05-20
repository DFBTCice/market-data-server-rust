use std::fmt;

/// Topic 解析错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopicError {
    InvalidFormat(String),
    UnknownPrefix(String),
}

impl fmt::Display for TopicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TopicError::InvalidFormat(s) => write!(f, "invalid topic format: {}", s),
            TopicError::UnknownPrefix(s) => write!(f, "unknown topic prefix: {}", s),
        }
    }
}

impl std::error::Error for TopicError {}

/// Topic 枚举 -- 对标 Go 版 topic 格式
///
/// - `tick.<exchange>.<symbol>`
/// - `kline.<interval>.<exchange>.<symbol>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Topic {
    Tick { exchange: String, symbol: String },
    Kline { interval: String, exchange: String, symbol: String },
}

impl Topic {
    /// 解析 topic 字符串
    ///
    /// "tick.binance.BTCUSDT" -> Topic::Tick { exchange: "binance", symbol: "BTCUSDT" }
    /// "kline.1m.binance.BTCUSDT" -> Topic::Kline { interval: "1m", exchange: "binance", symbol: "BTCUSDT" }
    pub fn parse(s: &str) -> Result<Self, TopicError> {
        let parts: Vec<&str> = s.split('.').collect();
        match parts.first() {
            Some(&"tick") => {
                if parts.len() != 3 {
                    return Err(TopicError::InvalidFormat(s.to_string()));
                }
                Ok(Topic::Tick {
                    exchange: parts[1].to_string(),
                    symbol: parts[2].to_string(),
                })
            }
            Some(&"kline") => {
                if parts.len() != 4 {
                    return Err(TopicError::InvalidFormat(s.to_string()));
                }
                Ok(Topic::Kline {
                    interval: parts[1].to_string(),
                    exchange: parts[2].to_string(),
                    symbol: parts[3].to_string(),
                })
            }
            Some(other) => Err(TopicError::UnknownPrefix(other.to_string())),
            None => Err(TopicError::InvalidFormat(s.to_string())),
        }
    }

    /// 格式化为 topic 字符串
    pub fn format(&self) -> String {
        match self {
            Topic::Tick { exchange, symbol } => {
                format!("tick.{}.{}", exchange, normalize_symbol(symbol))
            }
            Topic::Kline { interval, exchange, symbol } => {
                format!("kline.{}.{}.{}", interval, exchange, normalize_symbol(symbol))
            }
        }
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

/// Symbol 归一化 -- 全局唯一入口
/// 对标 Go 版所有 `strings.ToUpper(symbol)` 调用
pub fn normalize_symbol(symbol: &str) -> String {
    symbol.to_uppercase()
}

/// 构建 tick topic 字符串
pub fn tick_topic(exchange: &str, symbol: &str) -> String {
    format!("tick.{}.{}", exchange, normalize_symbol(symbol))
}

/// 构建 kline topic 字符串
pub fn kline_topic(exchange: &str, symbol: &str, interval: &str) -> String {
    format!("kline.{}.{}.{}", interval, exchange, normalize_symbol(symbol))
}

/// Tick cache key -- 对标 Go 版 `getTickCacheKey`
pub fn tick_cache_key(exchange: &str, symbol: &str) -> String {
    format!("{}:{}", exchange.to_uppercase(), symbol.to_uppercase())
}

/// Kline cache key -- 对标 Go 版 `getKlineCacheKey`
pub fn kline_cache_key(exchange: &str, symbol: &str, interval: &str) -> String {
    format!("{}:{}:{}", exchange.to_uppercase(), symbol.to_uppercase(), interval)
}

/// 解析 topic 字符串并返回 exchange + symbol（用于 REST handler 从 topic 提取信息）
pub fn extract_exchange_symbol(topic_str: &str) -> Option<(String, String)> {
    match Topic::parse(topic_str) {
        Ok(Topic::Tick { exchange, symbol }) => Some((exchange, symbol)),
        Ok(Topic::Kline { exchange, symbol, .. }) => Some((exchange, symbol)),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Topic::parse ----

    #[test]
    fn parse_tick_topic() {
        let t = Topic::parse("tick.binance.BTCUSDT").unwrap();
        assert_eq!(t, Topic::Tick {
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
        });
    }

    #[test]
    fn parse_kline_topic() {
        let t = Topic::parse("kline.1m.binance.BTCUSDT").unwrap();
        assert_eq!(t, Topic::Kline {
            interval: "1m".into(),
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
        });
    }

    #[test]
    fn parse_invalid_format() {
        assert!(Topic::parse("tick.binance").is_err());
        assert!(Topic::parse("kline.1m").is_err());
        assert!(Topic::parse("").is_err());
    }

    #[test]
    fn parse_unknown_prefix() {
        assert!(Topic::parse("trade.binance.BTCUSDT").is_err());
    }

    // ---- Topic::format ----

    #[test]
    fn format_tick() {
        let t = Topic::Tick {
            exchange: "binance".into(),
            symbol: "btcusdt".into(),  // 小写 -> 格式化时归一化为大写
        };
        assert_eq!(t.format(), "tick.binance.BTCUSDT");
    }

    #[test]
    fn format_kline() {
        let t = Topic::Kline {
            interval: "1m".into(),
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
        };
        assert_eq!(t.format(), "kline.1m.binance.BTCUSDT");
    }

    // ---- normalize_symbol ----

    #[test]
    fn normalize_uppercase() {
        assert_eq!(normalize_symbol("BTCUSDT"), "BTCUSDT");
    }

    #[test]
    fn normalize_lowercase() {
        assert_eq!(normalize_symbol("btcusdt"), "BTCUSDT");
    }

    #[test]
    fn normalize_mixed_case() {
        assert_eq!(normalize_symbol("BtcUsdt"), "BTCUSDT");
    }

    // ---- topic 构建函数 ----

    #[test]
    fn tick_topic_builder() {
        assert_eq!(tick_topic("binance", "BTCUSDT"), "tick.binance.BTCUSDT");
        assert_eq!(tick_topic("binance", "btcusdt"), "tick.binance.BTCUSDT");
    }

    #[test]
    fn kline_topic_builder() {
        assert_eq!(kline_topic("binance", "BTCUSDT", "1m"), "kline.1m.binance.BTCUSDT");
        assert_eq!(kline_topic("okx", "ETH-USDT-SWAP", "5m"), "kline.5m.okx.ETH-USDT-SWAP");
    }

    // ---- cache key ----

    #[test]
    fn tick_cache_key_format() {
        assert_eq!(tick_cache_key("binance", "BTCUSDT"), "BINANCE:BTCUSDT");
        assert_eq!(tick_cache_key("Binance", "btcusdt"), "BINANCE:BTCUSDT");
    }

    #[test]
    fn kline_cache_key_format() {
        assert_eq!(kline_cache_key("binance", "BTCUSDT", "1m"), "BINANCE:BTCUSDT:1m");
    }

    // ---- roundtrip ----

    #[test]
    fn parse_then_format_roundtrip() {
        let inputs = vec![
            "tick.binance.BTCUSDT",
            "kline.1m.binance.BTCUSDT",
            "tick.okx.ETH-USDT-SWAP",
            "kline.5m.okx.BTC-USDT-SWAP",
        ];
        for input in inputs {
            let topic = Topic::parse(input).unwrap();
            assert_eq!(topic.format(), input);
        }
    }

    // ---- extract ----

    #[test]
    fn extract_from_tick() {
        let (ex, sym) = extract_exchange_symbol("tick.binance.BTCUSDT").unwrap();
        assert_eq!(ex, "binance");
        assert_eq!(sym, "BTCUSDT");
    }

    #[test]
    fn extract_from_kline() {
        let (ex, sym) = extract_exchange_symbol("kline.1m.binance.BTCUSDT").unwrap();
        assert_eq!(ex, "binance");
        assert_eq!(sym, "BTCUSDT");
    }

    #[test]
    fn extract_invalid_returns_none() {
        assert!(extract_exchange_symbol("invalid").is_none());
    }
}
