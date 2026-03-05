use crate::types::{BtcPriceUpdate, MarketState};
use anyhow::Result;
use std::collections::VecDeque;
use tracing::info;

pub struct PolymarketWsClient {
    connected: bool,
}

impl PolymarketWsClient {
    pub fn new() -> Self {
        Self { connected: false }
    }

    pub async fn connect(&mut self) -> Result<()> {
        // TODO: 实现 WebSocket 连接
        info!("WebSocket 客户端初始化 (待实现)");
        self.connected = true;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        // TODO: 实现消息循环
        info!("WebSocket 运行中...");
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }
}

// BTC价格数据源
pub struct BtcPriceClient {
    price_history: VecDeque<(chrono::DateTime<chrono::Utc>, f64)>,
}

impl BtcPriceClient {
    pub fn new() -> Self {
        Self {
            price_history: VecDeque::with_capacity(1000),
        }
    }

    pub fn update_price(&mut self, price: f64) {
        self.price_history.push_back((chrono::Utc::now(), price));
        // 只保留最近10分钟的数据
        let cutoff = chrono::Utc::now() - chrono::Duration::minutes(10);
        while let Some((t, _)) = self.price_history.front() {
            if *t < cutoff {
                self.price_history.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn calculate_change(&self, seconds: i64) -> f64 {
        if self.price_history.len() < 2 {
            return 0.0;
        }

        let current = self.price_history.back().unwrap().1;
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(seconds);

        // 找到seconds前的价格
        for (time, price) in self.price_history.iter().rev() {
            if *time <= cutoff {
                return (current - price) / price;
            }
        }

        // 如果没找到足够早的数据，用最早的数据
        let earliest = self.price_history.front().unwrap().1;
        (current - earliest) / earliest
    }
}
