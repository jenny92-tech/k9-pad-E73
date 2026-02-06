#![no_std]
#![no_main]

// k9-pad-E73 Firmware - 多模式键盘
// 模式: MEDIA / EXCEL / CLAUDE

use rmk::macros::rmk_keyboard;

use panic_probe as _;

mod battery;
mod mode;
mod display;
mod menu;
mod wououi;

pub use battery::*;
pub use mode::*;
pub use display::run_display;
pub use menu::*;

/// Pre-init: Enable DC/DC for low power
// SAFETY: Called by cortex-m-rt before main. The address 0x4000_0078 is the
// nRF52840 POWER.DCDCEN register. Writing 1 enables the DC/DC converter.
// No other code runs at this point, so there are no data races.
#[cortex_m_rt::pre_init]
unsafe fn pre_init() {
    const DCDCEN_ADDR: *mut u32 = 0x4000_0078 as *mut u32;
    core::ptr::write_volatile(DCDCEN_ADDR, 1);
}

// RMK keyboard macro
#[rmk_keyboard]
mod keyboard {
    use crate::menu::MenuController;
    use rmk::controller::PollingController;

    /// 注册菜单控制器
    /// 监听 KeyEvent，在菜单模式下将按键转换为菜单输入
    /// 使用 poll 模式每 50ms 检测 SW1 长按
    #[register_controller(poll)]
    fn menu_controller() -> MenuController {
        MenuController::new()
    }
}
