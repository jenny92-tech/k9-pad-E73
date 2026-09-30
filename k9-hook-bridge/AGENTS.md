# k9-hook-bridge > CLI hook 事件桥接器 + 多 CLI hook 安装器

## 地位

AI CLI（Claude Code / Codex / Kimi Code / Pi / OMP）与 k9-host-app 之间的桥接进程：CLI 同步执行 hook 命令，
本进程把 stdin 事件 JSON 富化后经 Unix socket（`/tmp/k9pad-<uid>.sock`）转发给 host app 的 SocketServer；
阻塞事件（权限请求/提问）把 server 决策 JSON 原样写回 stdout 交给 CLI。
Pi/OMP 走 TS 扩展直连 socket，仅危险命令审批/ask 提问才 shell out 调本进程（无 argv，`_source` 从 payload 读）。

## 逻辑

- **分帧协议**（对照蓝本 CodeIsland）：单 JSON 文档 + `shutdown(Write)` 半关闭表结束；server 读到 EOF 处理并回一个 JSON；client 读响应到 EOF。
- **桥接流程**（`main.rs`）：5s 超时读 stdin → 解析 → 富化（`_source` payload 优先/argv 兜底、`_ppid`、`cwd` 兜底、`session_id` 兜底 `<source>-ppid-<pid>`）→ 阻塞性判定 → 连 socket（非阻塞 1s / 阻塞 3s）→ 写 → 半关闭 → 非阻塞直接退出，阻塞读响应到 EOF 写 stdout。
- **安全语义**：`K9PAD_SKIP` 置位直接 exit 0；桥接模式任何错误（socket 不存在/连接失败/解析失败）都**静默 exit 0 且无输出**，CLI 回退自身审批 UI。
- **安装模式**（`install <claude|codex|kimi|pi|omp>`）：统一先复制自身到 `~/.k9pad/k9-hook-bridge`；合并写各 CLI 配置文件（只动 hooks 节；幂等——已有 k9 条目先删再插；写前备份 `<file>.bak`；损坏 JSON/TOML 拒绝覆盖并非零退出）。
  - `install claude [--config-dir PATH]`：Claude settings.json hooks 节（路径解析：--config-dir > $CLAUDE_CONFIG_DIR > ~/.claude）。
  - `install codex`：`$CODEX_HOME`（前导 `~` 展开）或 `~/.codex` 下 hooks.json（nested 无 matcher）+ config.toml `[features] hooks = true`（toml_edit 保留注释；旧名 `codex_hooks` 改写为 `hooks`；内容未变不重写）。
  - `install kimi`：`~/.kimi-code/config.toml`（legacy `~/.kimi/config.toml` 存在且 modern 不存在时用 legacy）追加 `[[hooks]]` 块（toml_edit；标量 hooks 冲突拒绝覆盖）。
  - `install pi` / `install omp`：内嵌 TS 扩展模板（`pi_ts.rs` / `omp_ts.rs`，OMP 由 Pi 模板替换 import 生成）写到 `~/.pi|omp/agent/extensions/k9pad.ts`。

## 约束

- 依赖最小化：tokio 仅 `rt/macros/io-util/net/time`，不开 full；TOML 操作用 toml_edit（保留注释格式）。
- 只动各配置文件的 hooks 节；其他 key、注释与其他工具的 hook entry 原样保留。
- 事件表：Claude — PermissionRequest/Notification 86400 其余 5；Codex — PermissionRequest 86400、SessionEnd 3、其余 5；Kimi — 无 PermissionRequest、Notification 600、Pre/PostToolUse(+Failure) 带 matcher=".*"、其余 5。
- `unsafe { libc::getuid() }` 仅用于拼默认 socket 路径。
- OMP 扩展与蓝本一致复用 `_source: "pi"` 与 `pi-` session 前缀。

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| 入口编排 | `src/main.rs` | clap 解析（桥接 + install 五个子命令）、桥接流程编排、K9PAD_SKIP 直通 |
| 纯逻辑 | `src/bridge.rs` | 事件富化（payload `_source` 优先）、阻塞性判定、session_id 兜底（纯函数可测） |
| 传输层 | `src/socket.rs` | Unix socket 连接/写入/半关闭/读响应，socket 路径解析 |
| 安装器公共层 | `src/install.rs` | Claude settings.json 合并 + 二进制安装/`.bak` 备份公共逻辑 |
| Codex 安装器 | `src/install/codex.rs` | hooks.json nested 合并 + config.toml `[features] hooks` 开关（toml_edit） |
| Kimi 安装器 | `src/install/kimi.rs` | config.toml `[[hooks]]` 块合并（toml_edit）+ modern/legacy 路径解析 |
| Pi/OMP 安装器 | `src/install/pi_ext.rs` | TS 扩展落盘（`~/.pi|omp/agent/extensions/k9pad.ts`） |
| 扩展模板 | `src/install/pi_ts.rs` / `src/install/omp_ts.rs` | Pi 扩展源码字面量；OMP 由 Pi 模板替换 import 派生 |
| 集成测试 | `tests/socket_roundtrip.rs` | mock server round-trip、静默退出语义、K9PAD_SKIP |
