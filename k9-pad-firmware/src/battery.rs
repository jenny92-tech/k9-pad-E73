// INPUT:  embassy_sync, nRF52840 SAADC/GPIO registers
// OUTPUT: BatteryStatus, BATTERY_STATUS watch, calc_percentage(), ADC hardware functions
// POS:    电池管理：ADC 采样、充电检测、电量计算，通过 Watch 广播状态

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

// ============== Battery Hardware (Raw Register Access) ==============
//
// SAADC 和 GPIO 通过寄存器直接访问，因为 display task 是 RMK 宏唯一
// spawn 的自定义 async 函数，无法传入额外外设。

/// 配置 P0.07 (CHRG_DET) 为输入 + 上拉。
/// TP4054 CHRG# 是开漏输出：低电平 = 正在充电。
///
/// SAFETY: 访问 nRF52840 GPIO PIN_CNF[7] 寄存器。P0.07 未被 keyboard.toml
/// 中任何功能使用（battery 配置已注释掉），不存在竞争。
pub unsafe fn init_charge_detect_pin() {
    const P0_BASE: u32 = 0x5000_0000;
    const PIN_CNF_OFFSET: u32 = 0x700;
    // DIR=Input(0), INPUT=Connect(0), PULL=Pullup(3<<2)
    let addr = (P0_BASE + PIN_CNF_OFFSET + 7 * 4) as *mut u32;
    core::ptr::write_volatile(addr, 0x0000_000C);
}

/// 读取 P0.07 充电状态。返回 true = 正在充电（引脚低电平）。
///
/// SAFETY: 读取 nRF52840 GPIO IN 寄存器，只读操作无副作用。
pub unsafe fn read_charge_pin() -> bool {
    const P0_BASE: u32 = 0x5000_0000;
    const IN_OFFSET: u32 = 0x510;
    let state = core::ptr::read_volatile((P0_BASE + IN_OFFSET) as *const u32);
    (state & (1 << 7)) == 0 // Active low
}

/// 阻塞式 SAADC 单次读取 AIN6 (P0.30 = POWER_PIN)。
/// 返回 12-bit 原始值。每次读取前启用、读完后禁用 SAADC。
///
/// SAFETY: 访问 nRF52840 SAADC 寄存器。keyboard.toml 中 battery_adc_pin
/// 已注释掉，RMK 不会使用 SAADC，不存在竞争。
/// 阻塞等待时间约几十微秒，不影响 display loop。
unsafe fn read_battery_adc_raw() -> i16 {
    const SAADC: u32 = 0x4000_7000;

    // 启用 SAADC
    core::ptr::write_volatile((SAADC + 0x500) as *mut u32, 1); // ENABLE

    // Channel 0: AIN6 (P0.30), single-ended
    core::ptr::write_volatile((SAADC + 0x510) as *mut u32, 7); // CH[0].PSELP = AIN6
    core::ptr::write_volatile((SAADC + 0x514) as *mut u32, 0); // CH[0].PSELN = NC

    // CONFIG: Gain=1/6, Ref=Internal(0.6V), Tacq=40us, Mode=SE
    core::ptr::write_volatile(
        (SAADC + 0x518) as *mut u32,
        (2 << 8) | (0 << 12) | (5 << 16) | (0 << 20),
    );

    // Resolution 12-bit
    core::ptr::write_volatile((SAADC + 0x5F0) as *mut u32, 2);

    // 结果缓冲区（DMA 需要 static 地址）
    static mut ADC_BUF: i16 = 0;
    core::ptr::write_volatile(
        (SAADC + 0x62C) as *mut u32,
        core::ptr::addr_of_mut!(ADC_BUF) as u32,
    ); // RESULT.PTR
    core::ptr::write_volatile((SAADC + 0x630) as *mut u32, 1); // RESULT.MAXCNT

    // 清除事件
    core::ptr::write_volatile((SAADC + 0x100) as *mut u32, 0); // EVENTS_STARTED
    core::ptr::write_volatile((SAADC + 0x104) as *mut u32, 0); // EVENTS_END
    core::ptr::write_volatile((SAADC + 0x114) as *mut u32, 0); // EVENTS_STOPPED

    // Start → wait STARTED
    core::ptr::write_volatile((SAADC + 0x000) as *mut u32, 1); // TASKS_START
    while core::ptr::read_volatile((SAADC + 0x100) as *const u32) == 0 {}

    // Sample → wait END
    core::ptr::write_volatile((SAADC + 0x004) as *mut u32, 1); // TASKS_SAMPLE
    while core::ptr::read_volatile((SAADC + 0x104) as *const u32) == 0 {}

    // Stop → wait STOPPED
    core::ptr::write_volatile((SAADC + 0x008) as *mut u32, 1); // TASKS_STOP
    while core::ptr::read_volatile((SAADC + 0x114) as *const u32) == 0 {}

    // 禁用 SAADC
    core::ptr::write_volatile((SAADC + 0x500) as *mut u32, 0);

    ADC_BUF
}

/// 读取电池电压 (mV)，4 次采样取平均以降低噪声。
///
/// 硬件分压: R8=820kΩ (VBAT→POWER_PIN), R10=2MΩ (POWER_PIN→GND)
/// V_adc = VBAT × R10/(R8+R10) = VBAT × 2000/2820
/// → VBAT = V_adc × 2820/2000
///
/// SAADC 公式 (SE, 12-bit, Gain=1/6, Ref=0.6V):
/// raw = V_adc × 4096 / 3600
/// → V_adc = raw × 3600 / 4096
///
/// 合并: VBAT = raw × 3600 × 2820 / (4096 × 2000) = raw × 1269 / 1024
pub fn read_battery_voltage_mv() -> u16 {
    const SAMPLES: u32 = 4;
    let mut sum: u32 = 0;
    let mut i: u32 = 0;
    while i < SAMPLES {
        let raw = unsafe { read_battery_adc_raw() }.max(0) as u32;
        sum += (raw * 1269) / 1024;
        i += 1;
    }
    (sum / SAMPLES) as u16
}
