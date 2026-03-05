#!/usr/bin/env python3
"""
Polymarket 套利监控脚本 v3.1
直接使用 Gamma API 的 outcomePrices，单次请求完成筛选
"""

import os
import time
import json
import asyncio
import aiohttp
from datetime import datetime, timedelta
from typing import Dict, List, Optional, Tuple
from dataclasses import dataclass, asdict
from dotenv import load_dotenv
import logging
import sqlite3

load_dotenv()

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler('arbitrage.log'),
        logging.StreamHandler()
    ]
)
logger = logging.getLogger(__name__)


@dataclass
class ArbitrageOpportunity:
    event_id: str
    event_title: str
    event_slug: str
    market_id: str
    condition_id: str
    outcomes: List[str]
    prices: List[float]
    sum_prices: float
    arbitrage_percent: float
    potential_profit: float
    timestamp: datetime
    end_date: str
    category: str


class DatabaseManager:
    def __init__(self, db_path: str = "arbitrage.db"):
        self.db_path = db_path
        self.init_db()
    
    def init_db(self):
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS opportunities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT,
                event_title TEXT,
                market_id TEXT,
                condition_id TEXT,
                outcomes TEXT,
                prices TEXT,
                sum_prices REAL,
                arbitrage_percent REAL,
                timestamp TEXT
            )
        ''')
        conn.commit()
        conn.close()
    
    def save_opportunity(self, opp: ArbitrageOpportunity):
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        cursor.execute('''
            INSERT INTO opportunities 
            (event_id, event_title, market_id, condition_id, outcomes, prices, sum_prices, arbitrage_percent, timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ''', (
            opp.event_id, opp.event_title, opp.market_id, opp.condition_id,
            json.dumps(opp.outcomes), json.dumps(opp.prices),
            opp.sum_prices, opp.arbitrage_percent, opp.timestamp.isoformat()
        ))
        conn.commit()
        conn.close()


class PolymarketMonitor:
    def __init__(self):
        self.gamma_api = "https://gamma-api.polymarket.com"
        
        self.min_arbitrage_percent = float(os.getenv("MIN_ARBITRAGE_PERCENT", "0.5"))
        self.check_interval = int(os.getenv("CHECK_INTERVAL", "30"))
        self.max_events = int(os.getenv("MAX_EVENTS", "1000"))
        self.min_liquidity = float(os.getenv("MIN_LIQUIDITY", "5000"))
        self.min_liquidity = float(os.getenv("MIN_LIQUIDITY", "5000"))
        
        self.telegram_token = os.getenv("TELEGRAM_BOT_TOKEN", "")
        self.telegram_chat_id = os.getenv("TELEGRAM_CHAT_ID", "")
        
        self.db = DatabaseManager()
        self.notified = set()
        self.total_checks = 0
        self.opportunities_found = 0
    
    async def fetch_events_with_prices(self, session: aiohttp.ClientSession) -> List[Dict]:
        """获取所有事件及其价格（单次API调用）"""
        all_events = []
        limit = 100
        offset = 0
        max_events = self.max_events
        
        while len(all_events) < max_events:
            url = f"{self.gamma_api}/events"
            params = {
                "active": "true",
                "closed": "false",
                "archived": "false",
                "limit": str(limit),
                "offset": str(offset),
                "order": "volume",
                "ascending": "false"
            }
            
            try:
                async with session.get(url, params=params, timeout=aiohttp.ClientTimeout(total=30)) as resp:
                    if resp.status != 200:
                        break
                    data = await resp.json()
                    if not data:
                        break
                    all_events.extend(data)
                    offset += len(data)
                    if len(data) < limit:
                        break
            except Exception as e:
                logger.error(f"获取事件失败: {e}")
                break
        
        logger.info(f"获取 {len(all_events)} 个事件")
        return all_events[:max_events]
    
    def check_market_arbitrage(self, market: Dict, event: Dict) -> Optional[ArbitrageOpportunity]:
        """检查单个市场的套利机会（使用Gamma API自带的价格）"""
        try:
            # 跳过已关闭的市场
            if market.get("closed") or not market.get("active"):
                return None
            
            # 检查流动性
            liquidity = float(market.get("liquidity", 0) or 0)
            if liquidity < self.min_liquidity:
                return None
            
            # 解析 outcomes 和 prices
            outcomes = json.loads(market.get("outcomes", "[]"))
            prices = json.loads(market.get("outcomePrices", "[]"))
            
            if len(outcomes) != len(prices) or len(outcomes) < 2:
                return None
            
            # 转换为浮点数
            prices_float = [float(p) for p in prices]
            
            # 只处理二元市场（Yes/No）
            outcomes_lower = [o.lower().strip() for o in outcomes]
            if set(outcomes_lower) != {"yes", "no"}:
                return None
            
            # 计算价格总和
            sum_prices = sum(prices_float)
            
            # 检查套利条件
            if sum_prices >= 1.0:
                return None
            
            arbitrage_percent = (1.0 - sum_prices) * 100
            
            if arbitrage_percent < self.min_arbitrage_percent:
                return None
            
            # 找到对应的 Yes/No 价格
            yes_price = prices_float[outcomes_lower.index("yes")]
            no_price = prices_float[outcomes_lower.index("no")]
            
            potential_profit = 1000 * (1.0 / sum_prices - 1) if sum_prices > 0 else 0
            
            return ArbitrageOpportunity(
                event_id=event["id"],
                event_title=event.get("title", ""),
                event_slug=event.get("slug", ""),
                market_id=market["id"],
                condition_id=market.get("conditionId", ""),
                outcomes=outcomes,
                prices=prices_float,
                sum_prices=sum_prices,
                arbitrage_percent=arbitrage_percent,
                potential_profit=potential_profit,
                timestamp=datetime.now(),
                end_date=market.get("endDate", ""),
                category=self._get_category(event)
            )
            
        except (json.JSONDecodeError, ValueError) as e:
            return None
    
    def _get_category(self, event: Dict) -> str:
        tags = event.get("tags", [])
        if tags:
            return tags[0].get("label", "Unknown")
        return "Unknown"
    
    async def run_check_cycle(self, session: aiohttp.ClientSession):
        """运行一次检查周期"""
        start_time = time.time()
        
        # 获取所有事件（包含价格）
        events = await self.fetch_events_with_prices(session)
        
        opportunities = []
        checked_markets = 0
        skipped_closed = 0
        skipped_not_binary = 0
        
        # 遍历所有事件和市场
        for event in events:
            markets = event.get("markets", [])
            for market in markets:
                # 跳过已关闭市场
                if market.get("closed") or not market.get("active"):
                    skipped_closed += 1
                    continue
                
                # 检查是否是二元市场（快速预检）
                try:
                    outcomes = json.loads(market.get("outcomes", "[]"))
                    if len(outcomes) != 2 or set(o.lower().strip() for o in outcomes) != {"yes", "no"}:
                        skipped_not_binary += 1
                        continue
                except:
                    skipped_not_binary += 1
                    continue
                
                checked_markets += 1
                opp = self.check_market_arbitrage(market, event)
                if opp:
                    opportunities.append(opp)
                    await self.notify(opp)
        
        elapsed = time.time() - start_time
        self.total_checks += 1
        self.opportunities_found += len(opportunities)
        
        # 输出结果
        logger.info("=" * 70)
        logger.info(f"📊 本轮检查完成 | 用时: {elapsed:.2f}s | 检查市场: {checked_markets}")
        logger.info(f"🎯 发现套利机会: {len(opportunities)}")
        
        if opportunities:
            for opp in opportunities:
                yes_idx = opp.outcomes.index("Yes") if "Yes" in opp.outcomes else 0
                no_idx = opp.outcomes.index("No") if "No" in opp.outcomes else 1
                logger.info(f"   💰 {opp.event_title[:50]}...")
                logger.info(f"      YES: ${opp.prices[yes_idx]:.4f} | NO: ${opp.prices[no_idx]:.4f} | 合计: ${opp.sum_prices:.4f}")
                logger.info(f"      套利空间: {opp.arbitrage_percent:.2f}% | 预估利润: ${opp.potential_profit:.2f}")
        else:
            logger.info("   未发现套利机会")
        
        logger.info("=" * 70)
        
        return opportunities
    
    async def notify(self, opp: ArbitrageOpportunity):
        """发送通知"""
        opp_id = f"{opp.market_id}_{opp.sum_prices:.4f}"
        if opp_id in self.notified:
            return
        self.notified.add(opp_id)
        
        self.db.save_opportunity(opp)
        
        yes_idx = opp.outcomes.index("Yes") if "Yes" in opp.outcomes else 0
        no_idx = opp.outcomes.index("No") if "No" in opp.outcomes else 1
        
        message = f"""🚨 <b>Polymarket 套利机会!</b>

📊 <b>{opp.event_title}</b>

💰 <b>价格:</b>
• YES: ${opp.prices[yes_idx]:.4f}
• NO: ${opp.prices[no_idx]:.4f}
• 合计: ${opp.sum_prices:.4f}

📈 <b>套利空间: {opp.arbitrage_percent:.2f}%</b>
💵 预估利润 ($1000): ${opp.potential_profit:.2f}

🔗 <a href="https://polymarket.com/event/{opp.event_slug}">查看市场</a>
"""
        
        if self.telegram_token and self.telegram_chat_id:
            try:
                url = f"https://api.telegram.org/bot{self.telegram_token}/sendMessage"
                payload = {
                    "chat_id": self.telegram_chat_id,
                    "text": message,
                    "parse_mode": "HTML",
                    "disable_web_page_preview": True
                }
                async with aiohttp.ClientSession() as s:
                    async with s.post(url, json=payload, timeout=10) as resp:
                        if resp.status == 200:
                            logger.info(f"✅ 已发送通知: {opp.event_title[:40]}... ({opp.arbitrage_percent:.2f}%)")
            except Exception as e:
                logger.error(f"通知失败: {e}")
    
    async def run(self):
        logger.info("=" * 70)
        logger.info("Polymarket 套利监控 v3.1 启动")
        logger.info(f"最小套利空间: {self.min_arbitrage_percent}%")
        logger.info(f"检查间隔: {self.check_interval}秒")
        logger.info("=" * 70)
        
        async with aiohttp.ClientSession() as session:
            while True:
                try:
                    await self.run_check_cycle(session)
                    await asyncio.sleep(self.check_interval)
                except Exception as e:
                    logger.error(f"主循环异常: {e}")
                    await asyncio.sleep(5)


def main():
    monitor = PolymarketMonitor()
    try:
        asyncio.run(monitor.run())
    except KeyboardInterrupt:
        logger.info(f"\n监控已停止 | 总计检查: {monitor.total_checks} | 发现机会: {monitor.opportunities_found}")


if __name__ == "__main__":
    main()
