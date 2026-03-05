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

        let markets: Vec<serde_json::Value> = response.json().await?;
        
        // 过滤 BTC 5分钟市场
        let btc_markets: Vec<Btc5MinMarket> = markets
            .into_iter()
            .filter_map(|m| parse_btc_market(m))
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
}

/// 解析 BTC 市场数据
fn parse_btc_market(value: serde_json::Value) -> Option<Btc5MinMarket> {
    let question = value.get("question")?.as_str()?.to_lowercase();
    let slug = value.get("slug")?.as_str()?.to_string();
    
    // 过滤 BTC 5分钟市场
    if !question.contains("bitcoin") && !slug.contains("btc-updown-5m") {
        return None;
    }

    let market_id = value.get("id")?.as_str()?.to_string();
    let condition_id = value.get("conditionId")?.as_str()?.to_string();
    let end_date_str = value.get("endDate")?.as_str()?;
    let end_time = DateTime::parse_from_rfc3339(end_date_str).ok()?.with_timezone(&Utc);

    // 解析 outcomes 和 prices
    let outcomes = value.get("outcomes")?.as_array()?;
    let outcome_prices = value.get("outcomePrices")?.as_str()?;
    
    let prices: Vec<f64> = serde_json::from_str(outcome_prices).ok()?;
    
    if outcomes.len() != 2 || prices.len() != 2 {
        return None;
    }

    // 确定 Up/Down
    let up_idx = outcomes.iter().position(|o| {
        o.as_str().map(|s| s.to_lowercase().contains("up")).unwrap_or(false)
    })?;
    let down_idx = 1 - up_idx; // 另一个就是 Down

    Some(Btc5MinMarket {
        market_id,
        condition_id,
        end_time,
        up_outcome: outcomes[up_idx].as_str()?.to_string(),
        down_outcome: outcomes[down_idx].as_str()?.to_string(),
        up_price: prices[up_idx],
        down_price: prices[down_idx],
    })
}

/// 简化的 BTC 5分钟市场信息
#[derive(Debug, Clone)]
pub struct Btc5MinMarket {
    pub market_id: String,
    pub condition_id: String,
    pub end_time: DateTime<Utc>,
    pub up_outcome: String,
    pub down_outcome: String,
    pub up_price: f64,
    pub down_price: f64,
}

impl Btc5MinMarket {
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
