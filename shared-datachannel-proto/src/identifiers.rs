// INPUT:  (no_std core only)
// OUTPUT: K9-Pad device identifiers — USB VID/PID, usage page, BLE UUIDs
// POS:    Single source of truth for transport-level device identifiers shared by firmware and host

/// K9-Pad USB Vendor ID.
pub const K9_USB_VID: u16 = 0x4C4B;

/// K9-Pad USB Product ID.
pub const K9_USB_PID: u16 = 0x4643;

/// Vendor-specific HID usage page for the data channel interface (0xFF61).
/// Distinguishes the data channel from the standard HID keyboard and Via (0xFF60) interfaces.
pub const DATA_CHANNEL_USAGE_PAGE: u16 = 0xFF61;

/// Custom BLE GATT service UUID for K9-Pad data channel.
///
/// UUID: `e9dc0001-7374-7265-616d-6b3970616400`
pub const BLE_SERVICE_UUID: u128 = 0xe9dc0001_7374_7265_616d_6b3970616400;

/// BLE characteristic UUID for host -> device writes (RX from device perspective).
///
/// UUID: `e9dc0002-7374-7265-616d-6b3970616400`
pub const BLE_RX_CHAR_UUID: u128 = 0xe9dc0002_7374_7265_616d_6b3970616400;

/// BLE characteristic UUID for device -> host notifications (TX from device perspective).
///
/// UUID: `e9dc0003-7374-7265-616d-6b3970616400`
pub const BLE_TX_CHAR_UUID: u128 = 0xe9dc0003_7374_7265_616d_6b3970616400;
