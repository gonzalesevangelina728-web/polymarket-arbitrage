use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use tracing::{info, error};

use crate::types::{Trade, TradeSide};

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        
        // 创建交易表
        conn.execute(
            "CREATE TABLE IF NOT EXISTS paper_trades (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                market_id TEXT NOT NULL,
                side TEXT NOT NULL,
                price REAL NOT NULL,
                amount REAL NOT NULL,
                btc_price_at_entry REAL NOT NULL,
                btc_change_1m REAL NOT NULL,
                time_to_end INTEGER NOT NULL,
                settled BOOLEAN DEFAULT FALSE,
                pnl REAL
            )",
            [],
        )?;
        
        // 创建市场快照表
        conn.execute(
            "CREATE TABLE IF NOT EXISTS market_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                market_id TEXT NOT NULL,
                up_price REAL NOT NULL,
                down_price REAL NOT NULL,
                btc_price REAL NOT NULL,
                btc_change_1m REAL NOT NULL,
                time_to_end INTEGER NOT NULL
            )",
            [],
        )?;
        
        info!("Database initialized at {}", db_path);
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
    
    pub fn save_trade(&self, trade: &Trade) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        let side_str = match trade.side {
            TradeSide::Up => "UP",
            TradeSide::Down => "DOWN",
        };
        
        conn.execute(
            "INSERT INTO paper_trades 
             (id, timestamp, market_id, side, price, amount, btc_price_at_entry, btc_change_1m, time_to_end)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                trade.id,
                trade.timestamp.to_rfc3339(),
                trade.market_id,
                side_str,
                trade.price,
                trade.amount,
                trade.btc_price_at_entry,
                trade.btc_change_1m,
                trade.time_to_end,
            ],
        )?;
        
        info!(
            "💾 Saved trade to DB: {} {} @ ${:.3}",
            trade.side, trade.market_id, trade.price
        );
        
        Ok(())
    }
    
    pub fn save_snapshot(
        &self,
        timestamp: DateTime<Utc>,
        market_id: &str,
        up_price: f64,
        down_price: f64,
        btc_price: f64,
        btc_change_1m: f64,
        time_to_end: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "INSERT INTO market_snapshots 
             (timestamp, market_id, up_price, down_price, btc_price, btc_change_1m, time_to_end)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                timestamp.to_rfc3339(),
                market_id,
                up_price,
                down_price,
                btc_price,
                btc_change_1m,
                time_to_end,
            ],
        )?;
        
        Ok(())
    }
    
    pub fn get_completed_arbitrages(&self) -> Result<Vec<(String, f64, f64)>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT market_id, 
                    SUM(CASE WHEN side='UP' THEN price ELSE 0 END) as up_cost,
                    SUM(CASE WHEN side='DOWN' THEN price ELSE 0 END) as down_cost
             FROM paper_trades
             GROUP BY market_id
             HAVING COUNT(*) = 2"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        
        Ok(results)
    }
    
    pub fn generate_report(&self) -> Result<()> {
        let completed = self.get_completed_arbitrages()?;
        
        println!("\n{}", "=".repeat(60));
        println!("📊 Paper Trading Report");
        println!("{}", "=".repeat(60));
        println!("Completed arbitrages: {}", completed.len());
        
        if !completed.is_empty() {
            let mut total_pnl = 0.0;
            for (market_id, up_cost, down_cost) in completed {
                let total_cost = up_cost + down_cost;
                let profit = 1.0 - total_cost;
                total_pnl += profit;
                let profit_pct = profit / total_cost * 100.0;
                println!(
                    "  {}: Cost=${:.3} (Up${:.3}+Down${:.3}) Profit=${:.3} ({:.1}%)",
                    market_id, total_cost, up_cost, down_cost,
                    profit, profit_pct
                );
            }
            println!("\n💰 Total expected PnL: ${:.2}", total_pnl);
        }
        
        println!("{}", "=".repeat(60));
        
        Ok(())
    }

    pub fn print_stats(&self) -> Result<()> {
        self.generate_report()
    }
}
