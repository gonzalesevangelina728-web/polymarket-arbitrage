//! 策略配置参数

/// Up买入阈值（恐慌抛售信号）
pub const UP_PRICE_THRESHOLD: f64 = 0.25;

/// Down补仓阈值
pub const DOWN_PRICE_THRESHOLD: f64 = 0.70;

/// 最大总成本上限（安全边际）
pub const MAX_TOTAL_COST: f64 = 0.90;

/// BTC 1分钟跌幅触发（第一腿入场）
pub const BTC_1M_DROP_TRIGGER: f64 = -0.03;

/// BTC反弹触发（第二腿补仓）
pub const BTC_BOUNCE_TRIGGER: f64 = 0.02;

/// 最短剩余时间（秒）
pub const MIN_TIME_REMAINING: i64 = 90;

/// 入场窗口开始：距离结算240秒 (04:00)
pub const ENTRY_WINDOW_START: i64 = 240;

/// 入场窗口结束：距离结算90秒 (01:30)
pub const ENTRY_WINDOW_END: i64 = 90;

/// WebSocket URLs
pub const POLYMARKET_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws";

/// SQLite数据库路径
pub const DB_PATH: &str = "paper_trades.db";

/// 虚拟交易金额（USDC）
pub const TRADE_AMOUNT: f64 = 100.0;
