# 固件源码

> nRF52840 BLE 键盘固件主体，Embassy 异步运行时

## 地位

固件全部 Rust 源码。

## 逻辑

`main.rs` 初始化 → 并发 task（display, menu controller, battery, data_channel）

## 约束

- `no_std`，Embassy async
- `defmt` logging
- `unsafe` 必须附带 `// SAFETY:` 注释

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 程序入口 | `main.rs` | pre_init + RMK 键盘启动 |
| 库入口 | `lib.rs` | 测试用入口，条件 no_std |
| 显示驱动 | `display.rs` | SH1107 OLED 驱动 + 首页/菜单/数据通道渲染 |
| 数据通道 | `data_channel.rs` | BLE GATT 数据接收、协议解析、DisplayCommand 分发 |
| 电池管理 | `battery.rs` | ADC 采样、电量计算、BATTERY_STATUS Watch |
| 键码定义 | `keyboard.rs` | KeyCode 枚举（占位，对接 RMK） |
| 模式管理 | `mode.rs` | KeyboardMode 枚举 + CURRENT_MODE Watch |
| 完整性校验 | `integrity.rs` | CRC32 启动校验，损坏则进 DFU |
| 菜单系统 | `menu/` | WouoUI 菜单输入控制 + 状态管理 |
| OLED UI 框架 | `wououi/` | WouoUI C 库 FFI 绑定 |
| 独立二进制 | `bin/` | 测试/调试用独立固件 |
