use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

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
        let now = Utc::now().timestamp();
        
        // 向下取整到最近的 5 分钟 (300 秒)
        let current_timestamp = (now / 300) * 300;
        
        // 尝试获取当前市场
        for offset in [0, -300, 300] {
            let timestamp = current_timestamp + offset;
            let slug = get_btc_5m_slug(timestamp);
            
            info!("查找市场: {} (timestamp: {})", slug, timestamp);
            
            match self.get_market_by_slug(&slug).await {
                Ok(market) => {
                    info!("API 返回市场数据，开始解析...");
                    match parse_btc_market(market) {
                        Some(btc_market) => {
                            info!(
                                "✅ 找到市场: {} | 结束: {} | Up: {:.3} | Down: {:.3}",
                                btc_market.market_id,
                                btc_market.end_time.format("%H:%M:%S"),
                                btc_market.up_price,
                                btc_market.down_price
                            );
                            return Ok(Some(btc_market));
                        }
                        None => {
                            warn!("找到市场但解析失败: {}", slug);
                        }
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
    // 检查必需字段
    let question = match value.get("question").and_then(|v| v.as_str()) {
        Some(q) => q.to_lowercase(),
        None => {
            warn!("解析失败: 缺少 question 字段");
            return None;
        }
    };
    
    let slug = match value.get("slug").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            warn!("解析失败: 缺少 slug 字段");
            return None;
        }
    };
    
    // 过滤 BTC 5分钟市场
    if !question.contains("bitcoin") && !slug.contains("btc-updown-5m") {
        return None;
    }

    let market_id = match value.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            warn!("解析失败: 缺少 id 字段");
            return None;
        }
    };
    
    let condition_id = match value.get("conditionId").and_then(|v| v.as_str()) {
        Some(cid) => cid.to_string(),
        None => {
            warn!("解析失败: 缺少 conditionId 字段");
            return None;
        }
    };
    
    let end_date_str = match value.get("endDate").and_then(|v| v.as_str()) {
        Some(eds) => eds,
        None => {
            warn!("解析失败: 缺少 endDate 字段");
            return None;
        }
    };
    
    let end_time = match DateTime::parse_from_rfc3339(end_date_str) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            warn!("解析失败: endDate 格式错误: {}", e);
            return None;
        }
    };

    // 解析 outcomes 和 prices (都是 JSON 字符串)
    let outcomes_str = match value.get("outcomes").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            warn!("解析失败: 缺少 outcomes 字段或不是字符串");
            return None;
        }
    };
    
    let outcome_prices_str = match value.get("outcomePrices").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            warn!("解析失败: 缺少 outcomePrices 字段或不是字符串");
            return None;
        }
    };
    
    let outcomes: Vec<String> = match serde_json::from_str(outcomes_str) {
        Ok(o) => o,
        Err(e) => {
            warn!("解析失败: outcomes JSON 解析错误: {}", e);
            return None;
        }
    };
    
    let prices_str: Vec<String> = match serde_json::from_str(outcome_prices_str) {
        Ok(p) => p,
        Err(e) => {
            warn!("解析失败: outcomePrices JSON 解析错误: {}", e);
            return None;
        }
    };
    
    let prices: Vec<f64> = match prices_str.iter().map(|s| s.parse::<f64>()).collect::<Result<Vec<_>, _>>() {
        Ok(p) => p,
        Err(e) => {
            warn!("解析失败: 价格字符串转数字错误: {}", e);
            return None;
        }
    };
    
    if outcomes.len() != 2 || prices.len() != 2 {
        warn!("解析失败: outcomes 或 prices 长度不等于 2");
        return None;
    }

    // 确定 Up/Down
    let up_idx = match outcomes.iter().position(|o| o.to_lowercase().contains("up")) {
        Some(idx) => idx,
        None => {
            warn!("解析失败: 未找到 'Up' outcome");
            return None;
        }
    };
    let down_idx = 1 - up_idx;

    // 获取 token IDs (用于 CLOB 订阅)
    let clob_token_ids_str = match value.get("clobTokenIds").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            warn!("解析失败: 缺少 clobTokenIds 字段");
            return None;
        }
    };
    
    let clob_token_ids: Vec<String> = match serde_json::from_str(clob_token_ids_str) {
        Ok(ids) => ids,
        Err(e) => {
            warn!("解析失败: clobTokenIds JSON 解析错误: {}", e);
            return None;
        }
    };
    
    if clob_token_ids.len() != 2 {
        warn!("解析失败: clobTokenIds 长度不等于 2");
        return None;
    }
    
    let up_token_id = clob_token_ids[up_idx].clone();
    let down_token_id = clob_token_ids[down_idx].clone();
    
    // 获取开始时间
    let start_date_str = match value.get("startDate").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            warn!("解析失败: 缺少 startDate 字段");
            return None;
        }
    };
    
    let start_time = match DateTime::parse_from_rfc3339(start_date_str) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            warn!("解析失败: startDate 格式错误: {}", e);
            return None;
        }
    };

    info!("✅ 市场解析成功: {} | Up: {:.3} | Down: {:.3}", slug, prices[up_idx], prices[down_idx]);

    Some(Btc5MinMarket {
        market_id,
        slug: slug.clone(),
        condition_id,
        start_time,
        end_time,
        up_token_id,
        down_token_id,
        up_price: prices[up_idx],
        down_price: prices[down_idx],
        btc_start_price: None, // TODO: 从描述或 Chainlink 获取
    })
}

/// BTC 5分钟市场完整信息
#[derive(Debug, Clone)]
pub struct Btc5MinMarket {
    pub market_id: String,
    pub slug: String,
    pub condition_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub up_token_id: String,
    pub down_token_id: String,
    pub up_price: f64,
    pub down_price: f64,
    pub btc_start_price: Option<f64>, // 起始 BTC 价格（从描述中解析或后续获取）
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
    
    /// 格式化显示市场信息
    pub fn display_info(&self, current_btc_price: f64) -> String {
        let btc_change = if self.btc_start_price.is_some() {
            let start = self.btc_start_price.unwrap();
            (current_btc_price - start) / start * 100.0
        } else {
            0.0
        };
        
        format!(
            "📊 {} | 剩余{}s | BTC: ${:.2} ({:+.2}%) | Up: {:.3} | Down: {:.3}",
            self.slug,
            self.seconds_to_end(),
            current_btc_price,
            btc_change,
            self.up_price,
            self.down_price
        )
    }
}
