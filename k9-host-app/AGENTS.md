# k9-host-app
> GPUI 桌面管理应用 — 连接 K9-Pad 键盘并推送实时数据到键盘 OLED 显示屏；兼作 AI CLI 审批面板（hook 事件汇入 + 刘海审批卡）

## 地位

Monorepo 中的桌面端入口应用。依赖 `k9-host-lib`（BLE/USB 通信）和 `shared-datachannel-proto`（协议编解码），通过 GPUI 框架提供原生 macOS 窗口界面。替代原 `k9-host-cli` 命令行工具。

## 逻辑

1. `main.rs` 初始化 GPUI 应用窗口，注册 `AppState` 全局状态，启动 tokio 桥接线程和测试桥接线程，根据 `Page` 状态路由 Home/Test 页面；Home 页提供「Send Test Dialog」按钮（协议 v2+ 且已连接时可用）和最近一次弹窗结果展示
2. `bridge.rs` 在独立 OS 线程创建 `tokio::runtime::current_thread`，执行 BLE 连接、设备查询、Provider 启动；连接后订阅 `K9Client` 设备事件转发为 `AppEvent`（DialogResult 先查 dialog→req 映射，命中则按 Approval 或单题单选完成 oneshot + 投 `PermissionAnswered`）；GPUI→tokio 方向通过 `std::sync::mpsc` 传递 `HostCommand`（ShowDialog / AnswerPermission / AnswerQuestions / ResolvePermissions），dispatcher 同时轮询 provider 更新与 dialog 队列；**看门狗**：60s interval 投 `AppEvent::WatchdogSweep` → GPUI 侧 `sweep_sessions` → 受影响 pending 回流 deny
3. `hook_server.rs` 在同一 tokio runtime spawn Unix socket server：阻塞 PermissionRequest 注册 oneshot；普通工具权限解析为 Approval，`AskUserQuestion.tool_input.questions` 解析为保留描述的多题单选/多选模型（重复 question key 加 `_2` 后缀），完成后保留原始 tool_input 并写入 `updatedInput.answers`。设备只承接普通审批和单题单选，多题/多选明确留给桌面；其余 Codex reviewer 让路、SessionEnd/活动/完成 drain、重复 tool_use_id 去重规则保持不变
4. `session.rs` 是纯 reducer（Idle / Processing / Running(tool) / WaitingApproval / WaitingQuestion）；AskUserQuestion 进入 WaitingQuestion，普通 PermissionRequest 进入 WaitingApproval，waiting 粘性、看门狗、队列 drain 和 Kimi prompt 解析保持原规则
5. `notch_panel.rs` 刘海变形面板：Idle 会话也显示；Processing 显示 working，Running 显示当前 tool；普通权限卡显示 Allow/Deny；Questions 卡逐题显示 options，单选立即推进，多选勾选后 Submit，Cancel 走 deny。公共 InteractionController/PointerTracker 继续负责 hover、点击固定和外部点击收回，审批/问题仍是最高优先级强制展开
6. `notch_native.rs` 是 K9 到公共 `notchkit-macos` 的薄适配层：K9 只创建一个刘海窗口并显式选择 `PhysicalNotchOrWindow`（优先物理刘海屏，否则窗口所在屏）；不会默认在每块外接屏复制可操作审批。公共库另提供 Window/Main 策略；每屏显示需应用显式创建多个窗口实例
7. `notch_shape.rs` 轮廓纯函数：Notch 风格紧凑态按真实 MacBook 刘海 6/14 半径起步，下拉时连续过渡到 12/22，保持 k=0.62 连续曲率；Capsule 风格端部全圆；Widened 默认宽度按 `max(notch+96, 280)` 响应式派生，`widen_factor` 仅作基线微调
8. `providers/` 模块定义统一的 `Provider` trait 和多个具体实现，每个 Provider 独立轮询数据源
9. Provider 通过 `tokio::sync::mpsc` 发送 `DisplayUpdate`，bridge 调度器转发到 `K9Client` 推送至键盘
10. tokio 线程通过 `std::sync::mpsc` 向 GPUI 端发送 `AppEvent`，`bridge_loop` 以 50ms 轮询更新 `AppState`（含 dialog_supported / last_dialog_result）
11. `RootView` 观察 `AppState` 变化，自动刷新 UI 显示连接状态和设备信息
12. 测试控制台的刘海区提供 Idle / Working / Tool:Bash / Allow-Deny / Options / End 六个真实 hook 注入场景；Options 示例包含一道单选和一道多选

## 约束

- GPUI 0.2 仅支持 macOS（需要 Metal 工具链）
- 音量监控使用 `osascript`，仅 macOS 可用
- Bilibili API 有频率限制，需合理设置轮询间隔
- AI 配额依赖本地凭据文件（Claude Code / Codex CLI）
- 共享协议 crate 以 `std` feature 引入

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 应用入口 | `src/main.rs` | GPUI 窗口创建、AppState 注册、tokio 桥接启动、页面路由、RootView 渲染（含弹窗测试按钮与结果展示） |
| 应用状态 | `src/app_state.rs` | AppState、五态 session、Approval/Questions pending prompt、多题单选/多选推进与 typed answers、审批队列 drain |
| 运行时桥接 | `src/bridge.rs` | tokio/GPUI 桥接、AnswerPermission/AnswerQuestions、设备 DialogResult 路由、Provider 与看门狗 |
| Hook socket 服务 | `src/hook_server.rs` | hook 接入、AskUserQuestion 解析、decision/updatedInput.answers 构造、设备能力降级边界、pending 生命周期 |
| 会话状态机 | `src/session.rs` | 5 态 AgentStatus + SessionState + 纯 reducer（照蓝本迁移表，waiting 粘性，含单测）、看门狗 sweep_sessions（waiting 300s / idle 600s）、活动/完成事件谓词、HookEvent 含 tool_use_id、Kimi content-part prompt 拼接 |
| 刘海变形面板 | `src/notch_panel.rs` | Idle/Working/Tool 展示 + Approval/Questions 内容注入 + NotchKit hover/click 仲裁 + W/D 动画 |
| 面板原生适配 | `src/notch_native.rs` | GPUI→NotchKit 薄适配；单实例 `PhysicalNotchOrWindow` 屏幕策略 |
| 面板形状 | `src/notch_shape.rs` | 连续轮廓纯函数（Notch 紧凑 6/14 → 展开 12/22；Capsule=全圆端；k=0.62）+ 单测 |
| 面板配置 | `src/notch_config.rs` | `~/.k9pad/config.json` 读写（notch_style / 响应式宽度微调 / 紧凑态半径），启动加载、调试面板实时修改并持久化 |
| 弹簧 | `src/spring.rs` | 阻尼弹簧半隐式欧拉积分（pop 0.3/0.78、open 0.42/0.82、close 0.38/1.0）+ 单测 |
| 测试控制台 | `src/test_*.rs` | 测试页面状态、tokio 桥接、GPUI UI（手动 BLE/USB 连接 + 命令发送 + 日志） |
| 数据提供者 | `src/providers/` | Provider trait 定义 + 四个具体数据源实现 |
| 构建配置 | `Cargo.toml` | 依赖声明（gpui, tokio, reqwest, chrono, serde_json 等） |
