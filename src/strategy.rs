use crate::config::*;
use crate::types::{MarketState, Position, Trade, TradeSide};
use chrono::Utc;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct StrategyEngine {
    positions: std::collections::HashMap<String, Position>,
}

impl StrategyEngine {
    pub fn new() -> Self {
        Self {
            positions: std::collections::HashMap::new(),
        }
    }

    /// 检查是否在交易时间窗口内 (T-4:00 到 T-1:30)
    fn in_trading_window(&self, time_to_end: i64) -> bool {
        time_to_end <= ENTRY_WINDOW_START && time_to_end >= ENTRY_WINDOW_END
    }

    /// 检查是否满足买入Up条件
    pub fn check_up_entry(&self, state: &MarketState) -> Option<Trade> {
        let time_to_end = state.time_to_end();

        // 检查时间窗口
        if !self.in_trading_window(time_to_end) {
            debug!(
                "不在交易窗口: {}s (窗口: {}-{}s)",
                time_to_end, ENTRY_WINDOW_END, ENTRY_WINDOW_START
            );
            return None;
        }

        // 检查Up价格阈值
        if state.up_ask > UP_PRICE_THRESHOLD {
            debug!("Up价格不够低: {:.3} > {:.3}", state.up_ask, UP_PRICE_THRESHOLD);
            return None;
        }

        // 检查BTC跌幅触发
        if state.btc_change_1m > BTC_1M_DROP_TRIGGER {
            debug!(
                "BTC跌幅不够: {:.2}% > {:.2}%",
                state.btc_change_1m * 100.0,
                BTC_1M_DROP_TRIGGER * 100.0
            );
            return None;
        }

        info!(
            "🎯 UP入场信号! Up@{:.3}, BTC跌{:.2}%, 剩余{}s",
            state.up_ask,
            state.btc_change_1m * 100.0,
            time_to_end
        );

        Some(Trade {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            market_id: state.market_id.clone(),
            side: TradeSide::Up,
            price: state.up_ask,
            amount: TRADE_AMOUNT,
            btc_price_at_entry: state.btc_price,
            btc_change_1m: state.btc_change_1m,
            time_to_end,
        })
    }

    /// 检查是否满足买入Down条件（补仓）
    pub fn check_down_entry(&self, state: &MarketState, up_entry_price: f64) -> Option<Trade> {
        let time_to_end = state.time_to_end();

        // 检查时间窗口
        if !self.in_trading_window(time_to_end) {
            return None;
        }

        // 计算总成本
        let total_cost = up_entry_price + state.down_ask;
        if total_cost > MAX_TOTAL_COST {
            debug!(
                "总成本过高: {:.3} > {:.3} (Up@{:.3} + Down@{:.3})",
                total_cost, MAX_TOTAL_COST, up_entry_price, state.down_ask
            );
            return None;
        }

        // 检查BTC反弹触发
        if state.btc_change_1m < BTC_BOUNCE_TRIGGER {
            debug!(
                "BTC未反弹: {:.2}% < {:.2}%",
                state.btc_change_1m * 100.0,
                BTC_BOUNCE_TRIGGER * 100.0
            );
            return None;
        }

        info!(
            "🎯 DOWN补仓信号! Down@{:.3}, 总成本{:.3}, BTC反弹{:.2}%",
            state.down_ask,
            total_cost,
            state.btc_change_1m * 100.0
        );

        Some(Trade {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            market_id: state.market_id.clone(),
            side: TradeSide::Down,
            price: state.down_ask,
            amount: TRADE_AMOUNT,
            btc_price_at_entry: state.btc_price,
            btc_change_1m: state.btc_change_1m,
            time_to_end,
        })
    }

    /// 处理市场更新，返回可能的交易信号
    pub fn process_market_update(&mut self, state: &MarketState) -> Vec<Trade> {
        let mut trades = Vec::new();
        let market_id = &state.market_id;

        // 检查是否已有该市场的仓位
        let has_position = self.positions.contains_key(market_id);
        
        if !has_position {
            // 尝试开第一腿（Up）
            if let Some(trade) = self.check_up_entry(state) {
                let mut position = Position::new(market_id.clone());
                position.up_entry = Some(trade.clone());
                self.positions.insert(market_id.clone(), position);
                trades.push(trade);
            }
        } else {
            // 获取现有仓位信息（克隆数据避免借用冲突）
            let up_entry_price = self.positions.get(market_id)
                .and_then(|p| p.up_entry.as_ref().map(|t| t.price));
            let has_down = self.positions.get(market_id)
                .map(|p| p.down_entry.is_some())
                .unwrap_or(true);
            
            // 已有Up但没有Down，尝试补仓
            if let Some(up_price) = up_entry_price {
                if !has_down {
                    if let Some(trade) = self.check_down_entry(state, up_price) {
                        if let Some(position) = self.positions.get_mut(market_id) {
                            position.down_entry = Some(trade.clone());
                            trades.push(trade);

                            // 记录完成的套利对
                            if let Some(total_cost) = position.total_cost() {
                                let profit = 1.0 - total_cost;
                                info!(
                                    "✅ 套利对完成! 成本: {:.3}, 预期利润: {:.3} ({:.1}%)",
                                    total_cost,
                                    profit,
                                    profit / total_cost * 100.0
                                );
                            }
                        }
                    }
                }
            }
        }

        trades
    }

    /// 清理已结算的仓位
    pub fn cleanup_settled_positions(&mut self, settled_markets: &[String]) {
        for market_id in settled_markets {
            if self.positions.remove(market_id).is_some() {
                info!("清理已结算仓位: {}", market_id);
            }
        }
    }

    /// 获取当前活跃仓位数
    pub fn active_position_count(&self) -> usize {
        self.positions.len()
    }
}
