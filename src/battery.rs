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
/// 假设使用 1:2 分压（比如 200k+200k），Vbat/2 -> ADC
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
        
        // 计算电压 (参考电压 0.6V, 增益 1/6, 12bit)
        // Vadc = raw * 0.6 / (2^12 * 1/6) = raw * 0.6 / 682.67
        // 如果分压比是 2:1，Vbat = Vadc * 2
        let raw = buf[0].max(0) as u32;
        let v_adc = (raw * 600) / 682;  // mV
        status.voltage_mv = (v_adc * 2) as u16;  // 分压还原
        status.percentage = calc_percentage(status.voltage_mv);
        status.is_charging = chg_pin.is_low();
        
        tx.send(status);
        Timer::after(Duration::from_secs(5)).await;  // 5秒更新一次
    }
}