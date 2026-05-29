// INPUT:  nRF52840 NVMC registers
// OUTPUT: FlashStore struct (new, read, write)
// POS:    通用 log-structured flash KV 驱动，直接操作 NVMC 寄存器

//! Log-structured flash key-value store for nRF52840.
//!
//! Stores `u8` key-value pairs on a single flash page using append-only log.
//! Each write appends a 4-byte entry; page auto-erases when full (~1024 writes per cycle).

/// Log-structured flash key-value store for nRF52840 NVMC.
///
/// Entry format: `[magic[0], magic[1], value, key]` — 4 bytes per entry.
/// Reads scan the entire log and return the **last** match (newest value).
/// Writes append to the next free slot; if full, the page is erased.
///
/// The struct itself is just configuration (page address, size, magic).
/// All mutable state lives on the flash page.
pub struct FlashStore {
    page_addr: u32,
    page_size: usize,
    magic: [u8; 2],
}

impl FlashStore {
    /// Create a new store backed by a flash page.
    ///
    /// - `page_addr`: start address of the flash page (must be page-aligned)
    /// - `page_size`: size of the flash page in bytes (typically 4096)
    /// - `magic`: 2-byte magic header to identify valid entries
    pub const fn new(page_addr: u32, page_size: usize, magic: [u8; 2]) -> Self {
        Self {
            page_addr,
            page_size,
            magic,
        }
    }

    /// Maximum number of entries this page can hold.
    const fn max_entries(&self) -> usize {
        self.page_size / 4
    }

    /// Read the latest value for `key`. Returns `default` if not found.
    ///
    /// Scans the log from start to end, returning the last matching entry.
    /// Flash reads are fast (~1 cycle each), so scanning 1024 entries ≈ 50μs.
    pub fn read(&self, key: u8, default: u8) -> u8 {
        let mut result = default;
        let max = self.max_entries();

        for i in 0..max {
            let addr = self.page_addr + (i * 4) as u32;
            // SAFETY: Reading from flash memory at a valid address within
            // our reserved settings page.
            let word = unsafe { core::ptr::read_volatile(addr as *const u32) };

            // Erased flash reads as 0xFFFFFFFF — end of log
            if word == 0xFFFF_FFFF {
                break;
            }

            let bytes = word.to_le_bytes();
            if bytes[0] == self.magic[0] && bytes[1] == self.magic[1] && bytes[3] == key {
                result = bytes[2];
            }
        }

        result
    }

    /// Write a value for `key`. Skips if unchanged; appends to log; compacts if full.
    ///
    /// Safety mechanisms:
    /// 1. **Read-before-write**: reads current flash value first, skips if identical
    /// 2. **Compaction on erase**: when page is full, collects all latest values,
    ///    erases, then rewrites them (no data loss)
    ///
    /// Timing: ~100μs normal write, ~85ms + N×100μs if compaction triggers (rare).
    /// During NVMC write/erase, CPU stalls on flash access.
    pub fn write(&self, key: u8, val: u8) {
        // Guard 1: skip if flash already holds this exact value
        if self.read(key, val.wrapping_add(1)) == val {
            return;
        }

        let max = self.max_entries();

        // Find next free slot
        let mut free_slot: Option<usize> = None;
        for i in 0..max {
            let addr = self.page_addr + (i * 4) as u32;
            // SAFETY: Reading from flash at our reserved settings page.
            let word = unsafe { core::ptr::read_volatile(addr as *const u32) };

            if word == 0xFFFF_FFFF {
                free_slot = Some(i);
                break;
            }
        }

        // Page full → compact: collect all latest values, erase, rewrite
        if free_slot.is_none() {
            defmt::info!("FlashStore: page full, compacting 0x{:08x}", self.page_addr);

            // Collect latest value for each key (scan entire log)
            // Max 256 possible keys (u8), use fixed buffer
            let mut snapshot: [Option<u8>; 256] = [None; 256];
            for i in 0..max {
                let addr = self.page_addr + (i * 4) as u32;
                // SAFETY: Reading from flash at our reserved settings page.
                let word = unsafe { core::ptr::read_volatile(addr as *const u32) };
                if word == 0xFFFF_FFFF {
                    break;
                }
                let bytes = word.to_le_bytes();
                if bytes[0] == self.magic[0] && bytes[1] == self.magic[1] {
                    snapshot[bytes[3] as usize] = Some(bytes[2]);
                }
            }

            // Include the new value being written
            snapshot[key as usize] = Some(val);

            // Erase page
            // SAFETY: Erasing our dedicated settings page via NVMC registers.
            // This page is reserved for settings and not used by the linker.
            unsafe {
                nvmc_set_mode(2); // Erase mode
                core::ptr::write_volatile(NVMC_ERASEPAGE as *mut u32, self.page_addr);
                nvmc_wait_ready();
                nvmc_set_mode(0); // Back to read mode
            }

            // Rewrite all collected values
            let mut slot = 0usize;
            // SAFETY: Writing to freshly erased flash page via NVMC write mode.
            unsafe {
                nvmc_set_mode(1); // Write mode
                for (k, v) in snapshot.iter().enumerate() {
                    if let Some(value) = v {
                        let addr = self.page_addr + (slot * 4) as u32;
                        let entry = u32::from_le_bytes([
                            self.magic[0], self.magic[1], *value, k as u8,
                        ]);
                        core::ptr::write_volatile(addr as *mut u32, entry);
                        nvmc_wait_ready();
                        slot += 1;
                    }
                }
                nvmc_set_mode(0); // Back to read mode
            }

            defmt::info!("FlashStore: compacted {} entries", slot);
            return; // Already wrote the new value during compaction
        }

        let slot = free_slot.unwrap();
        let addr = self.page_addr + (slot * 4) as u32;
        let entry = u32::from_le_bytes([self.magic[0], self.magic[1], val, key]);

        // SAFETY: Writing a 4-byte aligned word to our reserved settings page
        // via NVMC write mode.
        unsafe {
            nvmc_set_mode(1); // Write mode
            core::ptr::write_volatile(addr as *mut u32, entry);
            nvmc_wait_ready();
            nvmc_set_mode(0); // Back to read mode

            // Read-back verification: detect flash write failures (rare but possible
            // on aged flash or if power is interrupted during write)
            let readback = core::ptr::read_volatile(addr as *const u32);
            if readback != entry {
                defmt::error!(
                    "FlashStore: write verify failed at 0x{:08x} (wrote 0x{:08x}, read 0x{:08x})",
                    addr, entry, readback
                );
            }
        }
    }

    /// 擦除整个设置页：所有键回到 `read` 的 default 值。
    /// 用于菜单"重置 App 设置 / 全部删除"。擦除是阻塞操作(~85ms)。
    pub fn erase(&self) {
        // SAFETY: Erasing our dedicated settings page via NVMC registers.
        // This page is reserved for settings and not used by the linker.
        unsafe {
            nvmc_set_mode(2); // Erase mode
            core::ptr::write_volatile(NVMC_ERASEPAGE as *mut u32, self.page_addr);
            nvmc_wait_ready();
            nvmc_set_mode(0); // Back to read mode
        }
    }
}

// ============== NVMC Low-Level ==============

/// NVMC register addresses (nRF52840)
const NVMC_BASE: u32 = 0x4001_E000;
const NVMC_READY: u32 = NVMC_BASE + 0x400;
const NVMC_CONFIG: u32 = NVMC_BASE + 0x504;
const NVMC_ERASEPAGE: u32 = NVMC_BASE + 0x508;

/// Wait for NVMC to become ready.
///
/// **Note**: This busy-waits the CPU. On nRF52840, during NVMC write/erase
/// operations the CPU stalls on any flash access — this is a hardware
/// limitation and cannot be made asynchronous without executing wait code
/// from RAM. Normal writes complete in ~100μs. Page erase takes ~85ms
/// but only occurs during compaction (when the settings page is full,
/// typically after ~1024 writes).
///
/// SAFETY: Reads NVMC READY register. Caller must be in a context
/// where NVMC access is valid (no concurrent flash operations).
unsafe fn nvmc_wait_ready() {
    while core::ptr::read_volatile(NVMC_READY as *const u32) & 1 == 0 {}
}

/// Set NVMC mode: 0=Read, 1=Write, 2=Erase.
///
/// SAFETY: Writes NVMC CONFIG register. Must not be called during
/// another flash operation.
unsafe fn nvmc_set_mode(mode: u32) {
    core::ptr::write_volatile(NVMC_CONFIG as *mut u32, mode);
    nvmc_wait_ready();
}
