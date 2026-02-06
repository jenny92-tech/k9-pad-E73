// menu/mod.rs - 菜单系统模块入口
//
// 参考 WouoUI 设计思想，用 Rust 重写的 OLED 菜单系统
// 支持：列表菜单、滚动、选中高亮、动画

pub mod state;
pub mod page;
pub mod renderer;
pub mod input;
pub mod animation;

#[cfg(not(test))]
pub mod processor;

#[cfg(not(test))]
pub mod controller;

pub use state::*;
pub use page::*;
pub use renderer::*;
pub use input::*;

#[cfg(not(test))]
pub use processor::*;

#[cfg(not(test))]
pub use controller::*;
