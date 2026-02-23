// INPUT:  ain_channel 参数
// OUTPUT: read_single() 阻塞式 SAADC 单次采样
// POS:    通用 SAADC 寄存器操作，参数化 AIN 通道，被 battery.rs 调用

//! Generic nRF52840 SAADC register operations (parameterised by AIN channel).

const SAADC_BASE: u32 = 0x4000_7000;

/// Blocking single-shot SAADC read. Returns 12-bit raw value.
///
/// Configuration: Gain=1/6, Ref=Internal(0.6V), Tacq=40μs, 12-bit, single-ended.
/// Enables SAADC before sampling and disables it after.
///
/// SAFETY: Accesses nRF52840 SAADC registers via raw pointers.
/// Caller must ensure no concurrent SAADC access. Uses a static DMA buffer.
/// Blocking wait is ~tens of microseconds.
pub unsafe fn read_single(ain_channel: u8) -> i16 {
    // Enable SAADC
    core::ptr::write_volatile((SAADC_BASE + 0x500) as *mut u32, 1);

    // Channel 0: selected AIN, single-ended
    // PSELP value = AIN channel + 1 (AIN0=1, AIN1=2, ..., AIN7=8)
    core::ptr::write_volatile((SAADC_BASE + 0x510) as *mut u32, ain_channel as u32 + 1);
    core::ptr::write_volatile((SAADC_BASE + 0x514) as *mut u32, 0); // PSELN = NC

    // CONFIG: Gain=1/6, Ref=Internal(0.6V), Tacq=40us, Mode=SE
    core::ptr::write_volatile(
        (SAADC_BASE + 0x518) as *mut u32,
        (2 << 8) | (0 << 12) | (5 << 16) | (0 << 20),
    );

    // Resolution 12-bit
    core::ptr::write_volatile((SAADC_BASE + 0x5F0) as *mut u32, 2);

    // Result buffer (DMA needs static address)
    static mut ADC_BUF: i16 = 0;
    core::ptr::write_volatile(
        (SAADC_BASE + 0x62C) as *mut u32,
        core::ptr::addr_of_mut!(ADC_BUF) as u32,
    ); // RESULT.PTR
    core::ptr::write_volatile((SAADC_BASE + 0x630) as *mut u32, 1); // RESULT.MAXCNT

    // Clear events
    core::ptr::write_volatile((SAADC_BASE + 0x100) as *mut u32, 0); // EVENTS_STARTED
    core::ptr::write_volatile((SAADC_BASE + 0x104) as *mut u32, 0); // EVENTS_END
    core::ptr::write_volatile((SAADC_BASE + 0x114) as *mut u32, 0); // EVENTS_STOPPED

    // Start → wait STARTED
    core::ptr::write_volatile((SAADC_BASE + 0x000) as *mut u32, 1); // TASKS_START
    while core::ptr::read_volatile((SAADC_BASE + 0x100) as *const u32) == 0 {}

    // Sample → wait END
    core::ptr::write_volatile((SAADC_BASE + 0x004) as *mut u32, 1); // TASKS_SAMPLE
    while core::ptr::read_volatile((SAADC_BASE + 0x104) as *const u32) == 0 {}

    // Stop → wait STOPPED
    core::ptr::write_volatile((SAADC_BASE + 0x008) as *mut u32, 1); // TASKS_STOP
    while core::ptr::read_volatile((SAADC_BASE + 0x114) as *const u32) == 0 {}

    // Disable SAADC
    core::ptr::write_volatile((SAADC_BASE + 0x500) as *mut u32, 0);

    ADC_BUF
}
