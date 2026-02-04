#![no_std]
#![no_main]

// k9-pad-E73 Firmware - 多模式键盘
// 模式: MEDIA / EXCEL / CLAUDE

use rmk::macros::rmk_keyboard;

use panic_probe as _;

mod battery;
mod mode;
mod display;
mod keyboard;

pub use battery::*;
pub use mode::*;
pub use display::run_display;
pub use keyboard::*;

/// Pre-init: Enable DC/DC for low power
#[cortex_m_rt::pre_init]
unsafe fn pre_init() {
    const DCDCEN_ADDR: *mut u32 = 0x4000_0078 as *mut u32;
    core::ptr::write_volatile(DCDCEN_ADDR, 1);
}

// RMK keyboard macro
#[rmk_keyboard]
mod keyboard {}
