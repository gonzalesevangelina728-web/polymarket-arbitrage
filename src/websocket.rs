use crate::types::{MarketState, TradeSide};
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};

pub struct PolymarketWsClient {
    stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    btc_client: BtcPriceClient,
}

impl PolymarketWsClient {
    pub fn new() -> Self {
        Self {
            stream: None,
            btc_client: BtcPriceClient::new(),
        }
    }

    pub async fn connect(&mut self) -> Result<()> {
        let url = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
        info!("连接 Polymarket WebSocket: {}", url);

        let (stream, response) = connect_async(url).await?;
        info!("WebSocket 连接成功: {:?}", response.status());

        self.stream = Some(stream);
        Ok(())
    }

    /// 订阅 BTC 5分钟市场
    pub async fn subscribe_btc_market(&mut self, asset_id: &str) -> Result<()> {
        let subscription = json!({
            "type": "market",
            "channel": "book",
            "asset_id": asset_id
        });

        if let Some(stream) = &mut self.stream {
            stream.send(Message::Text(subscription.to_string())).await?;
            info!("已订阅市场: {}", asset_id);
        }
        Ok(())
    }

    /// 订阅价格变动
    pub async fn subscribe_price_changes(&mut self, asset_ids: &[String]) -> Result<()> {
        let subscription = json!({
            "type": "market",
            "channel": "price_change",
            "asset_ids": asset_ids
        });

        if let Some(stream) = &mut self.stream {
            stream.send(Message::Text(subscription.to_string())).await?;
            info!("已订阅价格变动: {:?}", asset_ids);
        }
        Ok(())
    }

    pub async fn run<F>(&mut self, mut on_market_update: F) -> Result<()>
    where
        F: FnMut(MarketState) + Send + 'static,
    {
        if let Some(stream) = &mut self.stream {
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = self.handle_message(&text, &mut on_market_update).await {
                            warn!("处理消息失败: {}", e);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        warn!("WebSocket 连接关闭");
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket 错误: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn handle_message<F>(&mut self, text: &str, on_update: &mut F) -> Result<()>
    where
        F: FnMut(MarketState),
    {
        #[derive(Debug, Deserialize)]
        struct WsMessage {
            #[serde(rename = "event_type")]
            event_type: String,
            #[serde(default)]
            asset_id: Option<String>,
            #[serde(default)]
            bids: Option<Vec<OrderBookLevel>>,
            #[serde(default)]
            asks: Option<Vec<OrderBookLevel>>,
            #[serde(default)]
            price: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct OrderBookLevel {
            price: String,
            size: String,
        }

        match serde_json::from_str::<WsMessage>(text) {
            Ok(msg) => {
                debug!("收到 {} 消息", msg.event_type);

                // 解析订单簿数据
                if let (Some(bids), Some(asks), Some(asset_id)) = (msg.bids, msg.asks, msg.asset_id) {
                    if let Some(best_bid) = bids.first() {
                        if let Some(best_ask) = asks.first() {
                            let bid_price: f64 = best_bid.price.parse().unwrap_or(0.0);
                            let ask_price: f64 = best_ask.price.parse().unwrap_or(0.0);

                            info!(
                                "{} - Bid: {:.3}, Ask: {:.3}",
                                asset_id, bid_price, ask_price
                            );

                            // TODO: 构建 MarketState 并回调
                            // on_update(market_state);
                        }
                    }
                }
            }
            Err(e) => {
                debug!("解析消息失败: {} - raw: {}", e, text.chars().take(100).collect::<String>());
            }
        }

        Ok(())
    }

    pub fn update_btc_price(&mut self, price: f64) {
        self.btc_client.update_price(price);
    }

    pub fn get_btc_change(&self, seconds: i64) -> f64 {
        self.btc_client.calculate_change(seconds)
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

// Binance BTC 价格源
pub struct BinanceBtcClient;

impl BinanceBtcClient {
    pub async fn connect() -> Result<Self> {
        info!("连接 Binance BTC 价格源...");
        // TODO: 实现 Binance WebSocket 连接
        Ok(Self)
    }

    pub async fn run<F>(&self, mut on_price: F) -> Result<()>
    where
        F: FnMut(f64) + Send + 'static,
    {
        // TODO: 实现 Binance WebSocket 价格流
        // 临时模拟数据
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            on_price(85000.0); // 模拟价格
        }
    }
}
