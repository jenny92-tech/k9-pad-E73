# examples
> 开发者测试示例 — 验证主机与键盘的端到端通信和 AI 配额功能

## 地位

`k9-host-lib` 的可执行示例，用于开发阶段手动测试功能。不参与库编译，通过 `cargo run --example` 执行。

## 逻辑

- `test_connection.rs`：CLI 测试工具，支持 BLE/USB 两种模式（clap 参数），连接键盘后依次执行 get_capabilities、ping、get_status、push_text/numeric/progress、clear_slot 全流程测试
- `test_ai_quota.rs`：AI 配额冒烟测试，读取 Claude/Codex 凭证并调用用量 API，打印结果到 stdout

## 约束

- 依赖 dev-dependencies（clap, env_logger, tokio full）
- 需要实际硬件或网络环境才能运行成功
- `test_connection.rs` 默认使用 BLE，`--usb` 切换到 USB 模式
- `test_ai_quota.rs` 需要 `--features ai-quota` 编译

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 连接测试 | `test_connection.rs` | BLE/USB 端到端通信测试（全命令序列） |
| 配额测试 | `test_ai_quota.rs` | AI 配额模块冒烟测试（凭证 + API 调用） |
