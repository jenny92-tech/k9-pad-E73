// INPUT:  driver::gpio, driver::saadc, driver::board, embassy_sync
// OUTPUT: BatteryStatus, BATTERY_STATUS watch, calc_percentage(), battery hardware wrappers
// POS:    电池管理应用层：充电检测、电量计算，通过 Watch 广播状态；硬件操作委托 driver 层

use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::watch::Watch;

use crate::driver::{board, gpio, saadc};

/// 电池状态数据
#[derive(Clone, Copy, Debug, Default)]
pub struct BatteryStatus {
    pub voltage_mv: u16,      // 电池电压 mV
    pub percentage: u8,       // 电量百分比
    pub is_charging: bool,    // 是否充电中
}

// 全局状态，供显示任务读取
pub static BATTERY_STATUS: Watch<ThreadModeRawMutex, BatteryStatus, 1> = Watch::new();

/// 锂电池放电曲线查找表（电压 mV → 电量百分比）
/// 基于典型 LiPo 单芯电池轻负载放电特性，按电压降序排列
/// 锂电池放电曲线高度非线性：3.9V~3.7V 是长平台区（容量主体），
/// 两端（满充/接近耗尽）电压变化快但容量变化小。
const DISCHARGE_CURVE: [(u16, u8); 12] = [
    (4100, 100), // 4200 - 100
    (3980, 90),  // 4060 - 84
    (3900, 80),  // 3980 - 76
    (3850, 70),  // 3920 - 69
    (3800, 60),  // 3870 - 63
    (3770, 50),  // 3830 - 59
    (3740, 40),  // 3790 - 54
    (3700, 30),  // 3750 - 50
    (3660, 20),  // 3710 - 46
    (3630, 10),  // 3670 - 41
    (3480, 5),   // 3500 - 22
    (3300, 0),   // 3300 - 0
];

/// 根据电压计算电量百分比（查找表 + 线性插值）
pub fn calc_percentage(voltage_mv: u16) -> u8 {
    // 边界检查
    if voltage_mv >= DISCHARGE_CURVE[0].0 {
        return DISCHARGE_CURVE[0].1;
    }
    let last = DISCHARGE_CURVE.len() - 1;
    if voltage_mv <= DISCHARGE_CURVE[last].0 {
        return DISCHARGE_CURVE[last].1;
    }

    // 在相邻节点间线性插值
    let mut i = 0;
    while i < DISCHARGE_CURVE.len() - 1 {
        let (v_high, p_high) = DISCHARGE_CURVE[i];
        let (v_low, p_low) = DISCHARGE_CURVE[i + 1];
        if voltage_mv >= v_low {
            let v_range = (v_high - v_low) as u32;
            let p_range = (p_high - p_low) as u32;
            let v_offset = (voltage_mv - v_low) as u32;
            return (p_low as u32 + v_offset * p_range / v_range) as u8;
        }
        i += 1;
    }

    0
}

// ============== Battery Hardware Wrappers ==============
//
// Thin wrappers over driver::gpio / driver::saadc using board constants.

/// 配置充电检测引脚为输入 + 上拉。
/// TP4054 CHRG# 是开漏输出：低电平 = 正在充电。
///
/// SAFETY: Accesses nRF52840 GPIO PIN_CNF register for CHARGE_DET pin.
/// Pin is not used by keyboard.toml, no concurrent access.
pub unsafe fn init_charge_detect_pin() {
    gpio::configure_input_pullup(board::CHARGE_DET.0, board::CHARGE_DET.1);
}

/// 读取充电状态。返回 true = 正在充电（引脚低电平，active low）。
///
/// SAFETY: Reads nRF52840 GPIO IN register, read-only, no side effects.
pub unsafe fn read_charge_pin() -> bool {
    !gpio::read_pin(board::CHARGE_DET.0, board::CHARGE_DET.1)
}

/// 读取电池电压 (mV)，多次采样取平均以降低噪声。
///
/// 使用 board 中的校准常量将 ADC 原始值转换为电压。
pub fn read_battery_voltage_mv() -> u16 {
    let mut sum: u32 = 0;
    let mut i: u32 = 0;
    while i < board::BATTERY_SAMPLE_COUNT {
        // SAFETY: SAADC not used by RMK (battery_adc_pin commented out in keyboard.toml).
        // Blocking wait ~tens of microseconds per sample.
        let raw = unsafe { saadc::read_single(board::BATTERY_ADC_AIN) }.max(0) as u32;
        sum += (raw * board::BATTERY_RAW_TO_MV_NUM) / board::BATTERY_RAW_TO_MV_DEN;
        i += 1;
    }
    (sum / board::BATTERY_SAMPLE_COUNT) as u16
}
