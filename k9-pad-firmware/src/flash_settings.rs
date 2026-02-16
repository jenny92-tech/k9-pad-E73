// INPUT:  nRF52840 NVMC registers
// OUTPUT: flash_read_brightness(), flash_write_brightness(), flash_read_screen_timeout(), flash_write_screen_timeout()
// POS:    持久化设置存储（亮度、屏幕超时等），使用 Flash 末尾 4KB 页
//! Flash-based persistent settings storage
//!
//! Uses a dedicated 4KB flash page at 0xF3000 (last page before bootloader).
//! Log-structured: each setting write appends a 4-byte entry.
//! ~1024 writes per erase cycle.
//!
//! Entry format: [0x4B, 0x39, value, setting_type]
//!   setting_type = 0x00 → brightness (backward compatible with existing entries)
//!   setting_type = 0x01 → screen timeout (seconds: 5/10/20/30/60)

/// Flash page address for settings storage (last 4KB before bootloader at 0xF4000)
const SETTINGS_PAGE: u32 = 0x000F_3000;
const SETTINGS_PAGE_SIZE: usize = 4096;

/// Entry format: [magic_lo=0x4B, magic_hi=0x39, value, setting_type]
const MAGIC_LO: u8 = 0x4B; // 'K'
const MAGIC_HI: u8 = 0x39; // '9'
const ENTRY_SIZE: usize = 4;
const MAX_ENTRIES: usize = SETTINGS_PAGE_SIZE / ENTRY_SIZE;

/// Setting types
const SETTING_BRIGHTNESS: u8 = 0x00;
const SETTING_SCREEN_TIMEOUT: u8 = 0x01;

/// NVMC register addresses
const NVMC_BASE: u32 = 0x4001_E000;
const NVMC_READY: u32 = NVMC_BASE + 0x400;
const NVMC_CONFIG: u32 = NVMC_BASE + 0x504;
const NVMC_ERASEPAGE: u32 = NVMC_BASE + 0x508;

/// Wait for NVMC to become ready
///
/// SAFETY: Reads NVMC READY register. Caller must be in a context
/// where NVMC access is valid (no concurrent flash operations).
unsafe fn nvmc_wait_ready() {
    while core::ptr::read_volatile(NVMC_READY as *const u32) & 1 == 0 {}
}

/// Set NVMC mode: 0=Read, 1=Write, 2=Erase
///
/// SAFETY: Writes NVMC CONFIG register. Must not be called during
/// another flash operation.
unsafe fn nvmc_set_mode(mode: u32) {
    core::ptr::write_volatile(NVMC_CONFIG as *mut u32, mode);
    nvmc_wait_ready();
}

/// Read a setting from flash. Scans the log-structured page for the last
/// valid entry matching the given setting_type. Returns default if not found.
fn flash_read_setting(setting_type: u8, default: u8) -> u8 {
    let mut result = default;

    for i in 0..MAX_ENTRIES {
        let addr = SETTINGS_PAGE + (i * ENTRY_SIZE) as u32;
        // SAFETY: Reading from flash memory at a valid address within
        // our reserved settings page (0xF3000-0xF3FFF).
        let word = unsafe { core::ptr::read_volatile(addr as *const u32) };

        // Erased flash reads as 0xFFFFFFFF
        if word == 0xFFFF_FFFF {
            break;
        }

        let bytes = word.to_le_bytes();
        if bytes[0] == MAGIC_LO && bytes[1] == MAGIC_HI && bytes[3] == setting_type {
            result = bytes[2];
        }
    }

    result
}

/// Write a setting to flash. Appends a new entry. If page is full, erases and writes fresh.
fn flash_write_setting(setting_type: u8, val: u8) {
    // Find next free slot
    let mut free_slot: Option<usize> = None;

    for i in 0..MAX_ENTRIES {
        let addr = SETTINGS_PAGE + (i * ENTRY_SIZE) as u32;
        // SAFETY: Reading from flash memory at our reserved settings page.
        let word = unsafe { core::ptr::read_volatile(addr as *const u32) };

        if word == 0xFFFF_FFFF {
            free_slot = Some(i);
            break;
        }
    }

    // If page is full, erase it first
    if free_slot.is_none() {
        defmt::info!("Flash: settings page full, erasing");
        // SAFETY: Erasing our dedicated settings page via NVMC registers.
        // This page is reserved for settings and not used by the linker
        // (memory.x LENGTH reduced by 4K to exclude it).
        unsafe {
            nvmc_set_mode(2); // Erase mode
            core::ptr::write_volatile(NVMC_ERASEPAGE as *mut u32, SETTINGS_PAGE);
            nvmc_wait_ready();
            nvmc_set_mode(0); // Back to read mode
        }
        free_slot = Some(0);
    }

    let slot = free_slot.unwrap();
    let addr = SETTINGS_PAGE + (slot * ENTRY_SIZE) as u32;
    let entry: u32 = u32::from_le_bytes([MAGIC_LO, MAGIC_HI, val, setting_type]);

    // SAFETY: Writing a 4-byte aligned word to our reserved settings page
    // via NVMC write mode. The address is within 0xF3000-0xF3FFF.
    unsafe {
        nvmc_set_mode(1); // Write mode
        core::ptr::write_volatile(addr as *mut u32, entry);
        nvmc_wait_ready();
        nvmc_set_mode(0); // Back to read mode
    }
}

/// Read brightness from flash settings page.
/// Returns the stored brightness value, or 80 as default.
pub fn flash_read_brightness() -> u8 {
    let result = flash_read_setting(SETTING_BRIGHTNESS, 80);
    defmt::info!("Flash: read brightness = {}", result);
    result
}

/// Write brightness value to flash settings page.
pub fn flash_write_brightness(val: u8) {
    flash_write_setting(SETTING_BRIGHTNESS, val);
    defmt::info!("Flash: wrote brightness = {}", val);
}

/// Read screen timeout from flash settings page.
/// Returns the stored timeout in seconds, or 20 as default.
pub fn flash_read_screen_timeout() -> u8 {
    let result = flash_read_setting(SETTING_SCREEN_TIMEOUT, 20);
    defmt::info!("Flash: read screen timeout = {}s", result);
    result
}

/// Write screen timeout value (seconds) to flash settings page.
pub fn flash_write_screen_timeout(val: u8) {
    flash_write_setting(SETTING_SCREEN_TIMEOUT, val);
    defmt::info!("Flash: wrote screen timeout = {}s", val);
}
