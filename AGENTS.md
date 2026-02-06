# Repository Guidelines

> 这是项目的 AI 开发规范。规则会随着项目成长逐步添加。
> 原则：从简单开始，遇到问题再加规则。

---

## Project Structure

```
src/                # 主源代码
  ├── menu/         # 菜单系统
  ├── wououi/       # OLED UI 框架
  ├── main.rs       # 程序入口
  ├── lib.rs        # 库入口
  ├── battery.rs    # 电池管理
  ├── display.rs    # 显示驱动
  ├── keycode_defs.rs # 键码定义
  └── mode.rs       # 模式管理
menu-core/          # 菜单核心逻辑（独立 crate）
tests/              # 测试文件
docs/               # 文档
scripts/            # 脚本工具
```

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

> 这些规则继承自全局规则，在此重申以强调重要性。

- **MUST** 禁止使用 `git add .` 或 `git add -A`
  正确做法：使用 `scripts/committer` 或明确指定文件。

- **MUST** 禁止提交敏感信息（API keys, 密码, tokens）
  正确做法：使用 placeholder，检查 .gitignore。

- **MUST** 禁止提交编译产物（*.bin, *.elf, *.hex, *.uf2, target/）
  正确做法：确保 .gitignore 已覆盖这些文件。

---

## Commit Guidelines

- 使用 `scripts/committer "<msg>" <files...>` 提交代码。

- 提交信息格式：`type(scope): description`
  ```
  feat(display): add battery indicator
  fix(ble): correct connection timeout
  docs: update README
  chore: update dependencies
  ```

- type 类型：
  - `feat`: 新功能
  - `fix`: Bug 修复
  - `docs`: 文档
  - `style`: 格式（不影响代码运行）
  - `refactor`: 重构
  - `test`: 测试
  - `chore`: 杂项

---

## Testing

- **MUST** 推送前运行测试：`cargo test --lib`
- 修改逻辑代码后必须确保测试通过。
- 嵌入式相关代码使用 `#[cfg(test)]` 隔离测试。
- 纯测试改动不需要 changelog。

---

## Code Style

- **SHOULD** 文件不超过 500 行。超过时考虑拆分。
- **SHOULD** 复杂逻辑添加简短注释。
- **MUST** 避免使用 `unsafe` 除非绝对必要，且必须添加 `// SAFETY:` 注释。
- **SHOULD** 使用 `defmt` 进行日志输出，不要使用 `println!`（嵌入式环境不支持）。

---

## Embedded-Specific Rules

- **MUST** 注意 `no_std` 环境限制：不能使用标准库的 `std::*`。
- **MUST** 注意内存布局（`memory.x`），修改前需理解 Flash/RAM 分区。
- **SHOULD** 新功能先用 feature flag 隔离，确认稳定后再默认启用。
- **MUST** 修改 `keyboard.toml` 后验证键位映射正确性。

---

## Multi-agent Safety

> 如果多个 AI Agent 同时工作，必须遵循以下规则。

- **MUST** 禁止 git stash（除非用户明确要求）
- **MUST** 禁止切换分支（除非用户明确要求）
- **MUST** commit 时只提交自己明确修改的文件
- **SHOULD** 看到不认识的文件时，忽略它，专注自己的任务

---

## Agent Notes

- 回答问题时，先验证代码再回答，不要猜测。
- 这是 `no_std` 嵌入式项目，不能使用标准库。
- 目标芯片是 nRF52840（ARM Cortex-M4F），使用 Embassy 异步运行时。
- 构建工具链：`thumbv7em-none-eabihf`。

---

## Rules Evolution

> 记录规则的演进历史，帮助理解为什么有这些规则。

| 日期 | 规则 | 起因 | 类型 |
|------|------|------|------|
| 2026-02-06 | 初始化项目规范 | 项目创建 | - |
