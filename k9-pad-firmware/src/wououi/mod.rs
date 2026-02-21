// INPUT:  WouoUI C library (extern "C" FFI)
// OUTPUT: WouoUI struct, WououiInput enum, SCREEN_WIDTH/HEIGHT
// POS:    WouoUI C 库的安全 Rust 封装，提供 init/tick/input/get_buffer
//! WouoUI FFI bindings for K9-Pad E73
//!
//! This module provides Rust bindings to the WouoUI C library for
//! animated OLED menu interfaces.

/// Screen dimensions (from WouoUI_conf.h)
pub const SCREEN_WIDTH: usize = 128;
pub const SCREEN_HEIGHT: usize = 64;

/// Input types for WouoUI (from WouoUI_port.h)
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum WououiInput {
    None = 0,
    Up = 1,
    Down = 2,
    Left = 3,
    Right = 4,
    Click = 5,
    Return = 6,
}

// FFI declarations for C functions
extern "C" {
    /// Initialize WouoUI system
    fn WouoUI_PortInit();

    /// Process one frame, returns 1 if screen was updated
    fn WouoUI_PortTick(elapsed_ms: u16) -> u8;

    /// Send input to WouoUI
    fn WouoUI_PortSendInput(input_type: u8);

    /// Check if menu is active
    #[allow(dead_code)]
    fn WouoUI_PortIsMenuActive() -> u8;

    /// Enter menu mode
    fn WouoUI_PortEnterMenu();

    /// Exit menu mode
    fn WouoUI_PortExitMenu();

    /// Get pointer to screen buffer
    fn WouoUI_PortGetBuffer() -> *mut u8;

    /// Get buffer size
    fn WouoUI_PortGetBufferSize() -> u16;

    /// Check if currently on the home page
    fn WouoUI_PortIsOnHomePage() -> u8;

    /// Configure animation timing for target frame interval
    fn WouoUI_PortConfigFrameTime(frame_ms: u16);

    /// Reset WouoUI to clean entry state
    fn WouoUI_PortResetForEntry();

    /// Get selected layer index (0=Layer 0, 1=Layer 1, 2=Layer 2)
    fn WouoUI_K9Pad_GetSelectedPad() -> u8;

    /// Set selected pad (for syncing menu state from external source)
    #[allow(dead_code)]
    fn WouoUI_K9Pad_SetSelectedPad(pad: u8);

    /// Get brightness value (0-100)
    fn WouoUI_K9Pad_GetBrightness() -> u8;

    /// Get live brightness value (0-100) — real-time slider value when ValWin is active
    fn WouoUI_K9Pad_GetLiveBrightness() -> u8;

    /// Set brightness value (0-100) — sets both settings option and ValWin
    fn WouoUI_K9Pad_SetBrightness(val: u8);

    /// Get BLE enabled state (1=on, 0=off)
    fn WouoUI_K9Pad_GetBleEnabled() -> u8;

    /// Get selected user index (0=User A, 1=User B, 2=User C)
    fn WouoUI_K9Pad_GetSelectedUser() -> u8;

    /// Check if menu exit was requested
    fn WouoUI_K9Pad_GetExitRequested() -> u8;

    /// Clear exit request flag
    fn WouoUI_K9Pad_ClearExitRequested();

    /// Check if data channel is enabled for a pad (master "Data Ch" checkbox)
    fn WouoUI_K9Pad_IsDataChannelEnabled(pad_index: u8) -> u8;

    /// Get bitmask of enabled data channel functions for a pad
    fn WouoUI_K9Pad_GetEnabledFunctions(pad_index: u8) -> u16;

    /// Check if DFU mode was requested
    fn WouoUI_K9Pad_GetDFURequested() -> u8;

    /// Clear DFU request flag
    fn WouoUI_K9Pad_ClearDFURequested();

    /// Check if USB bootloader mode was requested
    fn WouoUI_K9Pad_GetUSBBootloaderRequested() -> u8;

    /// Clear USB bootloader request flag
    fn WouoUI_K9Pad_ClearUSBBootloaderRequested();

    /// Get screen timeout in seconds (5/10/20/30/60)
    fn WouoUI_K9Pad_GetScreenTimeout() -> u8;

    /// Set screen timeout by seconds value
    fn WouoUI_K9Pad_SetScreenTimeout(seconds: u8);
}

/// Safe Rust interface to WouoUI
pub struct WouoUI {
    initialized: bool,
}

impl WouoUI {
    /// Create a new uninitialized WouoUI instance
    pub const fn new() -> Self {
        Self { initialized: false }
    }

    /// Initialize the WouoUI system (call once at startup)
    /// `frame_ms` is the target frame interval in milliseconds, used to
    /// auto-adjust blur timing for consistent page transitions.
    pub fn init(&mut self, frame_ms: u16) {
        if !self.initialized {
            // SAFETY: WouoUI_PortInit is a C FFI function that initializes internal
            // state. The `initialized` flag ensures this is called at most once.
            unsafe {
                WouoUI_PortInit();
                WouoUI_PortConfigFrameTime(frame_ms);
            };
            self.initialized = true;
            defmt::info!("WouoUI: Initialized (frame_ms={})", frame_ms);
        }
    }

    /// Process one frame tick
    /// Returns true if the screen buffer was updated
    pub fn tick(&mut self, elapsed_ms: u16) -> bool {
        if !self.initialized {
            return false;
        }
        // SAFETY: WouoUI_PortTick is a C FFI function that processes one animation
        // frame. The `initialized` check above guarantees init was called first.
        // elapsed_ms is a plain u16 value, no pointer aliasing concerns.
        unsafe { WouoUI_PortTick(elapsed_ms) != 0 }
    }

    /// Send input to WouoUI
    pub fn send_input(&mut self, input: WououiInput) {
        if !self.initialized {
            return;
        }
        // SAFETY: WouoUI_PortSendInput is a C FFI function that enqueues an input
        // event. The `initialized` check above guarantees init was called first.
        // The input value is a valid u8 from the WououiInput repr(u8) enum.
        unsafe { WouoUI_PortSendInput(input as u8) };
    }

    /// Check if menu is currently active
    #[allow(dead_code)]
    pub fn is_menu_active(&self) -> bool {
        if !self.initialized {
            return false;
        }
        // SAFETY: WouoUI_PortIsMenuActive is a pure query C FFI function with no
        // side effects. The `initialized` check above guarantees init was called.
        unsafe { WouoUI_PortIsMenuActive() != 0 }
    }

    /// Enter menu mode (resets to clean entry state)
    pub fn enter_menu(&mut self) {
        if self.initialized {
            // SAFETY: WouoUI_PortResetForEntry resets internal state to initial
            // entry animation state. WouoUI_PortEnterMenu sets the menu active flag.
            // The `initialized` check guarantees the library is ready.
            unsafe {
                WouoUI_PortResetForEntry();
                WouoUI_PortEnterMenu();
            }
            defmt::info!("WouoUI: Enter menu (reset)");
        }
    }

    /// Exit menu mode
    pub fn exit_menu(&mut self) {
        if self.initialized {
            // SAFETY: WouoUI_PortExitMenu is a C FFI function that transitions
            // internal state out of menu mode. The `initialized` check guarantees
            // the library is ready.
            unsafe { WouoUI_PortExitMenu() };
            defmt::info!("WouoUI: Exit menu");
        }
    }

    /// Check if currently on the home page
    pub fn is_on_home_page(&self) -> bool {
        if !self.initialized {
            return false;
        }
        // SAFETY: WouoUI_PortIsOnHomePage is a pure query C FFI function.
        // The `initialized` check above guarantees init was called.
        unsafe { WouoUI_PortIsOnHomePage() != 0 }
    }

    /// Get the currently selected layer index (0=Layer 0, 1=Layer 1, 2=Layer 2)
    pub fn get_selected_pad(&self) -> u8 {
        if !self.initialized {
            return 0;
        }
        // SAFETY: WouoUI_K9Pad_GetSelectedPad reads the radio button state
        // from the C menu options array. Pure read, no side effects.
        unsafe { WouoUI_K9Pad_GetSelectedPad() }
    }

    /// Set the selected pad (sync menu state from Rust side)
    #[allow(dead_code)]
    pub fn set_selected_pad(&self, pad: u8) {
        if !self.initialized {
            return;
        }
        // SAFETY: WouoUI_K9Pad_SetSelectedPad writes to the C menu options
        // array. The `initialized` check guarantees the array exists.
        unsafe { WouoUI_K9Pad_SetSelectedPad(pad) }
    }

    /// Get brightness value (0-100)
    pub fn get_brightness(&self) -> u8 {
        if !self.initialized {
            return 80;
        }
        // SAFETY: WouoUI_K9Pad_GetBrightness reads settings_option_array[2].val
        // from the C menu. Pure read, no side effects.
        unsafe { WouoUI_K9Pad_GetBrightness() }
    }

    /// Get live brightness value (0-100)
    /// Returns the real-time slider value when the brightness ValWin is active,
    /// otherwise returns the confirmed value.
    pub fn get_live_brightness(&self) -> u8 {
        if !self.initialized {
            return 80;
        }
        unsafe { WouoUI_K9Pad_GetLiveBrightness() }
    }

    /// Set brightness value (0-100) — sets both settings option and ValWin
    pub fn set_brightness(&mut self, val: u8) {
        if !self.initialized {
            return;
        }
        // SAFETY: WouoUI_K9Pad_SetBrightness writes to settings_option_array[2].val
        // and brightness_win.val. The `initialized` check guarantees these exist.
        unsafe { WouoUI_K9Pad_SetBrightness(val) }
    }

    /// Get BLE enabled state
    pub fn get_ble_enabled(&self) -> bool {
        if !self.initialized {
            return true;
        }
        // SAFETY: WouoUI_K9Pad_GetBleEnabled reads settings_option_array[1].val
        // from the C menu. Pure read, no side effects.
        unsafe { WouoUI_K9Pad_GetBleEnabled() != 0 }
    }

    /// Get selected user index (0=User A, 1=User B, 2=User C)
    pub fn get_selected_user(&self) -> u8 {
        if !self.initialized {
            return 0;
        }
        // SAFETY: WouoUI_K9Pad_GetSelectedUser iterates user_option_array
        // to find the selected radio button. Pure read, no side effects.
        unsafe { WouoUI_K9Pad_GetSelectedUser() }
    }

    /// Check and consume exit request from C callbacks
    pub fn take_exit_request(&mut self) -> bool {
        if !self.initialized {
            return false;
        }
        unsafe {
            if WouoUI_K9Pad_GetExitRequested() != 0 {
                WouoUI_K9Pad_ClearExitRequested();
                true
            } else {
                false
            }
        }
    }

    /// Check if data channel is enabled for a pad (master "Data Ch" checkbox)
    pub fn is_data_channel_enabled(&self, pad: u8) -> bool {
        if !self.initialized {
            return false;
        }
        // SAFETY: WouoUI_K9Pad_IsDataChannelEnabled reads g_pad_dc_enabled[pad].
        // Pure read, no side effects. The `initialized` check guarantees init was called.
        unsafe { WouoUI_K9Pad_IsDataChannelEnabled(pad) != 0 }
    }

    /// Get bitmask of enabled data channel functions for a pad
    /// Bit 1: Volume, Bit 2: Subs, Bit 3: Time
    pub fn get_enabled_functions(&self, pad: u8) -> u16 {
        if !self.initialized {
            return 0;
        }
        // SAFETY: WouoUI_K9Pad_GetEnabledFunctions reads option .val fields
        // from the C menu arrays. Pure read, no side effects.
        unsafe { WouoUI_K9Pad_GetEnabledFunctions(pad) }
    }

    /// Check and consume DFU mode request from C callbacks
    pub fn take_dfu_request(&mut self) -> bool {
        if !self.initialized {
            return false;
        }
        unsafe {
            if WouoUI_K9Pad_GetDFURequested() != 0 {
                WouoUI_K9Pad_ClearDFURequested();
                true
            } else {
                false
            }
        }
    }

    /// Get screen timeout in seconds (5/10/20/30/60)
    pub fn get_screen_timeout(&self) -> u8 {
        if !self.initialized {
            return 20;
        }
        // SAFETY: WouoUI_K9Pad_GetScreenTimeout reads screen_timeout_win.sel_str_index
        // and maps it to seconds. Pure read, no side effects.
        unsafe { WouoUI_K9Pad_GetScreenTimeout() }
    }

    /// Set screen timeout by seconds value (5/10/20/30/60)
    pub fn set_screen_timeout(&mut self, seconds: u8) {
        if !self.initialized {
            return;
        }
        // SAFETY: WouoUI_K9Pad_SetScreenTimeout writes to screen_timeout_win.sel_str_index
        // and settings_option_array[3].content. The `initialized` check guarantees these exist.
        unsafe { WouoUI_K9Pad_SetScreenTimeout(seconds) }
    }

    /// Check and consume USB bootloader request from C callbacks
    pub fn take_usb_bl_request(&mut self) -> bool {
        if !self.initialized {
            return false;
        }
        unsafe {
            if WouoUI_K9Pad_GetUSBBootloaderRequested() != 0 {
                WouoUI_K9Pad_ClearUSBBootloaderRequested();
                true
            } else {
                false
            }
        }
    }

    /// Get a reference to the screen buffer
    /// The buffer is in column-major format for SSD1306/SH1107 displays
    pub fn get_buffer(&self) -> Option<&[u8]> {
        if !self.initialized {
            return None;
        }
        // SAFETY: WouoUI_PortGetBuffer returns a pointer to the C library's
        // internal screen buffer, and WouoUI_PortGetBufferSize returns its size.
        // The `initialized` check above guarantees the buffer has been allocated.
        // The null check ensures we never create a slice from a null pointer.
        // The returned slice borrows `self` immutably, preventing mutation
        // of the buffer while the slice is alive.
        unsafe {
            let ptr = WouoUI_PortGetBuffer();
            let size = WouoUI_PortGetBufferSize() as usize;
            if ptr.is_null() {
                None
            } else {
                Some(core::slice::from_raw_parts(ptr, size))
            }
        }
    }
}
