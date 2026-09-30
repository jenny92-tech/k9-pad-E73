# K9-Pad 固件

> nRF52840 BLE 机械键盘固件，Embassy 异步运行时 + WouoUI OLED 动画菜单

## 地位

Monorepo 中被 `exclude` 的独立 crate，使用自己的 `.cargo/config.toml`（ARM target）。
通过 `path = "../shared-datachannel-proto"` 引用共享协议。

## 构建

```bash
# 在本目录（k9-pad-firmware/）下执行
cargo make build       # 编译 release
cargo make objcopy     # 生成 hex
cargo make uf2         # 生成 UF2
cargo test --lib --target aarch64-apple-darwin   # 运行测试（host target，嵌入式依赖按 target 隔离）
make dfu               # 生成 BLE OTA 升级包
make flash             # SWD 烧录
make flash-init        # 首次烧录（全套初始化）
```

## 约束

- `no_std` 环境，目标 `thumbv7em-none-eabihf`
- Embassy 异步运行时
- WouoUI C 库通过 `build.rs` 交叉编译（需要 `arm-none-eabi-gcc`）
- `shared-datachannel-proto` 通过 `package` 别名为 `k9-datachannel-proto` 引入
- Cargo.toml 嵌入式依赖在 `[target.'cfg(all(target_arch = "arm", target_os = "none"))'.dependencies]`
  下（host 测试不拉入）；`[dependencies]` 里有一个 optional 的 rmk stub 条目，
  仅供 rmk-macro 解析 feature 奇偶性，**不可删除**
- rustc 1.97.1 与当前 rmk fork（rev ac179536）不兼容（`pin!` E0308），
  固件构建/检查需用 `cargo +1.96.1`

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 固件源码 | `src/` | 主固件代码（显示、菜单、BLE、电池等） |
| 测试 | `tests/` | 集成测试 |
| 构建工具 | `tools/` | DFU 包生成、CRC 补丁等构建辅助 |
| 构建脚本 | `build.rs` | Cargo 构建配置（WouoUI C 编译、vial 配置） |
| 键盘配置 | `keyboard.toml` | RMK 键位映射定义 |
| 内存布局 | `memory.x` | Flash/RAM 分区定义 |
| 文档 | `docs/` | 踩坑文档、设计文档 |
| 脚本 | `scripts/` | 构建辅助脚本 |
