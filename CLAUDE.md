# K9-Pad Monorepo

> nRF52840 BLE 机械键盘固件 + 桌面主机应用，Cargo workspace 组织

---

## 项目结构

```
k9-pad-E73-master/                     (monorepo 根目录)
├── Cargo.toml                          ← workspace: shared + host crates
├── rust-toolchain.toml
├── .gitignore
├── CLAUDE.md                           ← 本文件
│
├── shared-datachannel-proto/           ← 共享协议 crate（no_std）
├── k9-host-lib/                        ← 主机通信库（BLE/USB transport）
├── k9-host-app/                        ← GPUI 桌面应用
│
├── k9-pad-firmware/                    ← 固件（exclude 出 workspace，独立构建）
│   ├── Cargo.toml
│   ├── .cargo/config.toml
│   ├── build.rs, memory.x, keyboard.toml, vial.json
│   ├── Makefile, Makefile.toml
│   ├── src/
│   ├── tests/
│   ├── tools/
│   ├── docs/
│   └── scripts/
│
└── .cargo/config.toml                  ← workspace 级（无 target 设置）
```

**Workspace 模式**：`shared-datachannel-proto`、`k9-host-lib`、`k9-host-app` 在 workspace 内；
`k9-pad-firmware` 被 `exclude`，使用独立 `.cargo/config.toml`（ARM target）。

---

## Fractal Documentation Protocol

本项目采用三层分形文档协议，确保 AI Agent 能快速理解任意模块的上下文。

### 第一层：源码文件三行头部注释

每个源码文件最前面放三行结构化注释：

```
// INPUT:  依赖什么
// OUTPUT: 提供什么
// POS:    在系统中的地位
```

- Rust/C 用 `//`，Python 用 `#`
- 放在文件第一行，在任何 `use` / `#include` / `import` 之前

### 第二层：目录级 CLAUDE.md

格式：`# 模块名 > 一句话定位 ## 地位 ## 逻辑 ## 约束 ## 业务域清单`

### 第三层：级联更新规则

| 触发事件 | 文件级 | 目录级 | 上级目录级 |
|----------|--------|--------|-----------|
| 新增文件 | 添加三行注释 | 更新 CLAUDE.md 清单 | 更新上级清单 |
| 删除文件 | — | 更新 CLAUDE.md 清单 | 更新上级清单 |
| 修改接口/职责 | 更新三行注释 | 更新 CLAUDE.md | 如影响则更新上级 |
| 仅改内部实现 | 检查注释准确性 | 不更新 | 不更新 |

---

## Build & Dev Commands

### 固件（在 `k9-pad-firmware/` 目录下）

```bash
cd k9-pad-firmware

# 构建 release
cargo make build

# 生成 hex/uf2
cargo make objcopy
cargo make uf2

# 运行固件测试
cargo test --lib

# 检查编译
cargo check

# DFU 打包
make dfu

# SWD 烧录
make flash
```

### 主机侧（在根目录）

```bash
# 编译全部 host crates
cargo build

# 运行 host app
cargo run -p k9-host-app

# 测试共享协议
cargo test -p shared-datachannel-proto
```

---

## Git Safety

- **MUST** 禁止使用 `git add .` 或 `git add -A`
- **MUST** 禁止提交敏感信息（API keys, 密码, tokens）
- **MUST** 禁止提交编译产物（*.bin, *.elf, *.hex, *.uf2, target/）

---

## Commit Guidelines

- 提交信息格式：`type(scope): description`
- type 类型：feat, fix, docs, style, refactor, test, chore
- scope 范围：firmware, host, proto, repo

---

## Testing

- **MUST** 推送前运行固件测试：`cd k9-pad-firmware && cargo test --lib`
- **MUST** 推送前运行 host 测试：`cargo test`（根目录）
- 嵌入式相关代码使用 `#[cfg(test)]` 隔离测试

---

## Code Style

- **SHOULD** 文件不超过 500 行
- **MUST** 避免使用 `unsafe` 除非绝对必要，且必须添加 `// SAFETY:` 注释
- **SHOULD** 固件使用 `defmt` 进行日志输出
- **SHOULD** 主机使用 `log` + `env_logger` 进行日志输出

---

## Embedded-Specific Rules（仅 k9-pad-firmware）

- **MUST** 注意 `no_std` 环境限制
- **MUST** 注意内存布局（`memory.x`）
- **SHOULD** 新功能先用 feature flag 隔离
- **MUST** 修改 `keyboard.toml` 后验证键位映射正确性

---

## Multi-agent Safety

- **MUST** 禁止 git stash（除非用户明确要求）
- **MUST** 禁止切换分支（除非用户明确要求）
- **MUST** commit 时只提交自己明确修改的文件

---

## Agent Notes

- 回答问题时，先验证代码再回答，不要猜测
- 固件是 `no_std` 嵌入式项目，目标芯片 nRF52840（ARM Cortex-M4F），Embassy 异步运行时
- 主机应用使用 GPUI 框架，标准 Rust 环境
- 共享协议 crate 兼容 `no_std` 和 `std`

---

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 固件 | `k9-pad-firmware/` | nRF52840 BLE 键盘固件（显示、菜单、BLE、电池等） |
| 共享协议 | `shared-datachannel-proto/` | BLE 数据通道协议 crate（no_std 兼容） |
| 主机通信库 | `k9-host-lib/` | BLE/USB transport 抽象 + K9Client |
| 桌面应用 | `k9-host-app/` | GPUI 桌面管理应用（providers: time/volume/bilibili） |
