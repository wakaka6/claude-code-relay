# Changelog

本项目的所有重要更改都将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.2.2] - 2025-12-06

### Added

- Token 使用量持久化功能
  - 按客户端 API Key（SHA256 hash）+ 账户 + 模型维度统计
  - 支持 Claude 和 OpenAI 兼容路由的 token 统计
  - 流式响应实时提取 usage 信息
- 使用 sqlx migrate 管理数据库迁移
- 新增单元测试覆盖 `ClientApiKeyHash`、`record_usage_if_valid` 等模块

### Fixed

- 修复 `api_keys` 配置在 `[server]` section 之后被忽略的问题
  - 根本原因：TOML 解析规则将 section 后的键值对归属于该 section
  - 解决方案：将 `api_keys` 移至配置文件顶部（`[server]` 之前）
- 更新 `config.example.toml` 和文档，确保配置格式正确

### Changed

- 服务启动时输出 API keys 配置状态日志

## [0.2.1] - 2025-12-05

### Added

- Docker 支持
  - 多阶段构建 Dockerfile，基于 Alpine，支持 amd64/arm64
  - docker-compose.yml 配置文件
  - 内置健康检查（HEALTHCHECK）
  - OCI 镜像标签
- 包管理器发布
  - AUR 包：`yay -S claude-code-relay`
  - Homebrew tap：`brew install wakaka6/tap/claude-code-relay`
- GitHub Actions 自动构建并推送 Docker 镜像到 Docker Hub 和 GHCR
- LICENSE 文件（MIT）

### Changed

- 重构 README 文档结构
  - 新增"快速开始"4 步上手指南（用户导向）
  - "开发"部分独立（开发者导向）
  - 删除冗余的"部署"章节，与安装部分合并
- AUR 包配置优化
  - 配置文件使用绝对路径 `/var/lib/cc-relay/relay.db`
  - 配置文件权限 640，属组 cc-relay
  - 添加 sysusers/tmpfiles 配置

## [0.2.0] - 2025-12-04

### Added

- 新增 `unavailable_cooldown_seconds` 配置选项，可自定义账户失效后的冷却时间（默认 3600 秒）
- 粘性会话持久化到 SQLite 数据库，服务重启后会话不丢失
- 智能续期机制：仅在剩余时间低于阈值时才续期会话
- 预留用量统计功能（`record_usage`、`get_usage_by_account`）

### Changed

- 调度器方法重构为异步（`select_account`、`select_account_excluding`）
- 粘性会话从内存 HashMap 迁移到 SQLite 持久化存储

### Fixed

- 修复 GitHub Actions release workflow 无法正确生成 checksums 的问题
  - 替换废弃的 `actions/create-release` 和 `upload-release-asset` 为 `softprops/action-gh-release`
  - 添加 artifact 上传步骤，确保 checksums 生成正常工作
- 修复 Claude API 响应中未知 content 类型导致 400 错误的问题
  - `MessagesResponse.content` 从 `Vec<ContentBlock>` 改为 `serde_json::Value` 实现完全透传
  - 解决了 `thinking`、`tool_result` 等新类型被序列化为 `{"type": "Unknown"}` 的问题

## [0.1.0] - 2025-12-04

### Added

- 初始项目结构，支持 Claude Code 中继服务
- `relay-core`: 核心抽象层，定义 Relay、AccountProvider 等 trait
- `relay-claude`: Claude API 中继实现，支持 OAuth 认证和流式响应
- `relay-gemini`: Gemini API 中继实现，支持 OAuth token 刷新
- `relay-openai-to-anthropic`: OpenAI API 格式到 Anthropic API 格式的转换器
- `relay-codex`: OpenAI Codex/Responses API 中继支持
- `relay-server`: HTTP 服务器，提供统一的 API 入口
- 多账户调度器，支持负载均衡和账户冷却
- 跨平台自动发布 GitHub Actions workflow
- Systemd 服务文件，支持 Linux 系统部署
- 内容过滤错误处理
