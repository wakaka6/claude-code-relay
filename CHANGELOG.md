# Changelog

本项目的所有重要更改都将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

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
