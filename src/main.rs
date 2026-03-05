mod config;
mod database;
mod strategy;
mod types;
mod websocket;

use anyhow::Result;
use database::Database;
use strategy::StrategyEngine;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🚀 Polymarket BTC 5分钟套利策略启动");
    info!("配置参数:");
    info!("  - Up买入阈值: < ${}", config::UP_PRICE_THRESHOLD);
    info!("  - Down补仓阈值: < ${}", config::DOWN_PRICE_THRESHOLD);
    info!("  - 最大总成本: ${}", config::MAX_TOTAL_COST);
    info!(
        "  - BTC 1分钟跌幅触发: {}%",
        config::BTC_1M_DROP_TRIGGER * 100.0
    );
    info!("  - BTC反弹触发: {}%", config::BTC_BOUNCE_TRIGGER * 100.0);
    info!(
        "  - 交易窗口: T-{}s 到 T-{}s",
        config::ENTRY_WINDOW_START,
        config::ENTRY_WINDOW_END
    );
    info!("  - 虚拟交易金额: ${}", config::TRADE_AMOUNT);

    // 初始化数据库
    let db = Database::new("paper_trades.db")?;
    info!("✅ 数据库初始化完成");

    // 初始化策略引擎
    let mut strategy = StrategyEngine::new();

    // TODO: 启动WebSocket连接
    // 这里先做一个简单的测试循环
    info!("开始模拟运行...");

    // 模拟一些市场数据来测试策略逻辑
    test_strategy(&mut strategy, &db).await?;

    info!("运行结束");
    Ok(())
}

async fn test_strategy(strategy: &mut StrategyEngine, db: &Database) -> Result<()> {
    use chrono::{Duration, Utc};
    use types::{MarketState, TradeSide};

    // 模拟场景1: 满足Up入场条件
    info!("\n=== 测试场景1: Up入场信号 ===");
    let state1 = MarketState {
        market_id: "btc-updown-5m-test1".to_string(),
        end_time: Utc::now() + Duration::seconds(180), // T-3:00
        up_price: 0.18,
        down_price: 0.75,
        up_ask: 0.20,
        down_ask: 0.62,
        btc_price: 85000.0,
        btc_change_1m: -0.035, // -3.5%
        btc_change_5m: -0.06,
        timestamp: Utc::now(),
    };

    let trades1 = strategy.process_market_update(&state1);
    for trade in &trades1 {
        info!("生成交易: {:?} @ ${:.3}", trade.side, trade.price);
        db.save_trade(trade)?;
    }

    // 模拟场景2: 满足Down补仓条件
    info!("\n=== 测试场景2: Down补仓信号 ===");
    let state2 = MarketState {
        market_id: "btc-updown-5m-test1".to_string(),
        end_time: Utc::now() + Duration::seconds(120), // T-2:00
        up_price: 0.25,
        down_price: 0.65,
        up_ask: 0.27,
        down_ask: 0.60,
        btc_price: 85200.0,
        btc_change_1m: 0.025, // +2.5% 反弹
        btc_change_5m: -0.04,
        timestamp: Utc::now(),
    };

    let trades2 = strategy.process_market_update(&state2);
    for trade in &trades2 {
        info!("生成交易: {:?} @ ${:.3}", trade.side, trade.price);
        db.save_trade(trade)?;
    }

    // 模拟场景3: 不满足条件（价格不够低）
    info!("\n=== 测试场景3: 不满足条件 ===");
    let state3 = MarketState {
        market_id: "btc-updown-5m-test2".to_string(),
        end_time: Utc::now() + Duration::seconds(200),
        up_price: 0.45,
        down_price: 0.55,
        up_ask: 0.47,
        down_ask: 0.53,
        btc_price: 86000.0,
        btc_change_1m: -0.01,
        btc_change_5m: -0.02,
        timestamp: Utc::now(),
    };

    let trades3 = strategy.process_market_update(&state3);
    if trades3.is_empty() {
        info!("无交易信号（符合预期）");
    }

    // 打印统计
    info!("\n=== 统计报告 ===");
    db.print_stats()?;

    Ok(())
}
