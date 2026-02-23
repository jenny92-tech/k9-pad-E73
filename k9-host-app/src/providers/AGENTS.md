# k9-host-app/src/providers
> 数据提供者模块 — 定义 Provider trait 并实现四种键盘显示数据源

## 地位

`k9-host-app` 的核心业务模块。每个 Provider 独立轮询一个数据源，通过 `mpsc` channel 发送标准化的 `DisplayUpdate`，供上层调度推送到 K9-Pad 键盘 OLED 显示屏。

## 逻辑

1. `mod.rs` 定义 `Provider` trait（异步 `start` 方法）、`DisplayUpdate` 和 `DisplayData` 枚举
2. 每个具体 Provider 实现 `Provider` trait：
   - `TimeProvider`：每 60s 读取本地时钟，发送格式化时间字符串
   - `VolumeProvider`：每 2s 读取系统音量（macOS osascript），仅在变化时发送
   - `BilibiliProvider`：按配置间隔轮询 B 站粉丝数 API，发送数值
   - `AiQuotaProvider`：每 5min 并行查询 Claude Code + Codex CLI 配额，发送最高使用率
3. 每个 Provider 声明自己的 `function_bit`，对应 `shared-datachannel-proto` 中的功能位掩码

## 约束

- 所有 Provider 必须实现 `Send`（跨线程安全）
- `VolumeProvider::get_volume()` 仅 macOS 实现，其他平台返回 `None`
- `BilibiliProvider` 依赖外部网络，需处理 API 错误和频率限制
- `AiQuotaProvider` 依赖 `k9-host-lib` 的 `ai-quota` feature 和本地凭据文件
- `DisplayData` 有三种变体：`Text(String)`, `Numeric(i32)`, `Progress(u8)`

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 模块定义 | `mod.rs` | Provider trait、DisplayUpdate/DisplayData 类型定义、子模块声明 |
| 时间提供者 | `time.rs` | 本地时钟轮询，格式化时间字符串推送 |
| 音量提供者 | `volume.rs` | macOS 系统音量监控，变化时推送百分比 |
| B站提供者 | `bilibili.rs` | Bilibili 粉丝数 API 轮询，推送粉丝计数 |
| AI 配额提供者 | `ai_quota.rs` | Claude Code / Codex CLI 订阅配额监控，推送最高使用率 |
