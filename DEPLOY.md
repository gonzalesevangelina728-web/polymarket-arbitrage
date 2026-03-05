# Polymarket 套利监控 - 部署指南

## 快速开始（5分钟）

### 1. 安装依赖
```bash
cd polymarket-arbitrage-monitor
pip install -r requirements.txt
```

### 2. 配置环境变量
```bash
cp .env.example .env
# 编辑 .env 文件，填入你的配置
```

### 3. 运行监控
```bash
python monitor.py
```

---

## 详细部署方案

### 方案 A：本地运行（测试/开发）

```bash
# 创建虚拟环境
python -m venv venv
source venv/bin/activate  # Linux/Mac
# 或 venv\Scripts\activate  # Windows

# 安装依赖
pip install -r requirements.txt

# 配置
cp .env.example .env
nano .env  # 编辑配置

# 运行
python monitor.py
```

### 方案 B：服务器部署（推荐）

#### 使用 systemd（Linux）

创建服务文件 `/etc/systemd/system/polymarket-monitor.service`：

```ini
[Unit]
Description=Polymarket Arbitrage Monitor
After=network.target

[Service]
Type=simple
User=your_username
WorkingDirectory=/path/to/polymarket-arbitrage-monitor
Environment=PYTHONUNBUFFERED=1
ExecStart=/path/to/venv/bin/python monitor.py
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

启动服务：
```bash
sudo systemctl daemon-reload
sudo systemctl enable polymarket-monitor
sudo systemctl start polymarket-monitor

# 查看状态
sudo systemctl status polymarket-monitor

# 查看日志
sudo journalctl -u polymarket-monitor -f
```

#### 使用 Docker

创建 `Dockerfile`：

```dockerfile
FROM python:3.11-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

CMD ["python", "monitor.py"]
```

构建并运行：
```bash
docker build -t polymarket-monitor .
docker run -d \
  --name polymarket-monitor \
  --restart unless-stopped \
  -v $(pwd)/arbitrage.db:/app/arbitrage.db \
  -v $(pwd)/arbitrage.log:/app/arbitrage.log \
  polymarket-monitor
```

### 方案 C：云服务器（VPS）

推荐服务商：
- DigitalOcean ($5/月)
- AWS Lightsail ($3.5/月)
- Vultr ($2.5/月)

部署步骤：
```bash
# 1. 连接服务器
ssh root@your_server_ip

# 2. 安装 Python
apt update && apt install -y python3 python3-pip python3-venv git

# 3. 克隆代码
git clone <your-repo-url>
cd polymarket-arbitrage-monitor

# 4. 安装依赖
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt

# 5. 配置
cp .env.example .env
nano .env

# 6. 使用 screen 后台运行
screen -S polymarket
python monitor.py
# Ctrl+A, D  detach

# 7. 重新连接
screen -r polymarket
```

---

## 获取必要信息

### 1. Telegram Bot Token

1. 在 Telegram 搜索 @BotFather
2. 发送 `/newbot`
3. 按提示设置名称和用户名
4. 保存返回的 Token（格式：`123456789:ABCdefGHIjklMNOpqrsTUVwxyz`）

### 2. Telegram Chat ID

1. 在 Telegram 搜索 @userinfobot
2. 点击 Start
3. 保存返回的 ID（格式：`123456789`）

### 3. Discord Webhook

1. 在 Discord 服务器中，右键频道 → 编辑频道 → 集成 → Webhooks
2. 新建 Webhook，复制 URL

---

## 监控指标说明

| 指标 | 含义 | 建议值 |
|------|------|--------|
| 套利空间 | (1 - YES - NO) × 100% | > 0.5% |
| ROI | 投资回报率 | > 1% |
| 流动性 | 市场深度 | > $5,000 |
| 检查间隔 | 扫描频率 | 30-60秒 |

---

## 常见问题

### Q: 为什么收不到通知？
A: 检查以下几点：
1. `.env` 文件是否正确配置
2. Telegram Bot 是否已添加到聊天
3. 网络连接是否正常
4. 查看 `arbitrage.log` 日志

### Q: 如何降低误报？
A: 调整以下参数：
```bash
MIN_ARBITRAGE_PERCENT=2.0      # 提高最小套利要求
MIN_LIQUIDITY=10000            # 提高流动性要求
MARKET_TAGS=sports             # 只监控特定类别
```

### Q: 数据库文件在哪里？
A: 默认在当前目录生成 `arbitrage.db`，可用 SQLite 工具查看：
```bash
sqlite3 arbitrage.db "SELECT * FROM opportunities ORDER BY timestamp DESC LIMIT 10;"
```

---

## 下一步

脚本已就绪，接下来可以研究 **Polymarket + 币安合约对冲策略**。
