use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

/// Gamma API 客户端
pub struct GammaClient {
    client: reqwest::Client,
}

impl GammaClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// 获取 BTC 5分钟市场列表
    pub async fn get_btc_5min_markets(&self) -> Result<Vec<Btc5MinMarket>> {
        let url = format!("{}/markets", GAMMA_API_BASE);
        
        debug!("请求 Gamma API: {}", url);
        
        let response = self.client
            .get(&url)
            .query(&[
                ("active", "true"),
                ("archived", "false"),
                ("closed", "false"),
                ("limit", "100"),
            ])
            .send()
            .await?;

        let markets: Vec<Market> = response.json().await?;
        
        // 过滤 BTC 5分钟市场
        let btc_markets: Vec<Btc5MinMarket> = markets
            .into_iter()
            .filter(|m| m.question.to_lowercase().contains("bitcoin") || 
                    m.slug.contains("btc-updown-5m"))
            .filter_map(|m| Btc5MinMarket::from_market(m))
            .collect();

        info!("找到 {} 个 BTC 5分钟活跃市场", btc_markets.len());
        
        for market in &btc_markets {
            info!(
                "市场: {} | 结束: {} | Up: {:.3} | Down: {:.3}",
                market.market_id,
                market.end_time.format("%H:%M:%S"),
                market.up_price,
                market.down_price
            );
        }

        Ok(btc_markets)
    }

    /// 获取特定市场的详细信息
    pub async fn get_market(&self, market_id: &str) -> Result<Market> {
        let url = format!("{}/markets/{}", GAMMA_API_BASE, market_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let market: Market = response.json().await?;
        Ok(market)
    }
}

/// Gamma API 返回的市场数据
#[derive(Debug, Deserialize)]
pub struct Market {
    pub id: String,
    pub question: String,
    pub slug: String,
    #[serde(rename = "conditionId")]
    pub condition_id: String,
    #[serde(rename = "endDate")]
    pub end_date: DateTime<Utc>,
    pub outcomes: Option<Vec<String>>,
    pub outcome_prices: Option<String>, // JSON string like "[\"0.5\", \"0.5\"]"
    pub tokens: Option<Vec<Token>>,
    pub active: bool,
    pub closed: bool,
    pub archived: bool,
}

#[derive(Debug, Deserialize)]
pub struct Token {
    pub token_id: String,
    pub outcome: String,
    pub price: f64,
}

/// 简化的 BTC 5分钟市场信息
#[derive(Debug, Clone)]
pub struct Btc5MinMarket {
    pub market_id: String,
    pub condition_id: String,
    pub end_time: DateTime<Utc>,
    pub up_token_id: String,
    pub down_token_id: String,
    pub up_price: f64,
    pub down_price: f64,
}

impl Btc5MinMarket {
    fn from_market(market: Market) -> Option<Self> {
        let tokens = market.tokens?;
        
        if tokens.len() != 2 {
            return None;
        }

        let up_token = tokens.iter().find(|t| t.outcome.to_lowercase().contains("up"))?;
        let down_token = tokens.iter().find(|t| t.outcome.to_lowercase().contains("down"))?;

        Some(Self {
            market_id: market.id,
            condition_id: market.condition_id,
            end_time: market.end_date,
            up_token_id: up_token.token_id.clone(),
            down_token_id: down_token.token_id.clone(),
            up_price: up_token.price,
            down_price: down_token.price,
        })
    }

    /// 计算距离结算还有多少秒
    pub fn seconds_to_end(&self) -> i64 {
        (self.end_time - Utc::now()).num_seconds()
    }

    /// 是否还在交易窗口内 (T-240s 到 T-90s)
    pub fn in_trading_window(&self) -> bool {
        let secs = self.seconds_to_end();
        secs >= 90 && secs <= 240
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_btc_markets() {
        let client = GammaClient::new();
        let markets = client.get_btc_5min_markets().await.unwrap();
        assert!(!markets.is_empty());
    }
}
