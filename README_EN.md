# Claude Relay RS

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![AUR](https://img.shields.io/aur/version/claude-code-relay)](https://aur.archlinux.org/packages/claude-code-relay)
[![Homebrew](https://img.shields.io/badge/Homebrew-tap-blue)](https://github.com/wakaka6/homebrew-tap)
[![Docker](https://img.shields.io/docker/v/wakaka6/claude-code-relay?label=Docker)](https://hub.docker.com/r/wakaka6/claude-code-relay)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/wakaka6/claude-code-relay/pulls)

**English | [简体中文](./README.md)**

A high-performance AI API relay service built with Rust. Supports multi-platform account management and intelligent scheduling for Claude, Gemini, and OpenAI Responses (Codex).

## ✨ Features

### Multi-Platform Support

| Platform             | Authentication  | Description                                         |
| -------------------- | --------------- | --------------------------------------------------- |
| **Claude**           | OAuth / API Key | Supports Claude Code CLI OAuth and standard API Key |
| **Gemini**           | Google OAuth    | Supports Google OAuth authentication                |
| **OpenAI Responses** | API Key         | Supports OpenAI Responses API (Codex CLI)           |

### Core Features

- 🔄 **Smart Account Scheduling** - Priority-based automatic account switching
- 🔗 **Sticky Sessions** - Same session bound to same account for context continuity
- 🔑 **Auto Token Refresh** - OAuth token auto-renewal with 10-second advance refresh
- 🌐 **Proxy Support** - Independent SOCKS5/HTTP proxy per account
- 🔧 **Custom API URL** - Configurable API endpoints (mirrors/proxies)
- 📡 **Streaming Response** - Full SSE streaming support
- ⚡ **Error Failover** - Intelligent error detection and automatic account switching

## 🚀 Quick Start

### 1. Deploy the Service

Choose one of the following methods:

**Docker (Recommended):**

```bash
mkdir cc-relay && cd cc-relay
curl -O https://raw.githubusercontent.com/wakaka6/claude-code-relay/main/config.example.toml
curl -O https://raw.githubusercontent.com/wakaka6/claude-code-relay/main/docker-compose.yml
mv config.example.toml config.toml
```

**Arch Linux:**

```bash
yay -S claude-code-relay
```

**macOS:**

```bash
brew tap wakaka6/tap
brew install claude-code-relay
```

### 2. Configure Accounts

Edit the config file to add your account information:

```bash
# Docker
vim config.toml

# AUR
sudo vim /etc/cc-relay-server/config.toml

# Homebrew
vim $(brew --prefix)/etc/cc-relay-server/config.toml
```

Minimal configuration example (Claude API Key):

```toml
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

### 3. Start the Service

```bash
# Docker
docker compose up -d

# AUR
sudo systemctl enable --now cc-relay-server

# Homebrew
brew services start claude-code-relay
```

### 4. Configure Your Client

```bash
export ANTHROPIC_BASE_URL=http://localhost:3000
export ANTHROPIC_API_KEY=any-key  # Can be any value if api_keys auth is not configured
claude
```

## 📥 Installation

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
# or
paru -S claude-code-relay
```

### macOS (Homebrew)

```bash
brew tap wakaka6/tap
brew install claude-code-relay
```

### Binary Download

Download the binary for your platform from [Releases](https://github.com/wakaka6/claude-code-relay/releases).

## ⚙️ Configuration

### Server Configuration

```toml
[server]
host = "127.0.0.1"
port = 3000
database_path = "data/relay.db"
log_level = "info"  # trace, debug, info, warn, error
```

### API Key Authentication

```toml
api_keys = [
    "your-api-key-1",
    "your-api-key-2",
]
```

Leave empty `api_keys = []` to disable authentication. Any key will work, and usage will be tracked as `anonymous`.

### Session Configuration

```toml
[session]
sticky_ttl_seconds = 3600            # Session TTL (default: 1 hour)
renewal_threshold_seconds = 300       # Renew when less than 5 minutes remaining
unavailable_cooldown_seconds = 3600   # Account unavailable cooldown
```

### Account Configuration

> Only configure the platforms you need.

<details>
<summary><b>Claude OAuth Account</b></summary>

```toml
[[accounts]]
type = "claude-oauth"
id = "claude-1"
name = "Claude OAuth Account"
priority = 100
enabled = true
refresh_token = "your-refresh-token"
api_url = "https://api.anthropic.com"  # Optional
```

</details>

<details>
<summary><b>Claude API Key Account</b></summary>

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
<summary><b>Gemini Account</b></summary>

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
<summary><b>OpenAI Responses Account</b></summary>

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
<summary><b>Proxy Configuration</b></summary>

```toml
[accounts.proxy]
type = "socks5"  # or "http"
host = "127.0.0.1"
port = 1080
username = "user"  # Optional
password = "pass"  # Optional
```

</details>

## 🔌 API Endpoints

| Service               | Endpoint                                              | Description          |
| --------------------- | ----------------------------------------------------- | -------------------- |
| **Claude**            | `POST /api/v1/messages`                               | Claude Messages API  |
|                       | `POST /claude/v1/messages`                            | Alias route          |
| **Gemini**            | `POST /gemini/v1/models/:model:generateContent`       | Standard generation  |
|                       | `POST /gemini/v1/models/:model:streamGenerateContent` | Streaming generation |
| **OpenAI Compatible** | `POST /openai/v1/chat/completions`                    | Convert to Claude    |
| **OpenAI Responses**  | `POST /openai/v1/responses`                           | Responses API        |
| **System**            | `GET /health`                                         | Health check         |

## 📱 Client Configuration

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

## 🛠️ Development

### Build from Source

```bash
git clone https://github.com/wakaka6/claude-code-relay.git
cd claude-code-relay
cargo build --release
```

### Local Run

```bash
cp config.example.toml config.toml
# Edit config.toml
./target/release/cc-relay-server --config config.toml
```

### Test & Lint

```bash
cargo test
cargo clippy
cargo fmt
```

## ❓ FAQ

<details>
<summary><b>Cannot connect to service after starting with Docker Compose</b></summary>

**Symptom:** After starting the service with `docker compose up`, the client cannot connect to `localhost:3000`, showing connection refused or service not found errors.

**Cause:** The `host` in the config file is set to `127.0.0.1`, which means the service only listens on the container's internal localhost, not on externally accessible network interfaces.

**Solution:** Change `host` to `0.0.0.0` in your config file to listen on all network interfaces:

```toml
[server]
host = "0.0.0.0"  # Allow external access
port = 3000
```

> **Note:** `127.0.0.1` only allows local access, suitable for running directly on the host machine. Inside a Docker container, `127.0.0.1` refers to the container itself, making it inaccessible from outside. Setting it to `0.0.0.0` makes the service listen on all network interfaces within the container, which combined with Docker's port mapping allows access from the host machine.

</details>

## 🤝 Contributing

Contributions are welcome! Feel free to submit Issues and Pull Requests.

## 📄 License

[MIT License](LICENSE)
