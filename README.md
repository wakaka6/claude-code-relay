# Claude Relay RS

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![AUR](https://img.shields.io/aur/version/claude-code-relay)](https://aur.archlinux.org/packages/claude-code-relay)
[![Homebrew](https://img.shields.io/badge/Homebrew-tap-blue)](https://github.com/wakaka6/homebrew-tap)
[![Docker](https://img.shields.io/docker/v/wakaka6/claude-code-relay?label=Docker)](https://hub.docker.com/r/wakaka6/claude-code-relay)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/wakaka6/claude-code-relay/pulls)

**[English](./README_EN.md) | 简体中文**

高性能 AI API 中转服务，使用 Rust 实现。支持 Claude、Gemini、OpenAI Responses (Codex) 多平台账户管理与智能调度。

## ✨ 功能特性

### 多平台支持

| 平台                 | 认证方式        | 说明                                             |
| -------------------- | --------------- | ------------------------------------------------ |
| **Claude**           | OAuth / API Key | 支持 Claude Code CLI 的 OAuth 认证和标准 API Key |
| **Gemini**           | Google OAuth    | 支持 Google OAuth 认证                           |
| **OpenAI Responses** | API Key         | 支持 OpenAI Responses API (Codex CLI)            |

### 核心功能

- 🔄 **智能账户调度** - 基于优先级的多账户自动切换
- 🔗 **粘性会话** - 同一会话绑定同一账户，确保上下文连续性
- 🔑 **自动 Token 刷新** - OAuth Token 自动续期，10秒提前刷新策略
- 🌐 **代理支持** - 每个账户支持独立的 SOCKS5/HTTP 代理配置
- 🔧 **自定义 API URL** - 支持配置自定义 API 端点（镜像站/代理）
- 📡 **流式响应** - 完整的 SSE 流式传输支持
- ⚡ **错误故障转移** - 智能错误检测与账户自动切换

## 🚀 快速开始

### 1. 部署服务

选择以下任一方式：

**Docker（推荐）：**

```bash
mkdir cc-relay && cd cc-relay
curl -O https://raw.githubusercontent.com/wakaka6/claude-code-relay/main/config.example.toml
curl -O https://raw.githubusercontent.com/wakaka6/claude-code-relay/main/docker-compose.yml
mv config.example.toml config.toml
```

**Arch Linux：**

```bash
yay -S claude-code-relay
```

**macOS：**

```bash
brew tap wakaka6/tap
brew install claude-code-relay
```

### 2. 配置账户

编辑配置文件，添加你的账户信息：

```bash
# Docker
vim config.toml

# AUR
sudo vim /etc/cc-relay-server/config.toml

# Homebrew
vim $(brew --prefix)/etc/cc-relay-server/config.toml
```

最简配置示例（Claude API Key）：

```toml
# api_keys 必须在 [server] 之前
api_keys = ["your-relay-key"]

[server]
host = "127.0.0.1"
port = 3000

[[accounts]]
type = "claude-api"
id = "main"
name = "Main Account"
priority = 100
enabled = true
api_key = "sk-ant-api03-xxxx"
```

### 3. 启动服务

```bash
# Docker
docker compose up -d

# AUR
sudo systemctl enable --now cc-relay-server

# Homebrew
brew services start claude-code-relay
```

### 4. 配置客户端

```bash
export ANTHROPIC_BASE_URL=http://localhost:3000
export ANTHROPIC_API_KEY=any-key  # 如果未配置 api_keys 认证，可以是任意值
claude
```

## 📥 安装方式

### Docker

```bash
docker run -d \
  --name cc-relay-server \
  -p 3000:3000 \
  -v ./config.toml:/app/config.toml:ro \
  -v ./data:/app/data \
  wakaka6/claude-code-relay:latest
```

### Arch Linux (AUR)

```bash
yay -S claude-code-relay
# 或
paru -S claude-code-relay
```

### macOS (Homebrew)

```bash
brew tap wakaka6/tap
brew install claude-code-relay
```

### 二进制下载

从 [Releases](https://github.com/wakaka6/claude-code-relay/releases) 下载对应平台的二进制文件。

## ⚙️ 配置说明

### 服务器配置

```toml
[server]
host = "127.0.0.1"
port = 3000
database_path = "data/relay.db"
log_level = "info"  # trace, debug, info, warn, error
```

### API Key 认证

```toml
api_keys = [
    "your-api-key-1",
    "your-api-key-2",
]
```

留空 `api_keys = []` 则禁用认证，任意 key 都可访问，统计时标记为 `anonymous`。

### 会话配置

```toml
[session]
sticky_ttl_seconds = 3600            # 会话 TTL（默认 1 小时）
renewal_threshold_seconds = 300       # 续期阈值（剩余 5 分钟时续期）
unavailable_cooldown_seconds = 3600   # 账户不可用冷却时间
```

### 账户配置

> 只需配置你需要使用的平台即可。

<details>
<summary><b>Claude OAuth 账户</b></summary>

```toml
[[accounts]]
type = "claude-oauth"
id = "claude-1"
name = "Claude OAuth Account"
priority = 100
enabled = true
refresh_token = "your-refresh-token"
api_url = "https://api.anthropic.com"  # 可选
```

</details>

<details>
<summary><b>Claude API Key 账户</b></summary>

```toml
[[accounts]]
type = "claude-api"
id = "claude-api-1"
name = "Claude API Account"
priority = 90
enabled = true
api_key = "sk-ant-api03-xxxx"
```

</details>

<details>
<summary><b>Gemini 账户</b></summary>

```toml
[[accounts]]
type = "gemini"
id = "gemini-1"
name = "Gemini Account"
priority = 100
enabled = true
refresh_token = "your-google-refresh-token"
```

</details>

<details>
<summary><b>OpenAI Responses 账户</b></summary>

```toml
[[accounts]]
type = "openai-responses"
id = "codex-1"
name = "OpenAI Responses Account"
priority = 100
enabled = true
api_key = "sk-your-openai-api-key"
```

</details>

<details>
<summary><b>代理配置</b></summary>

```toml
[accounts.proxy]
type = "socks5"  # 或 "http"
host = "127.0.0.1"
port = 1080
username = "user"  # 可选
password = "pass"  # 可选
```

</details>

## 🔌 API 端点

| 服务                 | 端点                                                  | 说明                |
| -------------------- | ----------------------------------------------------- | ------------------- |
| **Claude**           | `POST /api/v1/messages`                               | Claude Messages API |
|                      | `POST /claude/v1/messages`                            | 别名路由            |
| **Gemini**           | `POST /gemini/v1/models/:model:generateContent`       | 标准生成            |
|                      | `POST /gemini/v1/models/:model:streamGenerateContent` | 流式生成            |
| **OpenAI 兼容**      | `POST /openai/v1/chat/completions`                    | 转换为 Claude       |
| **OpenAI Responses** | `POST /openai/v1/responses`                           | Responses API       |
| **系统**             | `GET /health`                                         | 健康检查            |

## 📱 客户端配置

<details>
<summary><b>Claude Code CLI</b></summary>

```bash
export ANTHROPIC_BASE_URL=http://localhost:3000
export ANTHROPIC_API_KEY=your-relay-api-key
claude
```

</details>

<details>
<summary><b>Gemini CLI</b></summary>

```bash
export GEMINI_API_BASE=http://localhost:3000/gemini
export GEMINI_API_KEY=your-relay-api-key
gemini
```

</details>

<details>
<summary><b>OpenAI Codex CLI</b></summary>

```bash
export OPENAI_BASE_URL=http://localhost:3000/openai/v1
export OPENAI_API_KEY=your-relay-api-key
codex
```

</details>

<details>
<summary><b>Python / Node.js SDK</b></summary>

**Python:**

```python
import anthropic
client = anthropic.Anthropic(base_url="http://localhost:3000", api_key="your-key")
```

**Node.js:**

```javascript
import Anthropic from "@anthropic-ai/sdk";
const client = new Anthropic({
  baseURL: "http://localhost:3000",
  apiKey: "your-key",
});
```

</details>

## 🛠️ 开发

### 从源码构建

```bash
git clone https://github.com/wakaka6/claude-code-relay.git
cd claude-code-relay
cargo build --release
```

### 本地运行

```bash
cp config.example.toml config.toml
# 编辑 config.toml
./target/release/cc-relay-server --config config.toml
```

### 测试与检查

```bash
cargo test
cargo clippy
cargo fmt
```

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

## 📄 License

[MIT License](LICENSE)
