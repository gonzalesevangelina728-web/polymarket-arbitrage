mod config;
mod database;
mod strategy;
mod types;
mod websocket;

use anyhow::Result;
use chrono::Utc;
use database::Database;
use std::sync::{Arc, Mutex};
use strategy::StrategyEngine;
use tracing::{error, info, Level};
use tracing_subscriber;
use websocket::{PolymarketClobClient, PolymarketRtdsClient};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🚀 Polymarket BTC 5分钟套利策略 - 实盘监测模式");
    info!("使用 Polymarket 同源数据 (RTDS)");

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
async fn run_live(db: Database) -> Result<()> {
    // 共享状态
    let btc_price = Arc::new(Mutex::new(0.0f64));
    let btc_change_1m = Arc::new(Mutex::new(0.0f64));
    let strategy = Arc::new(Mutex::new(StrategyEngine::new()));

    // 1. 启动 RTDS 客户端 (BTC 价格)
    let btc_price_clone = Arc::clone(&btc_price);
    let btc_change_clone = Arc::clone(&btc_change_1m);
    
    tokio::spawn(async move {
        let mut rtds = PolymarketRtdsClient::new();
        
        if let Err(e) = rtds.connect().await {
            error!("RTDS 连接失败: {}", e);
            return;
        }
        
        if let Err(e) = rtds.subscribe_btc_price().await {
            error!("订阅 BTC 价格失败: {}", e);
            return;
        }
        
        info!("✅ BTC 价格源已连接");
        
        if let Err(e) = rtds.run(|price, source| {
            let mut p = btc_price_clone.lock().unwrap();
            *p = price;
            
            let change = rtds.get_btc_change_1m();
            let mut c = btc_change_clone.lock().unwrap();
            *c = change;
            
            info!("BTC: ${:.2} ({}), 1m: {:.2}%", price, source, change * 100.0);
        }).await {
            error!("RTDS 运行错误: {}", e);
        }
    });

    // 2. 启动 CLOB 客户端 (订单簿)
    // TODO: 需要获取当前活跃的 BTC 5分钟市场 asset_id
    info!("⚠️ 订单簿监测待实现 - 需要 BTC 5分钟市场 asset_id");

    // 主循环：打印状态
    info!("按 Ctrl+C 停止监测");
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        
        let price = *btc_price.lock().unwrap();
        let change = *btc_change_1m.lock().unwrap();
        
        if price > 0.0 {
            info!("心跳 | BTC: ${:.2}, 1m涨跌: {:.2}%", price, change * 100.0);
        }
    }
}

/// 模拟测试模式
async fn run_simulation(db: Database) -> Result<()> {
    use chrono::Duration;
    use types::MarketState;

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
