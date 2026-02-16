# 菜单系统

> WouoUI 菜单的 Rust 侧控制和状态管理

## 地位

连接 RMK 按键事件和 WouoUI C 库的桥梁。

## 逻辑

`controller.rs` 监听 KeyEvent/编码器 → MenuInput channel → `display.rs` 消费

## 约束

- `controller.rs` 只监听不拦截事件
- `cfg(test)` 排除硬件依赖

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| 模块入口 | `mod.rs` | 条件编译导出 |
| 控制器 | `controller.rs` | SW1 长按进菜单、编码器滚动、确认键 |
| 状态 | `state.rs` | MenuInput/MenuState/PageId 类型 + 全局 channel |
