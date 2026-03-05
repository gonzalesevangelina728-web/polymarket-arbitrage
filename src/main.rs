mod config;
mod database;
mod gamma_api;
mod strategy;
mod types;
mod websocket;

use anyhow::Result;
use chrono::Utc;
use database::Database;
use gamma_api::GammaClient;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use strategy::StrategyEngine;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber;
use types::MarketState;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🚀 Polymarket BTC 5分钟套利策略 - 实盘监测模式");

    // 初始化数据库
    let db = Database::new(config::DB_PATH)?;
    info!("✅ 数据库初始化完成: {}", config::DB_PATH);

    // 选择运行模式
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--live" {
        info!("启动实盘监测模式...");
        run_live(db).await?;
    } else {
        info!("启动模拟测试模式（添加 --live 参数运行实盘）...");
        run_simulation(db).await?;
    }

    Ok(())
}

/// 实盘监测模式
async fn run_live(_db: Database) -> Result<()> {
    // 共享状态
    let btc_price = Arc::new(Mutex::new(0.0f64));
    let markets = Arc::new(Mutex::new(HashMap::new()));

    // 1. 启动市场刷新任务
    let markets_clone = Arc::clone(&markets);
    let btc_price_clone = Arc::clone(&btc_price);
    
    tokio::spawn(async move {
        let client = GammaClient::new();
        let mut ticker = interval(Duration::from_secs(10)); // 每10秒检查一次

        loop {
            ticker.tick().await;
            
            match client.get_current_btc_5min_market().await {
                Ok(Some(market)) => {
                    let mut m = markets_clone.lock().unwrap();
                    let btc_price = *btc_price_clone.lock().unwrap();
                    info!("{}", market.display_info(btc_price));
                    m.insert(market.market_id.clone(), market);
                }
                Ok(None) => {
                    debug!("当前无活跃 BTC 5分钟市场");
                }
                Err(e) => {
                    warn!("获取市场失败: {}", e);
                }
            }
        }
    });

    // 2. 主循环
    info!("✅ 服务已启动，开始监测...");
    info!("按 Ctrl+C 停止");

    let mut ticker = interval(Duration::from_secs(5));
    
    loop {
        ticker.tick().await;

        let price = *btc_price.lock().unwrap();
        let markets_snapshot = markets.lock().unwrap().clone();

        info!("心跳 | BTC: ${:.2}, 活跃市场: {} 个", price, markets_snapshot.len());

        // TODO: 连接 RTDS 获取实时价格，执行策略
    }
}

/// 模拟测试模式
async fn run_simulation(db: Database) -> Result<()> {
    use chrono::Duration;

    let mut strategy = StrategyEngine::new();

    info!("\n=== 测试场景1: Up入场信号 ===");
    let state1 = MarketState {
        market_id: "btc-updown-5m-test1".to_string(),
        end_time: Utc::now() + Duration::seconds(180),
        up_price: 0.18,
        down_price: 0.75,
        up_ask: 0.20,
        down_ask: 0.62,
        btc_price: 85000.0,
        btc_change_1m: -0.035,
        btc_change_5m: -0.06,
        timestamp: Utc::now(),
    };

    let trades1 = strategy.process_market_update(&state1);
    for trade in &trades1 {
        info!("生成交易: {:?} @ ${:.3}", trade.side, trade.price);
        db.save_trade(trade)?;
    }

    info!("\n=== 测试场景2: Down补仓信号 ===");
    let state2 = MarketState {
        market_id: "btc-updown-5m-test1".to_string(),
        end_time: Utc::now() + Duration::seconds(120),
        up_price: 0.25,
        down_price: 0.65,
        up_ask: 0.27,
        down_ask: 0.60,
        btc_price: 85200.0,
        btc_change_1m: 0.025,
        btc_change_5m: -0.04,
        timestamp: Utc::now(),
    };

    let trades2 = strategy.process_market_update(&state2);
    for trade in &trades2 {
        info!("生成交易: {:?} @ ${:.3}", trade.side, trade.price);
        db.save_trade(trade)?;
    }

    info!("\n=== 统计报告 ===");
    db.print_stats()?;

    Ok(())
}
