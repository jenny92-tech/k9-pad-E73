# k9-host-lib/src
> 库核心实现 — K9Client、Transport 抽象、AI 配额查询

## 地位

`k9-host-lib` crate 的源码根目录，`lib.rs` 为入口点，所有公共 API 从此导出。

## 逻辑

- `lib.rs`：crate 入口，声明并 re-export `client`、`transport`、`ai_quota` 模块
- `client.rs`：`K9Client<T>` 高层客户端，构建协议包、解析响应、序列化并发请求
- `transport/`：Transport trait 定义 + BLE/USB 实现
- `ai_quota/`：AI 工具配额查询（feature-gated）

## 约束

- `ai_quota` 模块仅在 `ai-quota` feature 启用时编译
- `transport::ble` 仅在 `ble` feature 启用时编译
- `transport::usb` 仅在 `usb` feature 启用时编译
- `K9Client` 对 `Transport` 泛型，不依赖具体传输实现

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 入口 | `lib.rs` | Crate 根，模块声明 + 公共 API re-export |
| 客户端 | `client.rs` | K9Client 高层 API（push/get/ping），含单元测试 |
| 传输层 | `transport/` | Transport trait + BLE/USB 实现 |
| AI 配额 | `ai_quota/` | Claude/Codex 凭证读取 + 用量 API 查询 |
