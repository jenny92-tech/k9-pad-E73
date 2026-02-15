// INPUT:  embassy_nrf(saadc, gpio), embassy_sync
// OUTPUT: BatteryStatus, BATTERY_STATUS watch, calc_percentage(), battery_task()
// POS:    电池 ADC 采样与电量计算，通过 Watch 广播状态
// battery.rs - 电池状态检测
use embassy_nrf::saadc::Saadc;
use embassy_nrf::gpio::Input;
use embassy_time::{Duration, Timer};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::watch::Watch;

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
    (4200, 100),
    (4060, 90),
    (3980, 80),
    (3920, 70),
    (3870, 60),
    (3830, 50),
    (3790, 40),
    (3750, 30),
    (3710, 20),
    (3670, 10),
    (3500, 5),
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

/// 硬件连接（来自你发的原理图截图）
/// - CHRG_DET → nRF P0.07（TP4054 的 CHRG# 开漏输出：充电中=低）
/// - POWER_PIN(VBAT分压采样) → nRF SAADC AIN6（符号上标为 AI6）
///
/// 充电检测引脚建议：上拉（因为 TP4054 CHRG 是开漏）。
///
/// 电池监控任务
pub async fn battery_monitor_task(
    mut saadc: Saadc<'static, 1>,
    chg_pin: Input<'static>,  // CHRG_DET，低电平=充电中
) {
    let mut status = BatteryStatus::default();
    let tx = BATTERY_STATUS.sender();
    
    loop {
        // 读取 ADC
        let mut buf = [0i16; 1];
        saadc.sample(&mut buf).await;
        
        // 计算电压 (参考电压 0.6V, 增益 1/6, 12bit, SE)
        // raw = V_adc × 4096 / 3600
        // 分压: R8=820kΩ, R10=2MΩ → VBAT = V_adc × 2820/2000
        // 合并: VBAT_mV = raw × 3600 × 2820 / (4096 × 2000) = raw × 1269 / 1024
        let raw = buf[0].max(0) as u32;
        status.voltage_mv = ((raw * 1269) / 1024) as u16;
        status.percentage = calc_percentage(status.voltage_mv);
        status.is_charging = chg_pin.is_low();
        
        tx.send(status);
        Timer::after(Duration::from_secs(5)).await;  // 5秒更新一次
    }
}