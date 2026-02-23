# 显示系统

> OLED 显示主循环 + UI 渲染，从 WouoUI 菜单到首页键盘状态

## 地位

固件的 UI 层，负责所有 OLED 屏幕输出。由 `main.rs` 启动 `run_display()` task。

## 逻辑

`mod.rs` 主循环编排：硬件初始化 → 菜单状态机 → 帧渲染 → 屏幕刷新。
子模块按职责拆分：`render.rs` 绘制 UI、`icons.rs` 绘制状态图标、`format.rs` 格式化文本。

## 约束

- `no_std`、Embassy async
- 依赖 `driver::sh1107` 进行 I2C 通信
- 依赖 `wououi` C FFI 进行菜单动画
- `mod.rs` 不超过 500 行

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| 主循环 | `mod.rs` | run_display() 主循环、硬件初始化、菜单状态机、Flash 设置同步 |
| UI 渲染 | `render.rs` | draw_keyboard_ui() 首页 + draw_data_channel_ui() 数据通道布局 |
| 图标 | `icons.rs` | draw_battery_icon() + draw_ble_icon() 状态栏图标 |
| 格式化 | `format.rs` | format_i32() + format_progress() no_std 文本格式化 |
