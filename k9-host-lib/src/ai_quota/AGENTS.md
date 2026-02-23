# ai_quota
> AI 工具配额查询 — 读取 Claude Code / Codex CLI 凭证并获取用量百分比

## 地位

`k9-host-lib` 的可选功能模块（`ai-quota` feature），为桌面应用提供 AI 工具使用量信息，最终推送到键盘 OLED 上显示。

## 逻辑

- `mod.rs`：模块入口，re-export credentials/error/quota 的公共 API
- `credentials.rs`：读取 Claude Code OAuth 凭证（文件 `~/.claude/.credentials.json` 或 macOS Keychain）和 Codex CLI 凭证（`~/.codex/auth.json`）
- `error.rs`：`AiQuotaError` 枚举，统一凭证读取和 HTTP 请求的错误类型
- `quota.rs`：`fetch_claude_quota()` 调用 Anthropic usage API，`fetch_codex_quota()` 调用 ChatGPT usage API，返回 `QuotaInfo`（工具名 + 利用率百分比）

## 约束

- 整个模块仅在 `ai-quota` feature 启用时编译
- 依赖 `reqwest`（HTTP 异步客户端）、`serde`/`serde_json`（JSON 解析）、`dirs`（home 目录定位）
- Claude 凭证来源：先尝试文件，macOS 上 fallback 到 Keychain（调用 `/usr/bin/security` CLI）
- Codex usage API 端点可能变化，获取失败视为非致命错误
- `QuotaInfo::as_progress()` 返回 0-100 u8 值，可直接推送到 OLED 进度条

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 入口 | `mod.rs` | 模块声明 + 公共 API re-export |
| 凭证 | `credentials.rs` | 读取 Claude/Codex OAuth 凭证（文件 + macOS Keychain） |
| 错误 | `error.rs` | AiQuotaError 统一错误枚举 |
| 配额 | `quota.rs` | fetch_claude_quota / fetch_codex_quota + QuotaInfo 数据结构 |
