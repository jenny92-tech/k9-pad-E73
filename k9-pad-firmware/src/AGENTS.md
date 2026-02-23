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
- 文件不超过 500 行

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| 程序入口 | `main.rs` | pre_init + RMK 键盘启动 |
| 库入口 | `lib.rs` | 测试用入口，条件 no_std |
| 硬件驱动 | `driver/` | 板级常量(board)、GPIO/SAADC 寄存器操作、SH1107 OLED 驱动、Flash KV 存储 |
| 显示系统 | `display/` | OLED 显示主循环 + UI 渲染（首页/菜单/数据通道） |
| 数据通道 | `data_channel/` | BLE GATT 数据接收、协议解析(parse)、DisplayCommand 分发(task) |
| 电池管理 | `battery.rs` | 充电检测、电量计算、BATTERY_STATUS Watch（硬件操作委托 driver 层） |
| 模式管理 | `mode.rs` | KeyboardMode 结构体 + NUM_LAYERS 常量 + CURRENT_MODE Watch |
| 应用设置 | `settings.rs` | 亮度等持久化设置 key 定义 + SETTINGS 全局实例 |
| 菜单系统 | `menu/` | WouoUI 菜单输入控制 + 状态管理 |
| OLED UI 框架 | `wououi/` | WouoUI C 库 FFI 绑定 |
| 独立二进制 | `bin/` | 测试/调试用独立固件 |
