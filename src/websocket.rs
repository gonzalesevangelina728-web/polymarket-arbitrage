use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use tokio_tungstenite::connect_async;
use tracing::{debug, error, info, warn};

/// Polymarket RTDS (Real-Time Data Socket) 客户端
pub struct PolymarketRtdsClient {
    btc_price_history: VecDeque<(chrono::DateTime<chrono::Utc>, f64)>,
}

impl PolymarketRtdsClient {
    pub fn new() -> Self {
        Self {
            btc_price_history: VecDeque::with_capacity(1000),
        }
    }

    /// 连接 RTDS WebSocket 并运行
    pub async fn run<F>(&mut self, mut on_btc_price: F) -> Result<()>
    where
        F: FnMut(f64, String),
    {
        let url = "wss://ws-live-data.polymarket.com";
        info!("连接 Polymarket RTDS: {}", url);

        let (mut stream, response) = connect_async(url).await?;
        info!("RTDS 连接成功: {:?}", response.status());

        // 订阅 BTC 实时价格
        let subscription = json!({
            "action": "subscribe",
            "subscriptions": [{
                "topic": "crypto_prices",
                "type": "update",
                "filters": "btcusdt"
            }]
        });

        use tokio_tungstenite::tungstenite::Message;
        stream.send(Message::Text(subscription.to_string())).await?;
        info!("已订阅 BTC 实时价格");

        // 消息循环
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.handle_message(&text, &mut on_btc_price).await {
                        debug!("处理消息失败: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    warn!("RTDS 连接关闭");
                    break;
                }
                Err(e) => {
                    error!("RTDS 错误: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn handle_message<F>(&mut self, text: &str, on_price: &mut F) -> Result<()>
    where
        F: FnMut(f64, String),
    {
        #[derive(Debug, Deserialize)]
        struct PriceUpdate {
            symbol: String,
            price: String,
            source: Option<String>,
        }

        if let Ok(update) = serde_json::from_str::<PriceUpdate>(text) {
            if let Ok(price) = update.price.parse::<f64>() {
                let source = update.source.unwrap_or_else(|| "unknown".to_string());
                
                // 只处理 BTC
                if update.symbol.to_lowercase().contains("btc") {
                    self.update_btc_price(price);
                    on_price(price, source);
                }
            }
        }

        Ok(())
    }

    fn update_btc_price(&mut self, price: f64) {
        self.btc_price_history.push_back((chrono::Utc::now(), price));

        // 只保留最近10分钟的数据
        let cutoff = chrono::Utc::now() - chrono::Duration::minutes(10);
        while let Some((t, _)) = self.btc_price_history.front() {
            if *t < cutoff {
                self.btc_price_history.pop_front();
            } else {
                break;
            }
        }
    }

    /// 获取 BTC 1分钟涨跌幅
    pub fn get_btc_change_1m(&self) -> f64 {
        self.calculate_change(60)
    }

    fn calculate_change(&self, seconds: i64) -> f64 {
        if self.btc_price_history.len() < 2 {
            return 0.0;
        }

        let current = self.btc_price_history.back().unwrap().1;
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(seconds);

        for (time, price) in self.btc_price_history.iter().rev() {
            if *time <= cutoff {
                return (current - price) / price;
            }
        }

        let earliest = self.btc_price_history.front().unwrap().1;
        (current - earliest) / earliest
    }
}
