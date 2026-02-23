// INPUT:  driver::flash::FlashStore
// OUTPUT: keys module, SETTINGS global instance
// POS:    应用层设置 key 定义 + 全局 FlashStore 实例

//! Application settings for K9-Pad.
//!
//! ```ignore
//! use crate::settings::{SETTINGS, keys};
//!
//! let brightness = SETTINGS.read(keys::BRIGHTNESS, 80);
//! SETTINGS.write(keys::BRIGHTNESS, new_val);
//! ```

use crate::driver::flash::FlashStore;

/// Well-known setting keys for K9-Pad.
///
/// Add new keys here as `pub const`. Indexed keys use a base + offset pattern.
pub mod keys {
    pub const BRIGHTNESS: u8 = 0x00;
    pub const SCREEN_TIMEOUT: u8 = 0x01;
    /// Per-pad data channel functions (pad 0-4 → key 0x02-0x06).
    /// Use as `DC_FUNCTIONS_PAD0 + pad_index`.
    pub const DC_FUNCTIONS_PAD0: u8 = 0x02;
    /// Quick Menu: long-press ESC enters menu directly when screen is off.
    pub const QUICK_MENU: u8 = 0x07;
}

/// Global settings store instance.
///
/// Uses flash page at 0xF3000 (4KB, last page before bootloader).
/// Magic bytes `K9` (0x4B, 0x39) identify valid entries.
pub static SETTINGS: FlashStore = FlashStore::new(0x000F_3000, 4096, [0x4B, 0x39]);
