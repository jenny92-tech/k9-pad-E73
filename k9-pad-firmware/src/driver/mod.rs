// INPUT:  board, gpio, saadc, sh1107, flash
// OUTPUT: pub mod board/gpio/saadc/sh1107/flash; enable_i2c_pullups(), enable_oled_power()
// POS:    硬件抽象层入口，聚合芯片级驱动 + GPIO 初始化
pub mod board;
pub mod gpio;
pub mod saadc;
pub mod sh1107;
pub mod flash;

/// 启用 I2C GPIO 内部上拉 (SDA/SCL pins from board config)
///
/// SAFETY: Accesses nRF52840 GPIO PIN_CNF registers via raw pointers.
/// Must be called before I2C communication begins. No concurrent GPIO access.
pub unsafe fn enable_i2c_pullups() {
    gpio::set_pullup(board::I2C_SDA.0, board::I2C_SDA.1);
    gpio::set_pullup(board::I2C_SCL.0, board::I2C_SCL.1);
    defmt::info!("I2C pullups enabled");
}

/// 启用 OLED 电源开关 (pin from board config → High)
///
/// SAFETY: Configures OLED power pin as output and sets it high.
/// No other code accesses this pin.
pub unsafe fn enable_oled_power() {
    gpio::configure_output_high(board::OLED_POWER.0, board::OLED_POWER.1);
    defmt::info!("OLED power enabled (P0.05)");
}
