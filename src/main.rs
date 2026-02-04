#![no_std]
#![no_main]

// k9-pad-E73 Firmware
// OLED: SH1107 64x128 (schematic shows SSD1312 placeholder)
// I2C: SDA=P0.08, SCL=P1.09, RESET=P0.06
// 
// NOTE: Display code is ready but requires RMK's upcoming display support
// or manual keyboard initialization. Current setup uses RMK macro for
// reliable keyboard operation.

use core::fmt::Write;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::peripherals::P0_06;
use embassy_nrf::twim::Twim;
use embassy_nrf::Peri;
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use rmk::macros::rmk_keyboard;
use ssd1306::{
    command::Command,
    prelude::*,
    size::DisplaySize,
    I2CDisplayInterface, Ssd1306,
};
use display_interface::DisplayError;
use arrayvec::ArrayString;
use static_cell::StaticCell;

use panic_probe as _;

/// Pre-init: Enable DC/DC for low power
#[cortex_m_rt::pre_init]
unsafe fn pre_init() {
    const DCDCEN_ADDR: *mut u32 = 0x4000_0078 as *mut u32;
    core::ptr::write_volatile(DCDCEN_ADDR, 1);
}

// SH1107 64x128 display size
pub struct DisplaySize64x128;

impl DisplaySize for DisplaySize64x128 {
    const WIDTH: u8 = 64;
    const HEIGHT: u8 = 128;
    const DRIVER_COLS: u8 = 128;
    const DRIVER_ROWS: u8 = 128;
    const OFFSETX: u8 = 32;
    const OFFSETY: u8 = 0;
    type Buffer = [u8; (Self::WIDTH as usize * Self::HEIGHT as usize) / 8];

    fn configure(
        &self,
        iface: &mut impl WriteOnlyDataCommand,
    ) -> Result<(), DisplayError> {
        Command::ComPinConfig(false, true).send(iface)
    }
}

type DisplayString = ArrayString<32>;
static TX_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();

/// Display driver - ready for integration when RMK display support lands
/// or when switching to manual keyboard initialization
#[allow(dead_code)]
pub async fn run_display(
    i2c: Twim<'static>,
    reset: Peri<'static, P0_06>,
) {
    let mut reset_pin = Output::new(reset, Level::High, OutputDrive::Standard);
    reset_pin.set_low();
    Timer::after(Duration::from_millis(10)).await;
    reset_pin.set_high();
    Timer::after(Duration::from_millis(10)).await;
    drop(reset_pin);

    let interface = I2CDisplayInterface::new_custom_address(i2c, 0x3c);
    let mut display = Ssd1306::new(
        interface,
        DisplaySize64x128,
        DisplayRotation::Rotate0,
    )
    .into_buffered_graphics_mode();
    
    if display.init().is_err() {
        defmt::warn!("OLED init failed");
        return;
    }

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    let mut seconds: u32 = 0;
    
    loop {
        display.clear(BinaryColor::Off).ok();
        Text::with_baseline("k9", Point::new(0, 0), text_style, Baseline::Top)
            .draw(&mut display).ok();
        Text::with_baseline("pad", Point::new(0, 12), text_style, Baseline::Top)
            .draw(&mut display).ok();
        Text::with_baseline("E73", Point::new(0, 26), text_style, Baseline::Top)
            .draw(&mut display).ok();
        Text::with_baseline("----", Point::new(0, 42), text_style, Baseline::Top)
            .draw(&mut display).ok();
        Text::with_baseline("BLE", Point::new(0, 56), text_style, Baseline::Top)
            .draw(&mut display).ok();
        Text::with_baseline("OK", Point::new(0, 70), text_style, Baseline::Top)
            .draw(&mut display).ok();
        
        let mut buf = DisplayString::new();
        write!(&mut buf, "T:{}", seconds).ok();
        Text::with_baseline(buf.as_str(), Point::new(0, 88), text_style, Baseline::Top)
            .draw(&mut display).ok();
        
        display.flush().ok();
        seconds = seconds.wrapping_add(1);
        Timer::after(Duration::from_secs(1)).await;
    }
}

// RMK keyboard - uses keyboard.toml config
#[rmk_keyboard]
mod keyboard {}
