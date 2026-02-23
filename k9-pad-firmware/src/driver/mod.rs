// INPUT:  sh1107, flash
// OUTPUT: pub mod sh1107, flash; enable_i2c_pullups(), enable_oled_power()
// POS:    硬件抽象层入口，聚合芯片级驱动 + GPIO 初始化
pub mod sh1107;
pub mod flash;

/// 启用 I2C GPIO 内部上拉 (SDA=P0.08, SCL=P1.09)
///
/// SAFETY: Accesses nRF52840 GPIO PIN_CNF registers via raw pointers.
/// Must be called before I2C communication begins. No concurrent GPIO access.
pub unsafe fn enable_i2c_pullups() {
    const P0_BASE: u32 = 0x5000_0000;
    const P1_BASE: u32 = 0x5000_0300;
    const PIN_CNF_OFFSET: u32 = 0x700;

    // SDA = P0.08
    let sda_cnf_addr = (P0_BASE + PIN_CNF_OFFSET + 8 * 4) as *mut u32;
    let sda_val = core::ptr::read_volatile(sda_cnf_addr);
    core::ptr::write_volatile(sda_cnf_addr, sda_val | (3 << 2));

    // SCL = P1.09
    let scl_cnf_addr = (P1_BASE + PIN_CNF_OFFSET + 9 * 4) as *mut u32;
    let scl_val = core::ptr::read_volatile(scl_cnf_addr);
    core::ptr::write_volatile(scl_cnf_addr, scl_val | (3 << 2));

    defmt::info!("I2C pullups enabled");
}

/// 启用 OLED 电源开关 (P0.05 → High)
///
/// SAFETY: Configures P0.05 as output and sets it high.
/// No other code accesses P0.05.
pub unsafe fn enable_oled_power() {
    const P0_BASE: u32 = 0x5000_0000;
    const PIN_CNF_OFFSET: u32 = 0x700;
    const OUTSET_OFFSET: u32 = 0x508;

    let pin_cnf_addr = (P0_BASE + PIN_CNF_OFFSET + 5 * 4) as *mut u32;
    core::ptr::write_volatile(pin_cnf_addr, 0x0000_0003);

    let outset_addr = (P0_BASE + OUTSET_OFFSET) as *mut u32;
    core::ptr::write_volatile(outset_addr, 1 << 5);

    defmt::info!("OLED power enabled (P0.05)");
}
