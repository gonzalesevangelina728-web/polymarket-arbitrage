use crate::types::{MarketState, TradeSide};
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};

/// Polymarket RTDS (Real-Time Data Socket) 客户端
pub struct PolymarketRtdsClient {
    stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    btc_price_history: VecDeque<(chrono::DateTime<chrono::Utc>, f64)>,
}

impl PolymarketRtdsClient {
    pub fn new() -> Self {
        Self {
            stream: None,
            btc_price_history: VecDeque::with_capacity(1000),
        }
    }

    /// 连接 RTDS WebSocket
    pub async fn connect(&mut self) -> Result<()> {
        let url = "wss://ws-live-data.polymarket.com";
        info!("连接 Polymarket RTDS: {}", url);

        let (stream, response) = connect_async(url).await?;
        info!("RTDS 连接成功: {:?}", response.status());

        self.stream = Some(stream);
        Ok(())
    }

    /// 订阅 BTC 实时价格 (Binance 源)
    pub async fn subscribe_btc_price(&mut self) -> Result<()> {
        let subscription = json!({
            "action": "subscribe",
            "subscriptions": [{
                "topic": "crypto_prices",
                "type": "update",
                "filters": "btcusdt"
            }]
        });

        if let Some(stream) = &mut self.stream {
            stream.send(Message::Text(subscription.to_string())).await?;
            info!("已订阅 BTC 实时价格 (Binance 源)");
        }
        Ok(())
    }

    /// 订阅 Chainlink 预言机价格（结算用）
    pub async fn subscribe_chainlink_btc(&mut self) -> Result<()> {
        let subscription = json!({
            "action": "subscribe",
            "subscriptions": [{
                "topic": "crypto_prices_chainlink",
                "type": "update",
                "filters": "btcusd"
            }]
        });

        if let Some(stream) = &mut self.stream {
            stream.send(Message::Text(subscription.to_string())).await?;
            info!("已订阅 BTC Chainlink 预言机价格");
        }
        Ok(())
    }

    /// 运行消息循环
    pub async fn run<F>(&mut self, mut on_btc_price: F) -> Result<()>
    where
        F: FnMut(f64, String) + Send + 'static,
    {
        if let Some(stream) = &mut self.stream {
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
            #[serde(rename = "timestamp")]
            ts: u64,
            source: Option<String>,
        }

        match serde_json::from_str::<PriceUpdate>(text) {
            Ok(update) => {
                if let Ok(price) = update.price.parse::<f64>() {
                    let source = update.source.unwrap_or_else(|| "unknown".to_string());
                    
                    // 只处理 BTC
                    if update.symbol.to_lowercase().contains("btc") {
                        self.update_btc_price(price);
                        
                        info!(
                            "BTC 价格更新: ${:.2} | 来源: {} | 1m涨跌: {:.2}%",
                            price,
                            source,
                            self.get_btc_change_1m() * 100.0
                        );
                        
                        on_price(price, source);
                    }
                }
            }
            Err(e) => {
                debug!("解析消息失败: {} - raw: {}", e, text.chars().take(100).collect::<String>());
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

    /// 获取 BTC 5分钟涨跌幅
    pub fn get_btc_change_5m(&self) -> f64 {
        self.calculate_change(300)
    }

    fn calculate_change(&self, seconds: i64) -> f64 {
        if self.btc_price_history.len() < 2 {
            return 0.0;
        }

        let current = self.btc_price_history.back().unwrap().1;
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(seconds);

        // 找到 seconds 前的价格
        for (time, price) in self.btc_price_history.iter().rev() {
            if *time <= cutoff {
                return (current - price) / price;
            }
        }

        // 如果没找到足够早的数据，用最早的数据
        let earliest = self.btc_price_history.front().unwrap().1;
        (current - earliest) / earliest
    }
}

/// Polymarket CLOB WebSocket 客户端（订单簿）
pub struct PolymarketClobClient {
    stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl PolymarketClobClient {
    pub fn new() -> Self {
        Self { stream: None }
    }

    pub async fn connect(&mut self) -> Result<()> {
        let url = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
        info!("连接 Polymarket CLOB: {}", url);

        let (stream, response) = connect_async(url).await?;
        info!("CLOB 连接成功: {:?}", response.status());

        self.stream = Some(stream);
        Ok(())
    }

    /// 订阅市场订单簿
    pub async fn subscribe_market(&mut self, asset_id: &str) -> Result<()> {
        let subscription = json!({
            "type": "market",
            "channel": "book",
            "asset_id": asset_id
        });

        if let Some(stream) = &mut self.stream {
            stream.send(Message::Text(subscription.to_string())).await?;
            info!("已订阅市场订单簿: {}", asset_id);
        }
        Ok(())
    }

    /// 运行消息循环（简化版，暂不处理）
    pub async fn run<F>(&mut self, _on_book_update: F) -> Result<()>
    where
        F: FnMut(String, f64, f64) + Send + 'static,
    {
        info!("CLOB 客户端运行中...");
        // TODO: 实现订单簿消息处理
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }
}
