// lib.rs - 库入口（支持测试）
//
// 测试时使用 std，非测试时使用 no_std

#![cfg_attr(not(test), no_std)]

// 在 no_std 环境下，需要引入 core prelude
#[cfg(not(test))]
extern crate core;

pub mod mode;
pub mod menu;
