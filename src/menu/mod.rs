// menu/mod.rs - 菜单系统模块入口
//
// WouoUI C library 集成，通过 FFI 实现 OLED 动画菜单
// controller.rs: RMK controller 监听按键/编码器，转换为菜单输入
// state.rs: 菜单状态管理（MenuState, MenuInput channel）

pub mod state;

#[cfg(not(test))]
pub mod controller;

pub use state::*;

#[cfg(not(test))]
pub use controller::*;
