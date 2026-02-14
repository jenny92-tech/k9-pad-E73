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

/// ADC 采样计算电压
/// 实际分压: R8=820kΩ (VBAT→POWER_PIN), R10=2MΩ (POWER_PIN→GND)
/// V_adc = VBAT × 2000/2820, 即 VBAT = V_adc × 1.41
pub fn calc_percentage(voltage_mv: u16) -> u8 {
    // 锂电池：4.2V=100%, 3.3V=0%
    const MAX_MV: u16 = 4200;
    const MIN_MV: u16 = 3300;
    
    if voltage_mv >= MAX_MV { return 100; }
    if voltage_mv <= MIN_MV { return 0; }
    ((voltage_mv - MIN_MV) as u32 * 100 / (MAX_MV - MIN_MV) as u32) as u8
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