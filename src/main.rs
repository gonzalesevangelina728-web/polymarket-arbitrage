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
use websocket::{BinanceBtcClient, PolymarketWsClient};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🚀 Polymarket BTC 5分钟套利策略 - 实盘监测模式");
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

    // 初始化数据库
    let db = Database::new(config::DB_PATH)?;
    info!("✅ 数据库初始化完成: {}", config::DB_PATH);

    // 初始化策略引擎
    let strategy = StrategyEngine::new();

    // 选择运行模式
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--live" {
        info!("启动实盘监测模式...");
        run_live(strategy, db).await?;
    } else {
        info!("启动模拟测试模式（添加 --live 参数运行实盘）...");
        run_simulation(strategy, db).await?;
    }

    Ok(())
}

/// 实盘监测模式
async fn run_live(mut _strategy: StrategyEngine, _db: Database) -> Result<()> {
    info!("连接数据源...");

    // 连接 Polymarket WebSocket
    let mut polymarket_client = PolymarketWsClient::new();
    if let Err(e) = polymarket_client.connect().await {
        error!("Polymarket 连接失败: {}", e);
        return Ok(());
    }

    // 连接 Binance BTC 价格源
    let binance_client = BinanceBtcClient::connect().await?;

    // 创建任务通道
    let (tx, mut rx) = tokio::sync::mpsc::channel::<types::MarketState>(100);

    // 启动 BTC 价格监听任务
    tokio::spawn(async move {
        if let Err(e) = binance_client.run(|price| {
            // 更新 BTC 价格
            info!("BTC 价格更新: ${:.2}", price);
        }).await {
            error!("Binance 连接错误: {}", e);
        }
    });

    // 启动 Polymarket 监听任务
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = polymarket_client.run(|market_state| {
            // 发送市场更新到主循环
            let _ = tx_clone.try_send(market_state);
        }).await {
            error!("Polymarket 连接错误: {}", e);
        }
    });

    info!("✅ 所有数据源已连接，开始监测...");
    info!("按 Ctrl+C 停止");

    // 主循环处理市场更新
    while let Some(market_state) = rx.recv().await {
        // TODO: 处理市场更新，执行策略
        info!("收到市场更新: {:?}", market_state.market_id);
    }

    Ok(())
}

/// 模拟测试模式
async fn run_simulation(mut strategy: StrategyEngine, db: Database) -> Result<()> {
    use chrono::{Duration, Utc};
    use types::MarketState;

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

    info!("\n=== 统计报告 ===");
    db.print_stats()?;

    Ok(())
}
