# k9-host-app
> GPUI 桌面管理应用 — 连接 K9-Pad 键盘并推送实时数据到键盘 OLED 显示屏

## 地位

Monorepo 中的桌面端入口应用。依赖 `k9-host-lib`（BLE/USB 通信）和 `shared-datachannel-proto`（协议编解码），通过 GPUI 框架提供原生 macOS 窗口界面。替代原 `k9-host-cli` 命令行工具。

## 逻辑

1. `main.rs` 初始化 GPUI 应用窗口，渲染主界面
2. `providers/` 模块定义统一的 `Provider` trait 和多个具体实现
3. 每个 Provider 独立轮询数据源（本地时钟、系统音量、Bilibili API、AI 配额 API）
4. Provider 通过 `mpsc` channel 发送 `DisplayUpdate`，由上层调度推送到键盘

## 约束

- GPUI 0.2 仅支持 macOS（需要 Metal 工具链）
- 音量监控使用 `osascript`，仅 macOS 可用
- Bilibili API 有频率限制，需合理设置轮询间隔
- AI 配额依赖本地凭据文件（Claude Code / Codex CLI）
- 共享协议 crate 以 `std` feature 引入

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 应用入口 | `src/main.rs` | GPUI 窗口初始化与主界面渲染 |
| 数据提供者 | `src/providers/` | Provider trait 定义 + 四个具体数据源实现 |
| 构建配置 | `Cargo.toml` | 依赖声明（gpui, tokio, reqwest, chrono 等） |
