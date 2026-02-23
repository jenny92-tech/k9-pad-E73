// INPUT:  无（纯常量定义）
// OUTPUT: 全部板级硬件常量（引脚、ADC、显示、Flash）
// POS:    硬件"魔法数字"唯一来源，V2 升级时拆分本文件即可

//! Board-level hardware constants for K9-Pad V1 (nRF52840 E73).
//!
//! All pin assignments, calibration values, and peripheral addresses
//! are centralised here. When V2 hardware arrives, split this file
//! into `board/v1.rs` + `board/v2.rs` with feature-flag selection.

// ===== GPIO Port Base (nRF52840 fixed) =====
pub const P0_BASE: u32 = 0x5000_0000;
pub const P1_BASE: u32 = 0x5000_0300;

// ===== Pin Assignments (V2 may change) =====
/// I2C SDA — (port_base, pin_number)
pub const I2C_SDA: (u32, u8) = (P0_BASE, 8); // P0.08
/// I2C SCL
pub const I2C_SCL: (u32, u8) = (P1_BASE, 9); // P1.09
/// OLED power switch
pub const OLED_POWER: (u32, u8) = (P0_BASE, 5); // P0.05
/// TP4054 charge detect (active low)
pub const CHARGE_DET: (u32, u8) = (P0_BASE, 7); // P0.07

// ===== SAADC (V2 may change ADC channel) =====
/// Battery ADC input: AIN6 = P0.30
pub const BATTERY_ADC_AIN: u8 = 6;

// ===== Battery Calibration (V2 may change resistor divider) =====
/// VBAT = raw × RAW_TO_MV_NUM / RAW_TO_MV_DEN
///
/// Derivation: R8=820kΩ, R10=2MΩ, Gain=1/6, Ref=0.6V, 12-bit
/// V_adc = VBAT × 2000/2820, raw = V_adc × 4096/3600
/// → VBAT = raw × 3600 × 2820 / (4096 × 2000) = raw × 1269 / 1024
pub const BATTERY_RAW_TO_MV_NUM: u32 = 1269;
pub const BATTERY_RAW_TO_MV_DEN: u32 = 1024;
pub const BATTERY_SAMPLE_COUNT: u32 = 4;

// ===== Display (V2 may change screen) =====
pub const DISPLAY_I2C_ADDR: u8 = 0x3C;
pub const DISPLAY_WIDTH: u32 = 128;
pub const DISPLAY_HEIGHT: u32 = 64;

// ===== Flash Settings (V2 typically unchanged) =====
pub const SETTINGS_PAGE_ADDR: u32 = 0x000F_3000;
pub const SETTINGS_PAGE_SIZE: usize = 4096;
pub const SETTINGS_MAGIC: [u8; 2] = [0x4B, 0x39]; // "K9"
