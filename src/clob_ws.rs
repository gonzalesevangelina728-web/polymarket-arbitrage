use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

/// CLOB 订单簿客户端
pub struct ClobClient {
    prices: HashMap<String, (f64, f64)>, // token_id -> (best_bid, best_ask)
}

impl ClobClient {
    pub fn new() -> Self {
        Self {
            prices: HashMap::new(),
        }
    }

    /// 连接 CLOB 并订阅市场
    pub async fn run(&mut self, up_token_id: &str, down_token_id: &str) -> Result<()> {
        let url = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
        info!("连接 CLOB: {}", url);

        let (mut stream, response) = connect_async(url).await?;
        info!("CLOB 连接成功: {:?}", response.status());

        // 订阅 Up token
        self.subscribe_token(&mut stream, up_token_id).await?;
        // 订阅 Down token
        self.subscribe_token(&mut stream, down_token_id).await?;

        info!("已订阅 Up: {}, Down: {}", up_token_id, down_token_id);

        // 消息循环
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    self.handle_message(&text);
                }
                Ok(Message::Close(_)) => {
                    warn!("CLOB 连接关闭");
                    break;
                }
                Err(e) => {
                    error!("CLOB 错误: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn subscribe_token(
        &self,
        stream: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        token_id: &str,
    ) -> Result<()> {
        let subscription = json!({
            "type": "market",
            "channel": "book",
            "asset_id": token_id
        });

        stream.send(Message::Text(subscription.to_string())).await?;
        Ok(())
    }

    fn handle_message(&mut self, text: &str) {
        #[derive(Debug, Deserialize)]
        struct BookMessage {
            #[serde(rename = "event_type")]
            event_type: String,
            #[serde(default)]
            asset_id: Option<String>,
            #[serde(default)]
            bids: Option<Vec<BookLevel>>,
            #[serde(default)]
            asks: Option<Vec<BookLevel>>,
        }

        #[derive(Debug, Deserialize)]
        struct BookLevel {
            price: String,
            size: String,
        }

        if let Ok(msg) = serde_json::from_str::<BookMessage>(text) {
            if msg.event_type == "book" {
                if let (Some(bids), Some(asks), Some(asset_id)) = (msg.bids, msg.asks, msg.asset_id) {
                    if let (Some(best_bid), Some(best_ask)) = (bids.first(), asks.first()) {
                        if let (Ok(bid), Ok(ask)) = (
                            best_bid.price.parse::<f64>(),
                            best_ask.price.parse::<f64>()
                        ) {
                            self.prices.insert(asset_id.clone(), (bid, ask));
                            info!("📈 {} | Bid: {:.3} | Ask: {:.3}", asset_id, bid, ask);
                        }
                    }
                }
            }
        }
    }

    /// 获取 token 的最佳价格
    pub fn get_price(&self, token_id: &str) -> Option<(f64, f64)> {
        self.prices.get(token_id).copied()
    }
}
