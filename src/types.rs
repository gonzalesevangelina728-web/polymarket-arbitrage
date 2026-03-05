use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct MarketState {
    pub market_id: String,
    pub end_time: DateTime<Utc>,
    pub up_price: f64,
    pub down_price: f64,
    pub up_ask: f64,
    pub down_ask: f64,
    pub btc_price: f64,
    pub btc_change_1m: f64,
    pub btc_change_5m: f64,
    pub timestamp: DateTime<Utc>,
}

impl MarketState {
    pub fn time_to_end(&self) -> i64 {
        (self.end_time - self.timestamp).num_seconds()
    }
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub market_id: String,
    pub side: TradeSide,
    pub price: f64,
    pub amount: f64,
    pub btc_price_at_entry: f64,
    pub btc_change_1m: f64,
    pub time_to_end: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TradeSide {
    Up,
    Down,
}

impl std::fmt::Display for TradeSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeSide::Up => write!(f, "UP"),
            TradeSide::Down => write!(f, "DOWN"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Position {
    pub market_id: String,
    pub up_entry: Option<Trade>,
    pub down_entry: Option<Trade>,
}

impl Position {
    pub fn new(market_id: String) -> Self {
        Self {
            market_id,
            up_entry: None,
            down_entry: None,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.up_entry.is_some() && self.down_entry.is_some()
    }

    pub fn total_cost(&self) -> Option<f64> {
        match (&self.up_entry, &self.down_entry) {
            (Some(up), Some(down)) => Some(up.price + down.price),
            _ => None,
        }
    }
}

// WebSocket消息类型
#[derive(Debug, Deserialize)]
pub struct WsMessage {
    #[serde(rename = "event_type")]
    pub event_type: String,
    #[serde(default)]
    pub market: Option<String>,
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub bids: Option<Vec<OrderBookLevel>>,
    #[serde(default)]
    pub asks: Option<Vec<OrderBookLevel>>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OrderBookLevel {
    pub price: String,
    pub size: String,
}

// BTC价格数据
#[derive(Debug, Deserialize)]
pub struct BtcPriceUpdate {
    pub symbol: String,
    pub price: f64,
    pub timestamp: u64,
}
