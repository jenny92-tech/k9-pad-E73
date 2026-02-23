# k9-host-app/src
> 桌面应用源码目录 — 包含 GPUI 入口和数据提供者模块

## 地位

`k9-host-app` crate 的全部 Rust 源码所在目录。`main.rs` 是 crate 入口，`providers/` 子模块封装所有向键盘推送的数据源。

## 逻辑

- `main.rs`：初始化 `env_logger`，创建 GPUI `Application`，设置 `AppState` 全局状态，启动 tokio 桥接线程，打开 800x600 窗口并渲染 `RootView`（观察 AppState 变化自动刷新）
- `app_state.rs`：定义 `AppState`（GPUI Global）、`ConnectionStatus` 枚举、`AppEvent` 跨运行时事件类型
- `bridge.rs`：tokio-GPUI 桥接层 — `start_tokio_thread()` 创建独立 OS 线程运行 tokio runtime，`tokio_main()` 负责 BLE 连接/设备查询/Provider 启动/数据调度，`bridge_loop()` 在 GPUI 端以 50ms 间隔轮询事件更新 AppState
- `providers/`：定义 `Provider` trait（`name`, `function_bit`, `start`）和 `DisplayUpdate`/`DisplayData` 类型，子模块实现四种具体数据源

## 约束

- GPUI 使用 smol 运行时，Provider/K9Client/BleTransport 使用 tokio，通过独立 OS 线程 + `std::sync::mpsc` 桥接
- 文件不应超过 500 行（项目规范）

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 应用入口 | `main.rs` | GPUI 窗口创建、AppState 注册、tokio 桥接启动、RootView 渲染 |
| 应用状态 | `app_state.rs` | AppState (Global)、ConnectionStatus、AppEvent 定义 |
| 运行时桥接 | `bridge.rs` | tokio OS 线程启动、BLE 连接、Provider 调度、GPUI 事件桥接 |
| 数据提供者 | `providers/` | Provider trait + 四种数据源（时间、音量、B站、AI 配额） |
