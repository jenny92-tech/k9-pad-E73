// INPUT:  rmk, embassy_nrf, battery, data_channel, mode, display, menu, wououi
// OUTPUT: Firmware entry point (pre_init + rmk_keyboard macro)
// POS:    程序入口，初始化硬件并启动 RMK 键盘主循环
#![no_std]
#![no_main]

// k9-pad-E73 Firmware - 多层键盘
// 模式: Pad A / Pad B / Pad C / Pad D / Pad E (对应 RMK Layer 0/1/2/3/4)

use rmk::macros::rmk_keyboard;

use panic_probe as _;

mod battery;
mod data_channel;
mod driver;
mod display;
mod mode;
mod menu;
mod settings;
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
    // Append TWISPI0 IRQ binding to the macro-generated `bind_interrupts!`.
    // We no longer use [display] in keyboard.toml, so the macro won't bind
    // this for us — we bring our own.
    add_interrupt! {
        TWISPI0 => ::embassy_nrf::twim::InterruptHandler<::embassy_nrf::peripherals::TWISPI0>;
    }

    #[Overwritten(entry)]
    async fn custom_entry() {
        use ::rmk::core_traits::Runnable;

        // OLED bus + reset pin. Previously injected from [display] config;
        // we now wire it manually so we can keep our SH1107 driver and
        // async render loop instead of upstream's DisplayProcessor.
        static DISPLAY_I2C_BUF: ::static_cell::StaticCell<[u8; 256]> =
            ::static_cell::StaticCell::new();
        let display_i2c_buf = DISPLAY_I2C_BUF.init([0u8; 256]);
        let mut i2c_config = ::embassy_nrf::twim::Config::default();
        // embassy-nrf 默认 frequency 是 K100；旧版 RMK macro 显式把 SH1107 拉到 400kHz
        // (传一帧 1024 字节从 ~80ms 降到 ~20ms)，新的 upstream macro 模板没保留这一点。
        i2c_config.frequency = ::embassy_nrf::twim::Frequency::K400;
        let i2c = ::embassy_nrf::twim::Twim::new(
            p.TWISPI0,
            Irqs,
            p.P0_08,
            p.P1_09,
            i2c_config,
            display_i2c_buf,
        );
        let reset_pin = p.P0_06;

        // RMK transports + WPM processor — created here because
        // #[Overwritten(entry)] replaces the macro's default entry body
        // which would normally instantiate them.
        //
        // CRITICAL: macro 还会自动生成 `watchdog_runner`（feature "watchdog" 默认开），
        // 必须包含 `watchdog_runner.run()` 喂狗，否则硬件 WDT 10s 超时硬复位设备。
        let mut wpm_processor = ::rmk::processor::builtin::wpm::WpmProcessor::new();
        let mut usb_transport = ::rmk::usb::UsbTransport::new(driver, rmk_config.device_config);
        let mut ble_transport = ::rmk::ble::BleTransport::new(&stack, rmk_config).await;

        // Run all tasks: devices + keyboard + transports + display + data channel + watchdog
        ::rmk::embassy_futures::join::join(
            ::rmk::embassy_futures::join::join(
                ::rmk::embassy_futures::join::join(
                    ::rmk::run_all!(matrix, encoder_0, storage),
                    keyboard.run()
                ),
                ::rmk::embassy_futures::join::join(
                    host_service.run(),
                    ::rmk::embassy_futures::join::join(
                        ::rmk::embassy_futures::join::join(
                            usb_transport.run(),
                            ble_transport.run(),
                        ),
                        ::rmk::embassy_futures::join::join(
                            wpm_processor.run(),
                            watchdog_runner.run(),
                        ),
                    ),
                )
            ),
            ::rmk::embassy_futures::join::join(
                crate::run_display(i2c, reset_pin),
                crate::data_channel::run_data_channel()
            )
        ).await;
    }
}
