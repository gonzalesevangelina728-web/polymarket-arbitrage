# Polymarket 套利监控脚本

## 功能
- 实时监控 Polymarket 市场
- 发现 YES + NO < $1 的套利机会
- 计算潜在收益率
- 推送通知（Telegram/Discord）

## 你需要提供的信息

### 1. **API 端点**（必需）
```bash
# Gamma API（免费，只读）
GAMMA_API_URL=https://gamma-api.polymarket.com

# Data API（需要 API Key）
DATA_API_URL=https://data-api.polymarket.com
DATA_API_KEY=your_api_key_here
```

### 2. **通知渠道**（至少一个）
```bash
# Telegram Bot
TELEGRAM_BOT_TOKEN=your_bot_token
TELEGRAM_CHAT_ID=your_chat_id

# Discord Webhook
DISCORD_WEBHOOK_URL=https://discord.com/api/webhooks/...
```

### 3. **监控参数**（可选，有默认值）
```bash
# 最小套利空间（默认 1%）
MIN_ARBITRAGE_PERCENT=1.0

# 检查间隔（秒，默认 30）
CHECK_INTERVAL=30

# 最小流动性（美元，默认 10000）
MIN_LIQUIDITY=10000

# 市场类型过滤（可选）
# sports, politics, crypto, etc.
MARKET_TAGS=sports
```

### 4. **运行环境**
- Python 3.9+
- 服务器/VPS（推荐）或本地电脑
- 网络连接稳定

---

## 快速开始

### 1. 安装依赖
```bash
pip install -r requirements.txt
```

### 2. 配置环境变量
```bash
cp .env.example .env
# 编辑 .env 文件填入你的信息
```

### 3. 运行监控
```bash
python monitor.py
```

### 4. 后台运行（Linux/Mac）
```bash
nohup python monitor.py > monitor.log 2>&1 &
```

---

## 获取 API Key

1. 访问 https://polymarket.com
2. 登录账户
3. 进入 Settings → API
4. 生成 Read-Only API Key

## 创建 Telegram Bot

1. 找 @BotFather
2. 发送 /newbot
3. 获取 Bot Token
4. 找 @userinfobot 获取你的 Chat ID
