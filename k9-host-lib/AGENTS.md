# k9-host-lib
> BLE/USB 主机通信库 — 提供 K9Client 和 Transport 抽象，连接 K9-Pad 键盘并交换数据

## 地位

Workspace 内的核心通信 crate，被 `k9-host-app` 桌面应用依赖。对下依赖 `shared-datachannel-proto` 协议层，对上暴露 `K9Client<T: Transport>` 高层 API。可选的 `ai-quota` feature 提供 AI 工具配额查询能力。

## 逻辑

1. **Transport 抽象层**（`src/transport/`）：定义 `Transport` trait（async send/receive/disconnect），提供 BLE（bluest）和 USB CDC（serialport）两种实现，均通过 feature flag 控制。
2. **K9Client**（`src/client.rs`）：泛型 `K9Client<T: Transport>`，封装协议层的 packet 构建/解析，提供 `push_text`、`push_numeric`、`push_progress`、`clear_slot`、`ping`、`get_status`、`get_capabilities` 等高层方法。内部 Mutex 保证请求-响应序列化。
3. **AI Quota**（`src/ai_quota/`，feature-gated）：读取 Claude Code / Codex CLI 的 OAuth 凭证，调用用量 API，返回标准化的 `QuotaInfo`。
4. **lib.rs**：统一 re-export，隐藏内部结构。

## 约束

- 运行在 std 环境（非 no_std），目标为桌面主机（macOS/Linux/Windows）
- BLE 和 USB 通过 Cargo feature 独立启用，默认二者均开启
- `ai-quota` feature 额外依赖 `reqwest`、`serde`、`serde_json`、`dirs`
- 所有 Transport impl 必须满足 `Send + Sync`（跨 async task 使用）
- 凭证文件路径依赖 `dirs::home_dir()`，macOS 额外支持 Keychain

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 源码 | `src/` | 库核心实现 |
| 示例 | `examples/` | 开发者测试/验证工具 |
