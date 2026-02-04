#![no_std]
#![no_main]

// OLED Display support for SH1107 64x128 (vertical)
// I2C: SDA=P0.08, SCL=P1.09, RESET=P0.06
// Note: Schematic shows SSD1312 placeholder, actual part is SH1107 64x128

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

// Panic handler
use panic_probe as _;

// Custom DisplaySize for SH1107 64x128
pub struct DisplaySize64x128;

impl DisplaySize for DisplaySize64x128 {
    const WIDTH: u8 = 64;
    const HEIGHT: u8 = 128;
    const DRIVER_COLS: u8 = 128; // SH1107 has 128 columns internally
    const DRIVER_ROWS: u8 = 128; // SH1107 has 128 rows internally
    const OFFSETX: u8 = 32;      // Center 64px in 128px width
    const OFFSETY: u8 = 0;
    type Buffer = [u8; (Self::WIDTH as usize * Self::HEIGHT as usize) / 8]; // 1024 bytes

    fn configure(
        &self,
        iface: &mut impl WriteOnlyDataCommand,
    ) -> Result<(), DisplayError> {
        // SH1107 uses different COM pin config than SSD1306
        // Use alternative COM pin config for vertical layout
        Command::ComPinConfig(false, true).send(iface)
    }
}

// Buffer types
type DisplayString = ArrayString<32>;

// Static I2C TX buffer
static TX_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();

/// Initialize and run OLED display task
/// Displays: device name, status, and uptime
/// Screen: SH1107 64x128 (vertical layout)
async fn run_display(
    i2c: Twim<'static>,
    reset: Peri<'static, P0_06>,
) {
    // Reset OLED (active low)
    let mut reset_pin = Output::new(reset, Level::High, OutputDrive::Standard);
    reset_pin.set_low();
    Timer::after(Duration::from_millis(10)).await;
    reset_pin.set_high();
    Timer::after(Duration::from_millis(10)).await;
    drop(reset_pin);

    // Initialize SH1107 driver (compatible with SSD1306 commands)
    let interface = I2CDisplayInterface::new_custom_address(i2c, 0x3c);
    let mut display = Ssd1306::new(
        interface,
        DisplaySize64x128,
        DisplayRotation::Rotate0, // Native 64x128 vertical
    )
    .into_buffered_graphics_mode();
    
    if display.init().is_err() {
        return; // Display init failed
    }

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    let mut seconds: u32 = 0;
    
    loop {
        display.clear(BinaryColor::Off).ok();
        
        // On 64x128 vertical screen:
        // X range: 0-63, Y range: 0-127
        // Layout optimized for narrow screen
        Text::with_baseline(
            "k9",
            Point::new(0, 0),
            text_style,
            Baseline::Top,
        )
        .draw(&mut display).ok();

        Text::with_baseline(
            "pad",
            Point::new(0, 12),
            text_style,
            Baseline::Top,
        )
        .draw(&mut display).ok();

        Text::with_baseline(
            "E73",
            Point::new(0, 26),
            text_style,
            Baseline::Top,
        )
        .draw(&mut display).ok();

        // Separator
        Text::with_baseline(
            "----",
            Point::new(0, 42),
            text_style,
            Baseline::Top,
        )
        .draw(&mut display).ok();

        // Status
        Text::with_baseline(
            "BLE",
            Point::new(0, 56),
            text_style,
            Baseline::Top,
        )
        .draw(&mut display).ok();

        Text::with_baseline(
            "OK",
            Point::new(0, 70),
            text_style,
            Baseline::Top,
        )
        .draw(&mut display).ok();

        // Uptime
        let mut buf = DisplayString::new();
        write!(&mut buf, "T:{}", seconds).ok();
        Text::with_baseline(
            buf.as_str(),
            Point::new(0, 88),
            text_style,
            Baseline::Top,
        )
        .draw(&mut display).ok();

        display.flush().ok();
        seconds = seconds.wrapping_add(1);
        Timer::after(Duration::from_secs(1)).await;
    }
}

// RMK keyboard module - generates keyboard task from keyboard.toml
// Display support is initialized separately before keyboard starts
#[rmk_keyboard]
mod keyboard {}
