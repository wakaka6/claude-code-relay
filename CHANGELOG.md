# Changelog

本项目的所有重要更改都将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

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
