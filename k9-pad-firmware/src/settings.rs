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

use crate::driver::{board, flash::FlashStore};

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
/// Uses flash page and magic bytes defined in board config.
pub static SETTINGS: FlashStore = FlashStore::new(
    board::SETTINGS_PAGE_ADDR,
    board::SETTINGS_PAGE_SIZE,
    board::SETTINGS_MAGIC,
);
