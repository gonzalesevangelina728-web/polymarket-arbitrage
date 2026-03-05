#!/usr/bin/env python3
"""
Polymarket BTC 5分钟套利策略 - 虚拟交易回测
实盘数据 + 模拟成交
"""

import asyncio
import json
import sqlite3
from dataclasses import dataclass, asdict
from datetime import datetime, timedelta
from typing import Optional, Dict, List
import websockets
import aiohttp


@dataclass
class Trade:
    """虚拟交易记录"""
    timestamp: str
    market_id: str
    side: str  # 'UP' or 'DOWN'
    price: float
    amount: float
    btc_price_at_entry: float
    btc_change_1m: float
    time_to_end: int  # 秒
    is_paper: bool = True  # 标记为虚拟交易


@dataclass
class MarketState:
    """市场状态快照"""
    market_id: str
    end_time: datetime
    up_price: float
    down_price: float
    up_ask: float
    down_ask: float
    btc_price: float
    btc_change_1m: float
    btc_change_5m: float
    timestamp: datetime


class PaperTradingEngine:
    """虚拟交易引擎"""
    
    # 策略参数
    UP_PRICE_THRESHOLD = 0.25      # Up买入阈值
    DOWN_PRICE_THRESHOLD = 0.70    # Down买入阈值  
    MAX_TOTAL_COST = 0.90          # 最大总成本
    BTC_1M_DROP_TRIGGER = -0.03    # BTC 1分钟跌幅触发
    BTC_BOUNCE_TRIGGER = 0.02      # BTC反弹触发
    MIN_TIME_REMAINING = 90        # 最短剩余时间(秒)
    
    def __init__(self, db_path: str = "paper_trades.db"):
        self.db_path = db_path
        self.active_positions: Dict[str, Dict] = {}  # market_id -> position
        self.trade_history: List[Trade] = []
        self.btc_price_history: List[tuple] = []  # (timestamp, price)
        self.init_db()
        
    def init_db(self):
        """初始化数据库"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS paper_trades (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT,
                market_id TEXT,
                side TEXT,
                price REAL,
                amount REAL,
                btc_price_at_entry REAL,
                btc_change_1m REAL,
                time_to_end INTEGER,
                paired_trade_id INTEGER,
                settled BOOLEAN DEFAULT FALSE,
                pnl REAL
            )
        ''')
        
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS market_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT,
                market_id TEXT,
                up_price REAL,
                down_price REAL,
                btc_price REAL,
                btc_change_1m REAL,
                time_to_end INTEGER
            )
        ''')
        
        conn.commit()
        conn.close()
    
    def calculate_btc_change(self, current_price: float, seconds_back: int = 60) -> float:
        """计算BTC涨跌幅"""
        if not self.btc_price_history:
            return 0.0
            
        cutoff_time = datetime.now() - timedelta(seconds=seconds_back)
        
        # 找到seconds_back前的价格
        for ts, price in reversed(self.btc_price_history):
            if ts <= cutoff_time:
                return (current_price - price) / price
                
        return 0.0
    
    def check_up_entry(self, state: MarketState) -> Optional[Dict]:
        """检查是否满足买入Up条件"""
        conditions = {
            'up_price_low': state.up_ask <= self.UP_PRICE_THRESHOLD,
            'time_ok': state.time_to_end >= self.MIN_TIME_REMAINING,
            'btc_dropping': state.btc_change_1m <= self.BTC_1M_DROP_TRIGGER,
        }
        
        if all(conditions.values()):
            return {
                'signal': 'BUY_UP',
                'price': state.up_ask,
                'conditions': conditions,
                'reason': f"Up超跌@{state.up_ask:.2f}, BTC 1分钟跌{state.btc_change_1m*100:.1f}%"
            }
        return None
    
    def check_down_entry(self, state: MarketState, up_entry_price: float) -> Optional[Dict]:
        """检查是否满足买入Down条件（补仓）"""
        total_cost = up_entry_price + state.down_ask
        
        conditions = {
            'total_cost_ok': total_cost <= self.MAX_TOTAL_COST,
            'time_ok': state.time_to_end >= self.MIN_TIME_REMAINING,
            'btc_bouncing': state.btc_change_1m >= self.BTC_BOUNCE_TRIGGER,
        }
        
        if all(conditions.values()):
            return {
                'signal': 'BUY_DOWN',
                'price': state.down_ask,
                'total_cost': total_cost,
                'conditions': conditions,
                'reason': f"补仓Down@{state.down_ask:.2f}, 总成本{total_cost:.2f}, BTC反弹{state.btc_change_1m*100:.1f}%"
            }
        return None
    
    async def process_market_update(self, state: MarketState):
        """处理市场更新"""
        # 保存市场快照
        self.save_snapshot(state)
        
        market_id = state.market_id
        
        # 检查是否有活跃仓位
        if market_id not in self.active_positions:
            # 尝试开第一腿（Up）
            signal = self.check_up_entry(state)
            if signal:
                await self.execute_paper_trade(market_id, 'UP', signal, state)
        else:
            position = self.active_positions[market_id]
            
            # 已有Up仓位，尝试补Down
            if 'down_entry' not in position:
                signal = self.check_down_entry(state, position['up_entry']['price'])
                if signal:
                    await self.execute_paper_trade(market_id, 'DOWN', signal, state)
    
    async def execute_paper_trade(self, market_id: str, side: str, signal: Dict, state: MarketState):
        """执行虚拟交易"""
        trade = Trade(
            timestamp=datetime.now().isoformat(),
            market_id=market_id,
            side=side,
            price=signal['price'],
            amount=100.0,  # 虚拟金额 $100
            btc_price_at_entry=state.btc_price,
            btc_change_1m=state.btc_change_1m,
            time_to_end=state.time_to_end
        )
        
        # 记录到内存
        self.trade_history.append(trade)
        
        # 更新仓位状态
        if market_id not in self.active_positions:
            self.active_positions[market_id] = {}
        
        self.active_positions[market_id][f'{side.lower()}_entry'] = {
            'price': signal['price'],
            'time': datetime.now(),
            'trade': trade
        }
        
        # 保存到数据库
        self.save_trade(trade)
        
        print(f"📝 [PAPER TRADE] {side} @ ${signal['price']:.3f}")
        print(f"   原因: {signal['reason']}")
        print(f"   BTC价格: ${state.btc_price:,.2f}")
        print(f"   剩余时间: {state.time_to_end}s")
        print("-" * 50)
    
    def save_trade(self, trade: Trade):
        """保存交易到数据库"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        cursor.execute('''
            INSERT INTO paper_trades 
            (timestamp, market_id, side, price, amount, btc_price_at_entry, btc_change_1m, time_to_end)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ''', (
            trade.timestamp, trade.market_id, trade.side, 
            trade.price, trade.amount, trade.btc_price_at_entry,
            trade.btc_change_1m, trade.time_to_end
        ))
        conn.commit()
        conn.close()
    
    def save_snapshot(self, state: MarketState):
        """保存市场快照"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        cursor.execute('''
            INSERT INTO market_snapshots 
            (timestamp, market_id, up_price, down_price, btc_price, btc_change_1m, time_to_end)
            VALUES (?, ?, ?, ?, ?, ?, ?)
        ''', (
            state.timestamp.isoformat(), state.market_id,
            state.up_price, state.down_price, state.btc_price,
            state.btc_change_1m, state.time_to_end
        ))
        conn.commit()
        conn.close()
    
    def generate_report(self):
        """生成交易报告"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        # 统计已完成的套利对
        cursor.execute('''
            SELECT market_id, 
                   SUM(CASE WHEN side='UP' THEN price ELSE 0 END) as up_cost,
                   SUM(CASE WHEN side='DOWN' THEN price ELSE 0 END) as down_cost,
                   COUNT(*) as legs
            FROM paper_trades
            GROUP BY market_id
            HAVING legs = 2
        ''')
        
        completed_arbitrages = cursor.fetchall()
        
        print("\n" + "="*60)
        print("📊 虚拟交易报告")
        print("="*60)
        
        total_trades = len(self.trade_history)
        completed_pairs = len(completed_arbitrages)
        
        print(f"总交易数: {total_trades}")
        print(f"完成套利对: {completed_pairs}")
        
        if completed_arbitrages:
            total_pnl = 0
            for market_id, up_cost, down_cost, _ in completed_arbitrages:
                total_cost = up_cost + down_cost
                profit = 1.0 - total_cost  # 假设结算为$1
                total_pnl += profit
                print(f"\n  {market_id}:")
                print(f"    成本: ${total_cost:.3f} (Up${up_cost:.3f} + Down${down_cost:.3f})")
                print(f"    利润: ${profit:.3f} ({profit/total_cost*100:.1f}%)")
            
            print(f"\n💰 总预期利润: ${total_pnl:.2f}")
        
        conn.close()


# 使用示例
async def main():
    engine = PaperTradingEngine()
    
    print("🚀 启动虚拟交易引擎...")
    print("监控条件:")
    print(f"  - Up买入阈值: < ${engine.UP_PRICE_THRESHOLD}")
    print(f"  - Down补仓阈值: < ${engine.DOWN_PRICE_THRESHOLD}")
    print(f"  - BTC 1分钟跌幅触发: {engine.BTC_1M_DROP_TRIGGER*100:.0f}%")
    print(f"  - 最大总成本: ${engine.MAX_TOTAL_COST}")
    print("="*60)
    
    # 这里接入实际的数据源
    # await engine.run_live()
    
    # 测试数据示例
    test_state = MarketState(
        market_id="btc-updown-5m-test",
        end_time=datetime.now() + timedelta(minutes=3),
        up_price=0.18,
        down_price=0.75,
        up_ask=0.20,
        down_ask=0.62,
        btc_price=85000,
        btc_change_1m=-0.035,  # -3.5%
        btc_change_5m=-0.06,
        timestamp=datetime.now()
    )
    
    await engine.process_market_update(test_state)
    
    # 模拟后续补仓
    await asyncio.sleep(1)
    
    test_state2 = MarketState(
        market_id="btc-updown-5m-test",
        end_time=datetime.now() + timedelta(minutes=2),
        up_price=0.25,
        down_price=0.65,
        up_ask=0.27,
        down_ask=0.60,
        btc_price=85200,
        btc_change_1m=0.025,  # +2.5% 反弹
        btc_change_5m=-0.04,
        timestamp=datetime.now()
    )
    
    await engine.process_market_update(test_state2)
    
    # 生成报告
    engine.generate_report()


if __name__ == "__main__":
    asyncio.run(main())
