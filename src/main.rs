// INPUT:  rmk, embassy_nrf, battery, data_channel, mode, display, menu, wououi
// OUTPUT: Firmware entry point (pre_init + rmk_keyboard macro)
// POS:    程序入口，初始化硬件并启动 RMK 键盘主循环
#![no_std]
#![no_main]

// k9-pad-E73 Firmware - 多层键盘
// 模式: Pad A / Pad B / Pad C (对应 RMK Layer 0/1/2)

use rmk::macros::rmk_keyboard;

use panic_probe as _;

mod battery;
mod data_channel;
mod mode;
mod display;
mod menu;
mod wououi;

pub use battery::*;
pub use data_channel::*;
pub use mode::*;
pub use display::run_display;
pub use menu::*;

/// Pre-init: enable DC/DC converter for low power.
// SAFETY: Called by cortex-m-rt before main, before .data/.bss init.
// Only writes to hardware register (no RAM access needed).
#[cortex_m_rt::pre_init]
unsafe fn pre_init() {
    const DCDCEN_ADDR: *mut u32 = 0x4000_0078 as *mut u32;
    core::ptr::write_volatile(DCDCEN_ADDR, 1);
}

// RMK keyboard macro
#[rmk_keyboard]
mod keyboard {
    // Add TWI interrupt for display I2C (TWISPI0 is embassy-nrf's name for TWIM0/SPI0)
    add_interrupt! {
        TWISPI0 => ::embassy_nrf::twim::InterruptHandler<::embassy_nrf::peripherals::TWISPI0>;
    }

    #[Overwritten(entry)]
    async fn custom_entry() {
        use ::rmk::input_device::Runnable;

        // Initialize display I2C
        static TWI_BUF: ::static_cell::StaticCell<[u8; 256]> = ::static_cell::StaticCell::new();
        let twi_buf = TWI_BUF.init([0u8; 256]);
        let twi_config = ::embassy_nrf::twim::Config::default();
        let twi = ::embassy_nrf::twim::Twim::new(
            p.TWISPI0, Irqs, p.P0_08, p.P1_09, twi_config, twi_buf
        );

        // Run all tasks: devices + keyboard + RMK + display + data channel
        ::rmk::embassy_futures::join::join(
            ::rmk::embassy_futures::join::join(
                ::rmk::embassy_futures::join::join(
                    ::rmk::run_all!(matrix, encoder_0),
                    keyboard.run()
                ),
                ::rmk::run_rmk(&keymap, driver, &stack, &mut storage, rmk_config)
            ),
            ::rmk::embassy_futures::join::join(
                crate::run_display(twi, p.P0_06),
                crate::data_channel::run_data_channel()
            )
        ).await;
    }
}
