// INPUT:  ain_channel 参数
// OUTPUT: read_single() 阻塞式 SAADC 单次采样
// POS:    通用 SAADC 寄存器操作，参数化 AIN 通道，被 battery.rs 调用

//! Generic nRF52840 SAADC register operations (parameterised by AIN channel).

use core::cell::UnsafeCell;

const SAADC_BASE: u32 = 0x4000_7000;

/// DMA result buffer for SAADC single-shot reads.
///
/// SAFETY: This buffer is only accessed within `read_single()`, which runs to
/// completion (blocking) on the single-core Cortex-M4. DMA writes are always
/// complete before the value is read (guarded by EVENTS_END busy-wait).
/// Callers of `read_single()` must ensure no concurrent SAADC access.
#[repr(C, align(4))]
struct AdcDmaBuf(UnsafeCell<i16>);

// SAFETY: Single-core Cortex-M4 — `read_single()` is blocking and runs to
// completion. No preemptive interrupt uses the SAADC peripheral.
unsafe impl Sync for AdcDmaBuf {}

static ADC_BUF: AdcDmaBuf = AdcDmaBuf(UnsafeCell::new(0));

/// Blocking single-shot SAADC read. Returns 12-bit raw value.
///
/// Configuration: Gain=1/6, Ref=Internal(0.6V), Tacq=40μs, 12-bit, single-ended.
/// Enables SAADC before sampling and disables it after.
///
/// SAFETY: Accesses nRF52840 SAADC registers via raw pointers.
/// Caller must ensure no concurrent SAADC access.
/// Blocking wait is ~tens of microseconds.
pub unsafe fn read_single(ain_channel: u8) -> i16 {
    // Enable SAADC
    core::ptr::write_volatile((SAADC_BASE + 0x500) as *mut u32, 1);

    // Channel 0: selected AIN, single-ended
    // PSELP value = AIN channel + 1 (AIN0=1, AIN1=2, ..., AIN7=8)
    core::ptr::write_volatile((SAADC_BASE + 0x510) as *mut u32, ain_channel as u32 + 1);
    core::ptr::write_volatile((SAADC_BASE + 0x514) as *mut u32, 0); // PSELN = NC

    // CONFIG: Gain=1/6, Ref=Internal(0.6V), Tacq=40us, Mode=SE
    // GAIN field (bits 8-10): 0=1/6, 1=1/5, 2=1/4, ...
    core::ptr::write_volatile(
        (SAADC_BASE + 0x518) as *mut u32,
        (0 << 8) | (0 << 12) | (5 << 16) | (0 << 20),
    );

    // Resolution 12-bit
    core::ptr::write_volatile((SAADC_BASE + 0x5F0) as *mut u32, 2);

    // Result buffer (DMA needs static address)
    core::ptr::write_volatile(
        (SAADC_BASE + 0x62C) as *mut u32,
        ADC_BUF.0.get() as u32,
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

    // Read DMA result with volatile to ensure we see what hardware wrote
    core::ptr::read_volatile(ADC_BUF.0.get())
}
