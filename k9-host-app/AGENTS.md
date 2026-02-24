# k9-host-app
> GPUI 桌面管理应用 — 连接 K9-Pad 键盘并推送实时数据到键盘 OLED 显示屏

## 地位

Monorepo 中的桌面端入口应用。依赖 `k9-host-lib`（BLE/USB 通信）和 `shared-datachannel-proto`（协议编解码），通过 GPUI 框架提供原生 macOS 窗口界面。替代原 `k9-host-cli` 命令行工具。

## 逻辑

1. `main.rs` 初始化 GPUI 应用窗口，注册 `AppState` 全局状态，启动 tokio 桥接线程和测试桥接线程，根据 `Page` 状态路由 Home/Test 页面
2. `bridge.rs` 在独立 OS 线程创建 `tokio::runtime::current_thread`，执行 BLE 连接、设备查询、Provider 启动
3. `providers/` 模块定义统一的 `Provider` trait 和多个具体实现，每个 Provider 独立轮询数据源
4. Provider 通过 `tokio::sync::mpsc` 发送 `DisplayUpdate`，bridge 调度器转发到 `K9Client` 推送至键盘
5. tokio 线程通过 `std::sync::mpsc` 向 GPUI 端发送 `AppEvent`，`bridge_loop` 以 50ms 轮询更新 `AppState`
6. `RootView` 观察 `AppState` 变化，自动刷新 UI 显示连接状态和设备信息
7. 测试控制台（`test_bridge.rs` + `test_view.rs`）提供独立的手动 BLE/USB 连接和消息发送功能，通过 `TestCommand`/`TestEvent` 与独立 tokio 线程通信

## 约束

- GPUI 0.2 仅支持 macOS（需要 Metal 工具链）
- 音量监控使用 `osascript`，仅 macOS 可用
- Bilibili API 有频率限制，需合理设置轮询间隔
- AI 配额依赖本地凭据文件（Claude Code / Codex CLI）
- 共享协议 crate 以 `std` feature 引入

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 应用入口 | `src/main.rs` | GPUI 窗口创建、AppState 注册、tokio 桥接启动、页面路由、RootView 渲染 |
| 应用状态 | `src/app_state.rs` | AppState (Global)、ConnectionStatus、Page、AppEvent 定义 |
| 运行时桥接 | `src/bridge.rs` | tokio OS 线程启动、BLE 连接重试、Provider 调度、GPUI 事件桥接 |
| 测试控制台 | `src/test_*.rs` | 测试页面状态、tokio 桥接、GPUI UI（手动 BLE/USB 连接 + 命令发送 + 日志） |
| 数据提供者 | `src/providers/` | Provider trait 定义 + 四个具体数据源实现 |
| 构建配置 | `Cargo.toml` | 依赖声明（gpui, tokio, reqwest, chrono 等） |
