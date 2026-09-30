// INPUT:  notchkit-core（公共刘海组件的 renderer-independent motion engine）
// OUTPUT: SpringVal / Spring 与 K9 既有 open/close/pop 参数名的薄兼容层
// POS:    刘海动画适配层 — 数值积分实现由独立 notchkit-rs 仓库统一维护，K9 只保留语义别名

pub use notchkit_core::{AnimatedValue as SpringVal, Spring};

pub const SPRING_OPEN: Spring = notchkit_core::MotionPreset::ELEGANT.open;
pub const SPRING_CLOSE: Spring = notchkit_core::MotionPreset::ELEGANT.close;
pub const SPRING_POP: Spring = notchkit_core::MotionPreset::ELEGANT.appear;
