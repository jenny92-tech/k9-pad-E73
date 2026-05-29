// INPUT:  port_base, pin 参数
// OUTPUT: set_pullup(), configure_input_pullup(), configure_output_high(), read_pin()
// POS:    通用 GPIO 寄存器操作，参数化 port/pin，被 driver/mod.rs 和 battery.rs 调用

//! Generic nRF52840 GPIO register operations (parameterised by port/pin).

const PIN_CNF_OFFSET: u32 = 0x700;
const OUTSET_OFFSET: u32 = 0x508;
const IN_OFFSET: u32 = 0x510;

/// Enable internal pull-up on a pin (modify PIN_CNF bits[3:2]).
///
/// SAFETY: Caller must ensure `port_base` is a valid GPIO port address
/// and no concurrent access to this pin's PIN_CNF register.
pub unsafe fn set_pullup(port_base: u32, pin: u8) {
    let addr = (port_base + PIN_CNF_OFFSET + pin as u32 * 4) as *mut u32;
    let val = core::ptr::read_volatile(addr);
    core::ptr::write_volatile(addr, val | (3 << 2));
}

/// Configure pin as input with pull-up (PIN_CNF = 0x0C).
///
/// DIR=Input(0), INPUT=Connect(0), PULL=Pullup(3<<2)
///
/// SAFETY: Caller must ensure `port_base` is valid and no concurrent access.
pub unsafe fn configure_input_pullup(port_base: u32, pin: u8) {
    let addr = (port_base + PIN_CNF_OFFSET + pin as u32 * 4) as *mut u32;
    core::ptr::write_volatile(addr, 0x0000_000C);
}

/// Configure pin as output and drive high (PIN_CNF = 0x03, OUTSET).
///
/// 顺序：先 OUTSET 把 OUT 锁存为高，再把 DIR 置为输出。这样使能输出缓冲的
/// 瞬间引脚直接驱动高电平；若反过来（先 DIR=输出）会先驱动 OUT 默认的低电平，
/// 在 OLED 电源等 active-high 引脚上产生一次上电毛刺。
///
/// SAFETY: Caller must ensure `port_base` is valid and no concurrent access.
pub unsafe fn configure_output_high(port_base: u32, pin: u8) {
    let outset_addr = (port_base + OUTSET_OFFSET) as *mut u32;
    core::ptr::write_volatile(outset_addr, 1 << pin);

    let addr = (port_base + PIN_CNF_OFFSET + pin as u32 * 4) as *mut u32;
    core::ptr::write_volatile(addr, 0x0000_0003);
}

/// Read pin level. Returns `true` if pin is high.
///
/// SAFETY: Caller must ensure `port_base` is valid. Read-only, no side effects.
pub unsafe fn read_pin(port_base: u32, pin: u8) -> bool {
    let state = core::ptr::read_volatile((port_base + IN_OFFSET) as *const u32);
    (state & (1 << pin)) != 0
}
