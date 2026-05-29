// INPUT:  driver::gpio, driver::board, embassy_nrf::saadc, embassy_time, rmk::event
// OUTPUT: BatteryStatus, BATTERY_STATUS watch, calc_percentage(), run_battery() 异步采样任务
// POS:    电池管理应用层：异步 SAADC 采样任务（充电检测、电量计算、EMA 平滑），经 Watch 广播 + RMK BLE 电量服务

use embassy_nrf::saadc::Saadc;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Timer};
use rmk::event::{publish_event, BatteryStatusEvent};
use rmk::types::battery::{BatteryStatus as RmkBatteryStatus, ChargeState};

use crate::driver::{board, gpio};

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
// 节点 (电压 mV, 电量 %)，已按 ADC 偏移重新标定。曲线对电压严格单调递减、
// 两端钳位到 100/0；各节点压降非均匀（平台区密、两端疏），符合 LiPo 特性。
const DISCHARGE_CURVE: [(u16, u8); 12] = [
    (4100, 100),
    (3980, 90),
    (3900, 80),
    (3850, 70),
    (3800, 60),
    (3770, 50),
    (3740, 40),
    (3700, 30),
    (3660, 20),
    (3630, 10),
    (3480, 5),
    (3300, 0),
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

/// 电池采样任务：异步 SAADC（P0.30/AIN6）+ 充电检测。每 5 秒一轮：多次采样取平均、
/// 算电量、EMA 平滑，广播到 `BATTERY_STATUS`，并同步给 RMK BLE 电量服务。
///
/// 关键点：用 embassy 异步 SAADC（`sample().await`），转换期间**让出执行器**，
/// 不再像旧的寄存器忙等那样阻塞 BLE/渲染任务。
pub async fn run_battery(mut saadc: Saadc<'static, 1>) -> ! {
    // 充电检测引脚（P0.07 上拉输入）。
    // SAFETY: P0.07 未被其他任务使用，见 init_charge_detect_pin 文档。
    unsafe { init_charge_detect_pin() };

    let tx = BATTERY_STATUS.sender();
    let mut smooth_pct_x10: u16 = 0;
    let mut initialized = false;

    loop {
        // 多次采样取平均降噪（沿用 board::BATTERY_SAMPLE_COUNT）。
        let mut sum: u32 = 0;
        let mut i: u32 = 0;
        while i < board::BATTERY_SAMPLE_COUNT {
            let mut buf = [0i16; 1];
            saadc.sample(&mut buf).await;
            let raw = buf[0].max(0) as u32;
            sum += (raw * board::BATTERY_RAW_TO_MV_NUM) / board::BATTERY_RAW_TO_MV_DEN;
            i += 1;
        }
        let voltage = (sum / board::BATTERY_SAMPLE_COUNT) as u16;
        let raw_pct = calc_percentage(voltage);
        // SAFETY: GPIO 数字读，瞬时无副作用。
        let is_charging = unsafe { read_charge_pin() };

        // EMA 平滑：smooth = raw*3 + prev*7（α≈0.3），×10 精度防截断累积误差。
        // 充电时 / 首次不平滑（允许快速反映充电进度，且首次直接取真实值而非从 0 爬升）。
        let smoothed_pct = if is_charging || !initialized {
            smooth_pct_x10 = raw_pct as u16 * 10;
            raw_pct
        } else {
            smooth_pct_x10 = (raw_pct as u16 * 10 * 3 + smooth_pct_x10 * 7 + 5) / 10;
            ((smooth_pct_x10 + 5) / 10) as u8
        };
        initialized = true;

        let status = BatteryStatus {
            voltage_mv: voltage,
            percentage: smoothed_pct,
            is_charging,
        };
        tx.send(status);

        // 同步给 RMK BLE Battery Service（让 Windows/macOS 看到电量）。
        publish_event(BatteryStatusEvent(RmkBatteryStatus::Available {
            charge_state: ChargeState::from(is_charging),
            level: Some(smoothed_pct),
        }));

        defmt::info!(
            "Battery: {}mV raw={}% smooth={}% charging={}",
            voltage,
            raw_pct,
            smoothed_pct,
            is_charging
        );

        Timer::after(Duration::from_secs(5)).await;
    }
}
