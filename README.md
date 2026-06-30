# HiMQTT

HiMQTT 是基于 Rust 编写的 MQTT 消息服务器，底层使用高性能 broker [rumqttd](https://github.com/bytebeamio/rumqtt)，适用于 Ubuntu / Linux 环境下的 IoT、边缘计算和本地消息中转场景。

---

## 目录

- [功能特性](#功能特性)
- [系统架构](#系统架构)
- [环境要求](#环境要求)
- [快速开始](#快速开始)
- [使用说明](#使用说明)
- [配置说明](#配置说明)
- [测试验证](#测试验证)
- [部署指南](#部署指南)
- [常见问题](#常见问题)
- [项目结构](#项目结构)

---

## 功能特性

| 能力 | 说明 |
|------|------|
| MQTT 3.1.1 | 标准端口 `1883`，兼容大多数 IoT 设备 |
| MQTT 5.0 | 端口 `1884`，支持新版协议特性 |
| WebSocket | 端口 `8083`，供浏览器和前端应用接入 |
| 主题路由 | 支持通配符订阅，TCP 与 WebSocket 客户端互通 |
| 管理控制台 | 端口 `3030`，用于运行时管理 |
| Prometheus | 端口 `9042`，暴露监控指标 |
| TOML 配置 | 配置文件驱动，支持认证、TLS（按需启用） |

---

## 系统架构

```
                    ┌─────────────────────────────┐
  IoT 设备 ──TCP──► │  MQTT v4  :1883             │
  新版客户端 ─TCP──► │  MQTT v5  :1884             ├──► HiMQTT Broker ──► 主题路由 / 消息转发
  浏览器/前端 ─WS──► │  WebSocket:8083             │
                    └─────────────────────────────┘
                              │
                    Console :3030  /  Prometheus :9042
```

客户端无论通过 TCP 还是 WebSocket 连接，只要订阅同一主题，即可收到彼此发布的消息。

---

## 环境要求

- **操作系统**：Ubuntu 20.04+ / 其他主流 Linux 发行版
- **Rust**：1.70+（[安装 rustup](https://rustup.rs/)）
- **可选工具**：
  - `mosquitto-clients`：手工测试 pub/sub
  - `curl`：检查 Console / Prometheus 端点

```bash
# Ubuntu 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 可选：MQTT 测试客户端
sudo apt install mosquitto-clients
```

---

## 快速开始

```bash
# 1. 进入项目目录
cd HiMQTT

# 2. 编译 release 版本
cargo build --release

# 3. 启动服务（默认读取 config/himqtt.toml）
./target/release/himqtt

# 4. 另开终端测试（需安装 mosquitto-clients）
mosquitto_sub -h 127.0.0.1 -p 1883 -t 'sensor/#' -v
mosquitto_pub -h 127.0.0.1 -p 1883 -t 'sensor/temp' -m '25.6'
```

启动成功后，终端会显示 HiMQTT 横幅，并监听以下端口：

| 服务 | 地址 | 用途 |
|------|------|------|
| MQTT 3.1.1 | `0.0.0.0:1883` | 标准 MQTT |
| MQTT 5.0 | `0.0.0.0:1884` | MQTT v5 |
| WebSocket | `0.0.0.0:8083` | WS + MQTT 子协议 |
| Console | `127.0.0.1:3030` | 管理界面 |
| Prometheus | `127.0.0.1:9042` | 监控指标 |

---

## 使用说明

### 命令行参数

```bash
himqtt [OPTIONS] [COMMAND]

Options:
  -c, --config <PATH>   配置文件路径 [default: config/himqtt.toml]
  -v, --verbose...      日志级别：-v info，-vv debug，-vvv trace
  -q, --quiet           不打印启动横幅
  -h, --help            帮助
  -V, --version         版本号

Commands:
  generate-config       输出内置默认配置到 stdout
```

### 常用启动方式

```bash
# 默认配置启动
./target/release/himqtt

# 指定配置文件
./target/release/himqtt -c /etc/himqtt/himqtt.toml

# 开启 info 日志
./target/release/himqtt -v

# 静默模式（适合 systemd）
./target/release/himqtt -q -c /etc/himqtt/himqtt.toml
```

### 导出默认配置

```bash
./target/release/himqtt generate-config > /etc/himqtt/himqtt.toml
```

### 客户端连接示例

**MQTT 3.1.1（TCP）**

```bash
mosquitto_pub -h <服务器IP> -p 1883 -t 'device/001/status' -m 'online'
mosquitto_sub -h <服务器IP> -p 1883 -t 'device/+/status' -v
```

**MQTT 5.0**

```bash
mosquitto_pub -h <服务器IP> -p 1884 -t 'device/001/status' -m 'online' -V 5
mosquitto_sub -h <服务器IP> -p 1884 -t 'device/+/status' -v -V 5
```

**WebSocket**

- 地址：`ws://<服务器IP>:8083/`
- 子协议：`mqtt`
- 适用于浏览器端 MQTT 库（如 MQTT.js）

**带认证连接（启用 auth 后）**

```bash
mosquitto_pub -h <服务器IP> -p 1883 -u admin -P changeme -t 'test' -m 'hello'
```

---

## 配置说明

主配置文件：`config/himqtt.toml`

### 核心参数

```toml
[router]
max_connections = 10000          # 最大连接数
max_payload_size = 262144      # 单条消息最大字节（在 connections 段配置）

[v4.1]
listen = "0.0.0.0:1883"          # MQTT v4 监听地址
```

### 启用用户名/密码认证

编辑 `config/himqtt.toml`，取消注释：

```toml
[v4.1.connections.auth]
admin = "your-strong-password"
device1 = "device-secret"
```

### 限制 Console / Prometheus 仅本机访问

生产环境建议保持：

```toml
[console]
listen = "127.0.0.1:3030"

[prometheus]
listen = "127.0.0.1:9042"
```

如需远程监控，请配合防火墙或反向代理，不要直接暴露到公网。

### TLS 加密（可选）

rumqttd 支持 TLS，可在配置中增加 `[v4.2]` 段并指定证书路径，例如：

```toml
[v4.2]
name = "mqtt-v4-tls"
listen = "0.0.0.0:8883"
    [v4.2.tls]
    capath = "/etc/himqtt/certs/ca.pem"
    certpath = "/etc/himqtt/certs/server.pem"
    keypath = "/etc/himqtt/certs/server.key"
    [v4.2.connections]
    connection_timeout_ms = 60000
    max_payload_size = 262144
    max_inflight_count = 100
```

---

## 测试验证

项目内置自动化冒烟测试，覆盖 MQTT v4、v5、WebSocket 及跨协议路由：

```bash
./scripts/run_dev_test.sh
```

测试内容包括：

- 5 个端口监听检查
- Console / Prometheus HTTP 探测
- MQTT CONNECT / PUBLISH / SUBSCRIBE / PING
- TCP 发布 → WebSocket 订阅的跨协议路由

全部通过时输出 `Overall: ALL PASSED`。

---

## 部署指南

### 方式一：手动部署

```bash
# 编译
cargo build --release

# 安装二进制和配置
sudo install -m 755 target/release/himqtt /usr/local/bin/himqtt
sudo mkdir -p /etc/himqtt
sudo cp config/himqtt.toml /etc/himqtt/himqtt.toml

# 前台试运行
himqtt -c /etc/himqtt/himqtt.toml -v
```

### 方式二：systemd 服务（推荐）

项目提供 systemd 单元文件 `deploy/himqtt.service`：

```bash
# 编译并安装
cargo build --release
sudo install -m 755 target/release/himqtt /usr/local/bin/himqtt
sudo mkdir -p /etc/himqtt
sudo cp config/himqtt.toml /etc/himqtt/himqtt.toml

# 安装并启用服务
sudo useradd --system --no-create-home --shell /usr/sbin/nologin himqtt 2>/dev/null || true
sudo mkdir -p /var/lib/himqtt
sudo chown himqtt:himqtt /var/lib/himqtt /etc/himqtt
sudo cp deploy/himqtt.service /etc/systemd/system/himqtt.service
sudo systemctl daemon-reload
sudo systemctl enable himqtt
sudo systemctl start himqtt

# 查看状态
sudo systemctl status himqtt
sudo journalctl -u himqtt -f
```

### 防火墙配置

```bash
# 仅开放 MQTT 和 WebSocket（按实际需求调整）
sudo ufw allow 1883/tcp comment 'HiMQTT v4'
sudo ufw allow 1884/tcp comment 'HiMQTT v5'
sudo ufw allow 8083/tcp comment 'HiMQTT WebSocket'
sudo ufw reload
```

Console（3030）和 Prometheus（9042）默认绑定 `127.0.0.1`，无需对外开放。

### 生产环境建议

1. **修改默认配置**：启用认证，设置强密码
2. **使用 release 构建**：`cargo build --release`
3. **systemd 托管**：开机自启、异常重启
4. **日志**：通过 `journalctl -u himqtt` 查看；需要时可调整 `-v` 级别
5. **监控**：从本机抓取 `http://127.0.0.1:9042/metrics` 接入 Prometheus
6. **资源限制**：可在 systemd unit 中设置 `LimitNOFILE`、`MemoryMax` 等
7. **TLS**：公网暴露时务必启用 TLS（端口 8883 等）

### 升级步骤

```bash
cd HiMQTT
git pull
cargo build --release
sudo install -m 755 target/release/himqtt /usr/local/bin/himqtt
sudo systemctl restart himqtt
```

---

## 常见问题

**Q: 启动后客户端连不上？**

- 检查端口是否监听：`ss -tlnp | grep -E '1883|1884|8083'`
- 检查防火墙是否放行
- 确认配置文件路径正确

**Q: WebSocket 连接失败？**

- 客户端需指定子协议 `mqtt`
- 连接地址示例：`ws://host:8083/`

**Q: 配置文件找不到？**

- 默认路径为相对路径 `config/himqtt.toml`，需在工作目录下启动，或使用 `-c` 指定绝对路径
- 配置文件缺失时，程序会使用内置默认配置并给出提示

**Q: 如何查看日志？**

```bash
# 直接运行时
./target/release/himqtt -vv

# systemd 托管时
sudo journalctl -u himqtt -f
```

---

## 项目结构

```
HiMQTT/
├── Cargo.toml              # Rust 项目配置
├── config/
│   └── himqtt.toml         # 默认服务配置
├── deploy/
│   └── himqtt.service      # systemd 单元文件
├── scripts/
│   ├── run_dev_test.sh     # 一键开发测试
│   └── mqtt_smoke_test.py  # MQTT 协议冒烟测试
├── src/
│   └── main.rs             # 程序入口
└── README.md               # 本文档
```

---

## 许可证

本项目基于 rumqttd（Apache-2.0）构建。使用前请阅读上游项目许可条款。
