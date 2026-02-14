// INPUT:  CRC32 table (flash .rodata)
// OUTPUT: verify_firmware(), enter_dfu_mode()
// POS:    启动时 CRC32 校验固件完整性，损坏则进入 DFU 模式
//! Firmware integrity check — CRC32 verification at boot.
//!
//! Build-time: `tools/patch_crc.py` computes CRC32 of the firmware binary
//! and patches it into [`FIRMWARE_INTEGRITY`].
//!
//! Boot-time: [`verify_firmware`] recomputes CRC32 over flash and compares.
//! If mismatched (corrupted from interrupted DFU), [`enter_dfu_mode`] is called.

/// CRC32 lookup table (IEEE 802.3 polynomial 0xEDB88320).
/// Placed in `.rodata` (flash) so it's accessible from `pre_init`.
static CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i as usize] = crc;
        i += 1;
    }
    table
};

/// Firmware integrity metadata — patched by `tools/patch_crc.py`.
#[repr(C)]
pub struct FirmwareIntegrity {
    /// Magic header: 0x4B39_4352 ("K9CR")
    pub magic_head: u32,
    /// CRC32 of firmware binary (with crc32+size fields zeroed)
    pub crc32: u32,
    /// Firmware binary size in bytes
    pub size: u32,
    /// Magic trailer: 0x5243_394B ("RC9K", reverse)
    pub magic_tail: u32,
}

const MAGIC_HEAD: u32 = 0x4B39_4352;
const MAGIC_TAIL: u32 = 0x5243_394B;
const UNPATCHED: u32 = 0xFFFF_FFFF;

/// Application flash base address (after SoftDevice + Bootloader MBR)
const FLASH_BASE: usize = 0x0002_6000;

#[used]
#[no_mangle]
#[link_section = ".rodata.fw_integrity"]
pub static FIRMWARE_INTEGRITY: FirmwareIntegrity = FirmwareIntegrity {
    magic_head: MAGIC_HEAD,
    crc32: UNPATCHED,
    size: UNPATCHED,
    magic_tail: MAGIC_TAIL,
};

/// Verify firmware integrity by computing CRC32 over flash.
///
/// Returns `true` if:
/// - Firmware is intact (CRC matches), or
/// - CRC was never patched (dev build — skip check).
///
/// Returns `false` if:
/// - Magic values are wrong (struct corrupted / flash erased), or
/// - CRC mismatch (incomplete DFU write).
///
/// # Safety
/// Reads directly from flash. Must only be called during early boot (`pre_init`).
pub unsafe fn verify_firmware() -> bool {
    let meta = &FIRMWARE_INTEGRITY;

    // Magic check — if flash is erased (0xFF) or struct is corrupted, fail immediately
    if meta.magic_head != MAGIC_HEAD || meta.magic_tail != MAGIC_TAIL {
        return false;
    }

    // Unpatched dev build — skip check (CRC not embedded by post-build script)
    if meta.crc32 == UNPATCHED || meta.size == UNPATCHED {
        return true;
    }

    let fw_size = meta.size as usize;
    if fw_size == 0 || fw_size > 824 * 1024 {
        return false;
    }

    // Offset of crc32 field relative to flash base
    let crc_field_addr = &meta.crc32 as *const u32 as usize;
    let zero_start = crc_field_addr - FLASH_BASE;
    let zero_end = zero_start + 8; // crc32(4) + size(4)

    // Compute CRC32 over firmware in flash, zeroing the crc32+size fields
    let flash_ptr = FLASH_BASE as *const u8;
    let mut crc: u32 = 0xFFFF_FFFF;
    let mut i = 0usize;
    while i < fw_size {
        let byte = if i >= zero_start && i < zero_end {
            0u8
        } else {
            core::ptr::read_volatile(flash_ptr.add(i))
        };
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC32_TABLE[idx] ^ (crc >> 8);
        i += 1;
    }
    crc ^= 0xFFFF_FFFF;

    crc == meta.crc32
}

/// Enter BLE OTA DFU mode: write GPREGRET=0xA8 and system reset.
///
/// # Safety
/// Writes to hardware registers and triggers system reset. Never returns.
pub unsafe fn enter_dfu_mode() -> ! {
    // POWER.GPREGRET = 0xA8 (Adafruit Bootloader: BLE OTA DFU)
    const GPREGRET: *mut u32 = 0x4000_051C as *mut u32;
    core::ptr::write_volatile(GPREGRET, 0xA8);

    // DSB + ISB before reset (ensure GPREGRET write completes)
    cortex_m::asm::dsb();
    cortex_m::asm::isb();

    // AIRCR: VECTKEY=0x05FA | SYSRESETREQ
    const AIRCR: *mut u32 = 0xE000_ED0C as *mut u32;
    let aircr_val = core::ptr::read_volatile(AIRCR);
    core::ptr::write_volatile(AIRCR, 0x05FA_0004 | (aircr_val & 0x0000_0700));

    loop {
        core::hint::spin_loop();
    }
}
