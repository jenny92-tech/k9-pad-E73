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

    /// Reset WouoUI to clean entry state
    fn WouoUI_PortResetForEntry();

    /// Get selected pad index (0=Pad A, 1=Pad B, 2=Pad C)
    fn WouoUI_K9Pad_GetSelectedPad() -> u8;

    /// Set selected pad (for syncing menu state from external source)
    fn WouoUI_K9Pad_SetSelectedPad(pad: u8);

    /// Get brightness value (0-100)
    fn WouoUI_K9Pad_GetBrightness() -> u8;

    /// Get BLE enabled state (1=on, 0=off)
    fn WouoUI_K9Pad_GetBleEnabled() -> u8;

    /// Get selected user index (0=User A, 1=User B, 2=User C)
    fn WouoUI_K9Pad_GetSelectedUser() -> u8;
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
    pub fn init(&mut self) {
        if !self.initialized {
            // SAFETY: WouoUI_PortInit is a C FFI function that initializes internal
            // state. The `initialized` flag ensures this is called at most once.
            unsafe { WouoUI_PortInit() };
            self.initialized = true;
            defmt::info!("WouoUI: Initialized");
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

    /// Get the currently selected pad index (0=Pad A, 1=Pad B, 2=Pad C)
    pub fn get_selected_pad(&self) -> u8 {
        if !self.initialized {
            return 0;
        }
        // SAFETY: WouoUI_K9Pad_GetSelectedPad reads the radio button state
        // from the C menu options array. Pure read, no side effects.
        unsafe { WouoUI_K9Pad_GetSelectedPad() }
    }

    /// Set the selected pad (sync menu state from Rust side)
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
