# K9-Pad E73

> nRF52840 BLE 机械键盘固件，Embassy 异步运行时 + WouoUI OLED 动画菜单

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

```bash
# 构建 release
cargo make build

# 生成 hex 文件
cargo make objcopy

# 生成 uf2 固件
cargo make uf2

# 运行测试（host 端）
cargo test --lib

# 检查编译（不生成固件）
cargo check
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

---

## Testing

- **MUST** 推送前运行测试：`cargo test --lib`
- 嵌入式相关代码使用 `#[cfg(test)]` 隔离测试

---

## Code Style

- **SHOULD** 文件不超过 500 行
- **MUST** 避免使用 `unsafe` 除非绝对必要，且必须添加 `// SAFETY:` 注释
- **SHOULD** 使用 `defmt` 进行日志输出

---

## Embedded-Specific Rules

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
- 这是 `no_std` 嵌入式项目，不能使用标准库
- 目标芯片是 nRF52840（ARM Cortex-M4F），使用 Embassy 异步运行时
- 构建工具链：`thumbv7em-none-eabihf`

---

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 固件源码 | `src/` | 主固件代码（显示、菜单、BLE、电池等） |
| 数据通道协议 | `k9-datachannel-proto/` | BLE 数据通道协议 crate（no_std） |
| 构建工具 | `tools/` | DFU 包生成、CRC 补丁等构建辅助 |
| 构建脚本 | `build.rs` | Cargo 构建配置（WouoUI C 编译、vial 配置） |
| 键盘配置 | `keyboard.toml` | RMK 键位映射定义 |
| 内存布局 | `memory.x` | Flash/RAM 分区定义 |
| 文档 | `docs/` | 踩坑文档、设计文档 |
