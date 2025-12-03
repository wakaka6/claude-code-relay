# Claude Relay RS

高性能 AI API 中转服务，使用 Rust 实现。支持 Claude、Gemini 多平台账户管理与智能调度。

## 功能特性

### 多平台支持

- **Claude OAuth** - 支持 Claude Code CLI 的 OAuth 认证
- **Claude API Key** - 支持标准 Anthropic API Key
- **Gemini** - 支持 Google OAuth 认证

### 核心功能

- **智能账户调度** - 基于优先级的多账户自动切换
- **粘性会话** - 同一会话绑定同一账户，确保上下文连续性
- **自动 Token 刷新** - OAuth Token 自动续期，10秒提前刷新策略
- **代理支持** - 每个账户支持独立的 SOCKS5/HTTP 代理配置
- **自定义 API URL** - 支持配置自定义 API 端点（镜像站/代理）
- **流式响应** - 完整的 SSE 流式传输支持
- **错误故障转移** - 智能错误检测与账户自动切换

### 错误处理

支持的错误类型自动检测与处理：

| 错误码 | 类型 | 处理方式 |
|--------|------|----------|
| 401 | 认证失败 | 标记账户不可用 |
| 402 | 余额不足 | 切换到其他账户 |
| 403 | 组织禁用 | 标记账户不可用 |
| 429 | 速率限制 | 等待后重试 |
| 429 | Opus 周限制 | 切换到其他账户 |
| 529 | API 过载 | 暂时排除账户 |

## 项目结构

```
claude-relay-rs/
├── crates/
│   ├── relay-core/      # 核心类型、Trait 定义
│   ├── relay-claude/    # Claude 账户与转发实现
│   ├── relay-gemini/    # Gemini 账户与转发实现
│   ├── relay-openai/    # OpenAI 格式转换器
│   └── relay-server/    # HTTP 服务器与路由
├── config.example.toml  # 配置文件示例
└── migrations/          # 数据库迁移文件
```

## 快速开始

### 编译

```bash
cargo build --release
```

### 配置

1. 复制配置文件：

```bash
cp config.example.toml config.toml
```

2. 编辑 `config.toml`，配置账户信息

### 运行

```bash
./target/release/relay-server --config config.toml
```

## 配置说明

### 服务器配置

```toml
[server]
host = "127.0.0.1"      # 监听地址
port = 3000             # 监听端口
database_path = "data/relay.db"  # SQLite 数据库路径
log_level = "info"      # 日志级别: trace, debug, info, warn, error
```

### API Key 认证

```toml
# 留空则禁用认证
api_keys = [
    "your-api-key-1",
    "your-api-key-2",
]
```

### 粘性会话配置

```toml
[session]
sticky_ttl_seconds = 3600           # 会话 TTL（默认1小时）
renewal_threshold_seconds = 300      # 续期阈值（剩余5分钟时续期）
```

### 账户配置

> **注意**: 你不需要配置所有类型的账户。只需配置你需要使用的平台即可。
> - 只配置 Claude 账户：可以使用 Claude API 和 OpenAI 兼容端点
> - 只配置 Gemini 账户：可以使用 Gemini API 端点
> - 同时配置：可以使用所有端点
>
> 未配置账户的端点在被调用时会返回"无可用账户"错误。

#### Claude OAuth 账户

```toml
[[accounts]]
type = "claude-oauth"
id = "claude-1"
name = "Claude OAuth Account"
priority = 100                    # 优先级，数值越大优先级越高
enabled = true
refresh_token = "your-refresh-token"
api_url = "https://api.anthropic.com"  # 可选：自定义 API URL
```

#### Claude API Key 账户

```toml
[[accounts]]
type = "claude-api"
id = "claude-api-1"
name = "Claude API Account"
priority = 90
enabled = true
api_key = "sk-ant-api03-xxxx"
api_url = "https://api.anthropic.com"  # 可选：自定义 API URL
```

#### Gemini 账户

```toml
[[accounts]]
type = "gemini"
id = "gemini-1"
name = "Gemini Account"
priority = 100
enabled = true
refresh_token = "your-google-refresh-token"
api_url = "https://cloudcode.googleapis.com"  # 可选：自定义 API URL
```

### 代理配置

每个账户支持独立的代理配置：

#### SOCKS5 代理

```toml
[[accounts]]
type = "claude-oauth"
id = "claude-proxy"
name = "Claude with SOCKS5 Proxy"
priority = 50
enabled = true
refresh_token = "your-refresh-token"

[accounts.proxy]
type = "socks5"
host = "127.0.0.1"
port = 1080
username = "user"    # 可选
password = "pass"    # 可选
```

#### HTTP 代理

```toml
[[accounts]]
type = "gemini"
id = "gemini-proxy"
name = "Gemini with HTTP Proxy"
priority = 50
enabled = true
refresh_token = "your-refresh-token"

[accounts.proxy]
type = "http"
host = "proxy.example.com"
port = 8080
username = "user"    # 可选
password = "pass"    # 可选
```

## API 端点

### Claude API

```
POST /api/v1/messages          # Claude Messages API
POST /claude/v1/messages       # 别名路由
GET  /api/v1/models            # 模型列表
```

### Gemini API

```
POST /gemini/v1/models/:model:generateContent       # 标准生成
POST /gemini/v1/models/:model:streamGenerateContent # 流式生成
GET  /gemini/v1/models                              # 模型列表
```

### OpenAI 兼容

```
POST /openai/v1/chat/completions   # OpenAI 格式转 Claude
GET  /openai/v1/models             # 模型列表
```

### 系统端点

```
GET /health    # 健康检查
GET /metrics   # 系统指标
```

## 客户端配置

### Claude Code CLI

Claude Code 是 Anthropic 官方的命令行工具。

**环境变量配置：**

```bash
# 设置 API 地址指向中转服务
export ANTHROPIC_BASE_URL=http://localhost:3000

# 设置中转服务的 API Key（如果启用了认证）
export ANTHROPIC_API_KEY=your-relay-api-key

# 启动 Claude Code
claude
```

**配置文件方式（~/.claude/settings.json）：**

```json
{
  "apiUrl": "http://localhost:3000",
  "apiKey": "your-relay-api-key"
}
```

**验证连接：**

```bash
# 检查连接状态
curl http://localhost:3000/health

# 测试 API 调用
curl -X POST http://localhost:3000/api/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-relay-api-key" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

### Gemini CLI

Gemini CLI 是 Google 的命令行工具。

**环境变量配置：**

```bash
# 设置 API 地址指向中转服务
export GEMINI_API_BASE=http://localhost:3000/gemini

# 设置中转服务的 API Key
export GEMINI_API_KEY=your-relay-api-key

# 启动 Gemini CLI
gemini
```

**验证连接：**

```bash
# 测试 API 调用
curl -X POST "http://localhost:3000/gemini/v1/models/gemini-2.0-flash:generateContent" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-relay-api-key" \
  -d '{
    "contents": [{"parts": [{"text": "Hello"}]}]
  }'
```

### OpenAI Codex CLI

Codex CLI 使用 OpenAI 兼容格式，中转服务会自动转换为 Claude 请求。

**环境变量配置：**

```bash
# 设置 API 地址指向中转服务的 OpenAI 兼容端点
export OPENAI_API_BASE=http://localhost:3000/openai/v1
export OPENAI_BASE_URL=http://localhost:3000/openai/v1

# 设置中转服务的 API Key
export OPENAI_API_KEY=your-relay-api-key

# 启动 Codex
codex
```

**验证连接：**

```bash
# 测试 OpenAI 兼容 API
curl -X POST http://localhost:3000/openai/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-relay-api-key" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

### Cherry Studio

Cherry Studio 是一个支持多模型的桌面客户端。

**配置步骤：**

1. 打开设置 → 模型服务
2. 添加自定义服务：
   - **服务名称**: Claude Relay
   - **API 地址**: `http://localhost:3000`
   - **API Key**: `your-relay-api-key`
   - **API 格式**: Claude (Anthropic)

3. 或配置为 OpenAI 兼容：
   - **API 地址**: `http://localhost:3000/openai/v1`
   - **API 格式**: OpenAI

### Cursor / VS Code 插件

**Cursor 配置：**

在设置中配置：
- **OpenAI API Base**: `http://localhost:3000/openai/v1`
- **OpenAI API Key**: `your-relay-api-key`

**Continue.dev 插件配置（~/.continue/config.json）：**

```json
{
  "models": [
    {
      "title": "Claude via Relay",
      "provider": "anthropic",
      "model": "claude-sonnet-4-20250514",
      "apiBase": "http://localhost:3000",
      "apiKey": "your-relay-api-key"
    }
  ]
}
```

### 通用 HTTP 客户端

**Python 示例：**

```python
import anthropic

client = anthropic.Anthropic(
    base_url="http://localhost:3000",
    api_key="your-relay-api-key"
)

message = client.messages.create(
    model="claude-sonnet-4-20250514",
    max_tokens=1024,
    messages=[{"role": "user", "content": "Hello, Claude!"}]
)
print(message.content)
```

**Node.js 示例：**

```javascript
import Anthropic from '@anthropic-ai/sdk';

const client = new Anthropic({
  baseURL: 'http://localhost:3000',
  apiKey: 'your-relay-api-key',
});

const message = await client.messages.create({
  model: 'claude-sonnet-4-20250514',
  max_tokens: 1024,
  messages: [{ role: 'user', content: 'Hello, Claude!' }],
});
console.log(message.content);
```

### 模型映射说明

使用 OpenAI 兼容端点时，模型名称直接传递给 Claude API：

| 请求模型 | 实际调用 |
|----------|----------|
| `gpt-4o` | `gpt-4o`（Claude 后端处理） |
| `claude-sonnet-4-20250514` | `claude-sonnet-4-20250514` |
| 任意模型名 | 直接传递 |

## 开发

### 运行测试

```bash
cargo test
```

### 代码检查

```bash
cargo clippy
```

### 格式化

```bash
cargo fmt
```

## License

MIT
