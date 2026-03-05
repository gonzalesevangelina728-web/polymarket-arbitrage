use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

/// 生成当前 BTC 5分钟市场的时间戳
pub fn get_current_btc_5m_timestamp() -> i64 {
    let now = Utc::now().timestamp();
    // 向下取整到最近的 5 分钟 (300 秒)
    (now / 300) * 300
}

/// 生成 BTC 5分钟市场的 slug
pub fn get_btc_5m_slug(timestamp: i64) -> String {
    format!("btc-updown-5m-{}", timestamp)
}

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

    /// 获取当前活跃的 BTC 5分钟市场
    pub async fn get_current_btc_5min_market(&self) -> Result<Option<Btc5MinMarket>> {
        // 使用已知的市场时间戳作为基准
        let base_timestamp = 1772724600i64; // 2026-03-05 23:30:00 UTC
        let now = Utc::now().timestamp();
        
        // 计算当前应该处于哪个 5分钟周期
        let cycles_passed = (now - base_timestamp) / 300;
        let current_timestamp = base_timestamp + cycles_passed * 300;
        
        // 尝试获取当前市场
        for offset in [0, -300, 300] {
            let timestamp = current_timestamp + offset;
            let slug = get_btc_5m_slug(timestamp);
            
            info!("查找市场: {} (timestamp: {})", slug, timestamp);
            
            match self.get_market_by_slug(&slug).await {
                Ok(market) => {
                    if let Some(btc_market) = parse_btc_market(market) {
                        info!(
                            "✅ 找到市场: {} | 结束: {} | Up: {:.3} | Down: {:.3}",
                            btc_market.market_id,
                            btc_market.end_time.format("%H:%M:%S"),
                            btc_market.up_price,
                            btc_market.down_price
                        );
                        return Ok(Some(btc_market));
                    }
                }
                Err(e) => {
                    debug!("未找到市场 {}: {}", slug, e);
                }
            }
        }
        
        Ok(None)
    }
    
    /// 通过 slug 获取市场
    async fn get_market_by_slug(&self, slug: &str) -> Result<serde_json::Value> {
        let url = format!("{}/markets", GAMMA_API_BASE);
        
        let response = self.client
            .get(&url)
            .query(&[("slug", slug)])
            .send()
            .await?;

        let markets: Vec<serde_json::Value> = response.json().await?;
        
        markets.into_iter().next().ok_or_else(|| anyhow::anyhow!("Market not found"))
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

    // 解析 outcomes 和 prices (都是 JSON 字符串)
    let outcomes_str = value.get("outcomes")?.as_str()?;
    let outcome_prices_str = value.get("outcomePrices")?.as_str()?;
    
    let outcomes: Vec<String> = serde_json::from_str(outcomes_str).ok()?;
    let prices: Vec<f64> = serde_json::from_str(outcome_prices_str).ok()?;
    
    if outcomes.len() != 2 || prices.len() != 2 {
        return None;
    }

    // 确定 Up/Down
    let up_idx = outcomes.iter().position(|o| {
        o.to_lowercase().contains("up")
    })?;
    let down_idx = 1 - up_idx; // 另一个就是 Down

    Some(Btc5MinMarket {
        market_id,
        condition_id,
        end_time,
        up_outcome: outcomes[up_idx].clone(),
        down_outcome: outcomes[down_idx].clone(),
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
