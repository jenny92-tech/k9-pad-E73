# k9-host-app/src
> 桌面应用源码目录 — 包含 GPUI 入口和数据提供者模块

## 地位

`k9-host-app` crate 的全部 Rust 源码所在目录。`main.rs` 是 crate 入口，`providers/` 子模块封装所有向键盘推送的数据源。

## 逻辑

- `main.rs`：初始化 `env_logger`，创建 GPUI `Application`，打开 800x600 窗口并渲染 `RootView`
- `providers/`：定义 `Provider` trait（`name`, `function_bit`, `start`）和 `DisplayUpdate`/`DisplayData` 类型，子模块实现四种具体数据源

## 约束

- 当前 `RootView` 仅显示静态占位文字，BLE 连接逻辑尚未集成到 UI
- `providers` 模块使用 `tokio` 异步运行时，与 GPUI 主线程的集成方式待确定
- 文件不应超过 500 行（项目规范）

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 应用入口 | `main.rs` | GPUI 窗口创建、env_logger 初始化、RootView 渲染 |
| 数据提供者 | `providers/` | Provider trait + 四种数据源（时间、音量、B站、AI 配额） |
