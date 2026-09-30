// INPUT:  (no_std core only)
// OUTPUT: CommandId, DataType, DialogResultCode, Packet, parse/serialize API, device identifiers
// POS:    数据通道协议 crate，固件和主机共用
#![cfg_attr(not(test), no_std)]

pub mod identifiers;

/// Maximum total packet size (header + payload), aligned with BLE characteristic size and USB CDC.
pub const MAX_PACKET_SIZE: usize = 64;

/// 协议版本。开发测试阶段：始终为 1，不做版本演进/兼容层。
pub const PROTOCOL_VERSION: u8 = 1;

/// Header size: CMD(1) + TYPE(1) + LEN(2).
pub const HEADER_SIZE: usize = 4;

/// Maximum payload size per packet.
pub const MAX_PAYLOAD_SIZE: usize = MAX_PACKET_SIZE - HEADER_SIZE;

// ---------------------------------------------------------------------------
// Command IDs
// ---------------------------------------------------------------------------

/// Command byte in the packet header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandId {
    /// Host -> Keyboard: push display data.
    SetDisplay = 0x01,
    /// Host -> Keyboard: request keyboard status.
    GetStatus = 0x02,
    /// Keyboard -> Host: status response (current pad, enabled functions).
    StatusResp = 0x03,
    /// Keyboard -> Host: user changed config in menu.
    ConfigChanged = 0x04,
    /// Keyboard -> Host: acknowledgement.
    Ack = 0x05,
    /// Host -> Keyboard: request device capabilities.
    GetCapabilities = 0x06,
    /// Keyboard -> Host: capabilities response.
    CapabilitiesResp = 0x07,
    /// Bidirectional heartbeat.
    Ping = 0x10,
    /// Bidirectional heartbeat response.
    Pong = 0x11,
    /// Host -> Keyboard: show a confirm/cancel dialog with text on the device screen.
    ShowDialog = 0x20,
    /// Keyboard -> Host: user's choice on a dialog (confirm / cancel / timeout).
    DialogResult = 0x21,
    /// Host -> Keyboard: layout config (rows, cols, status bar on/off) for the
    /// generic component grid.
    LayoutCfg = 0x22,
    /// Host -> Keyboard: declare a display component (id, kind, label, row, col).
    CompLayout = 0x23,
    /// Host -> Keyboard: update a display component's value (id + kind-dependent value).
    CompSet = 0x24,
    /// Host -> Keyboard: declare one option of a dialog (id + index + label).
    DialogOption = 0x25,
}

impl CommandId {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::SetDisplay),
            0x02 => Some(Self::GetStatus),
            0x03 => Some(Self::StatusResp),
            0x04 => Some(Self::ConfigChanged),
            0x05 => Some(Self::Ack),
            0x06 => Some(Self::GetCapabilities),
            0x07 => Some(Self::CapabilitiesResp),
            0x10 => Some(Self::Ping),
            0x11 => Some(Self::Pong),
            0x20 => Some(Self::ShowDialog),
            0x21 => Some(Self::DialogResult),
            0x22 => Some(Self::LayoutCfg),
            0x23 => Some(Self::CompLayout),
            0x24 => Some(Self::CompSet),
            0x25 => Some(Self::DialogOption),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Data Types (used in TYPE field)
// ---------------------------------------------------------------------------

/// Data type byte — semantics depend on the command.
///
/// For `SetDisplay`: describes what kind of display data is in the payload.
/// For `ConfigChanged` / `StatusResp`: describes config payload format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataType {
    // -- Display data types (used with SetDisplay) --
    /// `slot_id(1B) + UTF-8 string`
    Text = 0x01,
    /// `slot_id(1B) + i32 LE`
    Numeric = 0x02,
    /// `slot_id(1B) + u8(0-100)`
    Progress = 0x03,
    /// `slot_id(1B) + u16 LE`
    IconId = 0x04,
    /// `slot_id(1B) + key_len(1B) + key + value`
    KeyValue = 0x05,
    /// `slot_id(1B)` — clear the specified slot
    Clear = 0x06,

    // -- 组件值类型（COMP_SET，按组件 kind 选择）--
    /// `id(1B) + flags(1B) + u8(0-100)` — Percentage 组件值
    Percentage = 0x12,
    /// `id(1B) + flags(1B) + u8(0/1)` — Checkbox 组件值
    Checkbox = 0x13,

    // -- Config types (used with ConfigChanged / StatusResp) --
    /// `active_pad(1B) + enabled_functions_bitmask(2B LE)`
    PadConfig = 0x10,

    // -- Device info types (used with CapabilitiesResp) --
    /// `DeviceCapabilities` struct (10 bytes)
    DeviceInfo = 0x11,
}

impl DataType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Text),
            0x02 => Some(Self::Numeric),
            0x03 => Some(Self::Progress),
            0x04 => Some(Self::IconId),
            0x05 => Some(Self::KeyValue),
            0x06 => Some(Self::Clear),
            0x12 => Some(Self::Percentage),
            0x13 => Some(Self::Checkbox),
            0x10 => Some(Self::PadConfig),
            0x11 => Some(Self::DeviceInfo),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Component kinds (generic grid component types, used with COMP_LAYOUT)
// ---------------------------------------------------------------------------

/// 通用显示组件类型：设备按 kind 决定怎么把值渲染进自己的格子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompKind {
    /// 文本（UTF-8）
    Text = 0x01,
    /// 数字（i32 LE）
    Numeric = 0x02,
    /// 进度条（u8 0-100）
    Progress = 0x03,
    /// 百分比（u8 0-100，渲染为 "NN%"）
    Percentage = 0x04,
    /// 勾选框（u8 0/1）
    Checkbox = 0x05,
    /// 图标（u16 LE）
    Icon = 0x06,
}

impl CompKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Text),
            0x02 => Some(Self::Numeric),
            0x03 => Some(Self::Progress),
            0x04 => Some(Self::Percentage),
            0x05 => Some(Self::Checkbox),
            0x06 => Some(Self::Icon),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Packet Header
// ---------------------------------------------------------------------------

/// 4-byte packet header: `| CMD (1B) | TYPE (1B) | LEN (2B LE) |`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketHeader {
    pub cmd: CommandId,
    pub data_type: DataType,
    /// Payload length (not including the header).
    pub payload_len: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    BufferTooShort,
    UnknownCommand(u8),
    UnknownDataType(u8),
    PayloadTooLarge,
}

impl PacketHeader {
    /// Encode the header into the first 4 bytes of `buf`.
    /// Returns `HEADER_SIZE` on success, or `None` if `buf` is too small.
    pub fn encode(&self, buf: &mut [u8]) -> Option<usize> {
        if buf.len() < HEADER_SIZE {
            return None;
        }
        buf[0] = self.cmd as u8;
        buf[1] = self.data_type as u8;
        buf[2] = (self.payload_len & 0xFF) as u8;
        buf[3] = ((self.payload_len >> 8) & 0xFF) as u8;
        Some(HEADER_SIZE)
    }

    /// Decode a header from the first 4 bytes of `buf`.
    pub fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        if buf.len() < HEADER_SIZE {
            return Err(DecodeError::BufferTooShort);
        }
        let cmd = CommandId::from_u8(buf[0]).ok_or(DecodeError::UnknownCommand(buf[0]))?;
        let data_type = DataType::from_u8(buf[1]).ok_or(DecodeError::UnknownDataType(buf[1]))?;
        let payload_len = u16::from_le_bytes([buf[2], buf[3]]);
        if payload_len as usize > MAX_PAYLOAD_SIZE {
            return Err(DecodeError::PayloadTooLarge);
        }
        Ok(Self {
            cmd,
            data_type,
            payload_len,
        })
    }
}

// ---------------------------------------------------------------------------
// Pad Configuration
// ---------------------------------------------------------------------------

/// Configuration payload for `ConfigChanged` / `StatusResp` with `DataType::PadConfig`.
///
/// Wire format: `active_pad(1B) + enabled_functions(2B LE)` = 3 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PadConfig {
    /// Currently active pad index (0 = Pad A, 1 = Pad B, 2 = Pad C).
    pub active_pad: u8,
    /// Bitmask of enabled display functions.
    ///
    /// Deprecated semantics: display is push-driven (the device shows whatever
    /// slots the host pushes). Kept for wire compatibility only — firmware
    /// always reports `0xFFFF` and hosts must ignore this field.
    pub enabled_functions: u16,
}

/// Bit positions within `PadConfig::enabled_functions`.
///
/// Deprecated: no longer used for any decision (firmware reports `0xFFFF`,
/// hosts ignore the field). Retained only to document the legacy wire format.
pub mod function_bits {
    pub const FOLLOW_PC: u16 = 1 << 0;
    pub const VOLUME: u16 = 1 << 1;
    pub const SUBSCRIBERS: u16 = 1 << 2;
    pub const TIME: u16 = 1 << 3;
    pub const AI_QUOTA: u16 = 1 << 4;
}

impl PadConfig {
    pub const WIRE_SIZE: usize = 3;

    pub fn encode(&self, buf: &mut [u8]) -> Option<usize> {
        if buf.len() < Self::WIRE_SIZE {
            return None;
        }
        buf[0] = self.active_pad;
        buf[1] = (self.enabled_functions & 0xFF) as u8;
        buf[2] = ((self.enabled_functions >> 8) & 0xFF) as u8;
        Some(Self::WIRE_SIZE)
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::WIRE_SIZE {
            return None;
        }
        Some(Self {
            active_pad: buf[0],
            enabled_functions: u16::from_le_bytes([buf[1], buf[2]]),
        })
    }
}

// ---------------------------------------------------------------------------
// Dialog (ShowDialog / DialogResult)
// ---------------------------------------------------------------------------

/// Result code carried by `DialogResult` packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DialogResultCode {
    /// User pressed the confirm button.
    Confirm = 0,
    /// User pressed cancel (or backed out of the dialog).
    Cancel = 1,
    /// Dialog timed out with no user input.
    Timeout = 2,
}

impl DialogResultCode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Confirm),
            1 => Some(Self::Cancel),
            2 => Some(Self::Timeout),
            _ => None,
        }
    }
}

/// 弹窗类型：设备按 kind 决定呈现方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DialogKind {
    /// 授权：两个选项（Allow/Deny），确认/取消键快速响应
    ConfirmCancel = 0x01,
    /// AI 选择：3-4 个选项，滚轮选择 + 数字键直选
    Choice = 0x02,
}

impl DialogKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::ConfirmCancel),
            0x02 => Some(Self::Choice),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Device Capabilities
// ---------------------------------------------------------------------------

/// Device capability descriptor returned by `CapabilitiesResp`.
///
/// Wire format (4 bytes):
/// `protocol_version(1B) + firmware_major(1B) + firmware_minor(1B) + firmware_patch(1B)`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCapabilities {
    /// Protocol revision (starts at 1).
    pub protocol_version: u8,
    /// Firmware major version.
    pub firmware_major: u8,
    /// Firmware minor version.
    pub firmware_minor: u8,
    /// Firmware patch version.
    pub firmware_patch: u8,
}

impl DeviceCapabilities {
    pub const WIRE_SIZE: usize = 4;

    pub fn encode(&self, buf: &mut [u8]) -> Option<usize> {
        if buf.len() < Self::WIRE_SIZE {
            return None;
        }
        buf[0] = self.protocol_version;
        buf[1] = self.firmware_major;
        buf[2] = self.firmware_minor;
        buf[3] = self.firmware_patch;
        Some(Self::WIRE_SIZE)
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::WIRE_SIZE {
            return None;
        }
        Some(Self {
            protocol_version: buf[0],
            firmware_major: buf[1],
            firmware_minor: buf[2],
            firmware_patch: buf[3],
        })
    }
}

// ---------------------------------------------------------------------------
// Packet builder helpers
// ---------------------------------------------------------------------------

/// Build a complete packet in `buf`. Returns total bytes written (header + payload).
pub fn build_packet(
    buf: &mut [u8],
    cmd: CommandId,
    data_type: DataType,
    payload: &[u8],
) -> Option<usize> {
    if payload.len() > MAX_PAYLOAD_SIZE || buf.len() < HEADER_SIZE + payload.len() {
        return None;
    }
    let header = PacketHeader {
        cmd,
        data_type,
        payload_len: payload.len() as u16,
    };
    header.encode(buf)?;
    buf[HEADER_SIZE..HEADER_SIZE + payload.len()].copy_from_slice(payload);
    Some(HEADER_SIZE + payload.len())
}

/// Build a `PING` packet (no meaningful payload).
pub fn build_ping(buf: &mut [u8]) -> Option<usize> {
    build_packet(buf, CommandId::Ping, DataType::Text, &[])
}

/// Build a `PONG` packet.
pub fn build_pong(buf: &mut [u8]) -> Option<usize> {
    build_packet(buf, CommandId::Pong, DataType::Text, &[])
}

// ---------------------------------------------------------------------------
// Generic component layout commands (LAYOUT_CFG / COMP_LAYOUT / COMP_SET)
// ---------------------------------------------------------------------------

/// Build a `LAYOUT_CFG` packet: `rows(1B) + cols(1B) + show_status(1B)`.
pub fn build_layout_cfg(buf: &mut [u8], rows: u8, cols: u8, show_status: bool) -> Option<usize> {
    let payload = [rows, cols, show_status as u8];
    build_packet(buf, CommandId::LayoutCfg, DataType::Text, &payload)
}

/// Build a `COMP_LAYOUT` packet:
/// `id(1B) + row(1B) + col(1B) + kind(1B) + label_len(1B) + label`.
pub fn build_comp_layout(
    buf: &mut [u8],
    id: u8,
    row: u8,
    col: u8,
    kind: CompKind,
    label: &str,
) -> Option<usize> {
    let label_bytes = label.as_bytes();
    if label_bytes.len() > 16 || 5 + label_bytes.len() > MAX_PAYLOAD_SIZE {
        return None;
    }
    let mut payload = [0u8; MAX_PAYLOAD_SIZE];
    payload[0] = id;
    payload[1] = row;
    payload[2] = col;
    payload[3] = kind as u8;
    payload[4] = label_bytes.len() as u8;
    payload[5..5 + label_bytes.len()].copy_from_slice(label_bytes);
    build_packet(
        buf,
        CommandId::CompLayout,
        DataType::Text,
        &payload[..5 + label_bytes.len()],
    )
}

/// Build a `COMP_SET` packet for a text component: `id(1B) + flags(1B) + UTF-8`.
/// `wake=true` 时设备屏幕关着也会唤醒显示（新通知）；`wake=false` 只更新不唤醒（时钟跳动）。
pub fn build_comp_set_text(buf: &mut [u8], id: u8, wake: bool, text: &str) -> Option<usize> {
    let text_bytes = text.as_bytes();
    if 2 + text_bytes.len() > MAX_PAYLOAD_SIZE {
        return None;
    }
    let mut payload = [0u8; MAX_PAYLOAD_SIZE];
    payload[0] = id;
    payload[1] = wake as u8;
    payload[2..2 + text_bytes.len()].copy_from_slice(text_bytes);
    build_packet(
        buf,
        CommandId::CompSet,
        DataType::Text,
        &payload[..2 + text_bytes.len()],
    )
}

/// Build a `COMP_SET` packet for a numeric component: `id(1B) + flags(1B) + i32 LE`.
pub fn build_comp_set_numeric(buf: &mut [u8], id: u8, wake: bool, value: i32) -> Option<usize> {
    let mut payload = [0u8; 6];
    payload[0] = id;
    payload[1] = wake as u8;
    payload[2..6].copy_from_slice(&value.to_le_bytes());
    build_packet(buf, CommandId::CompSet, DataType::Numeric, &payload)
}

/// Build a `COMP_SET` packet for a progress component: `id(1B) + flags(1B) + u8`.
pub fn build_comp_set_u8(buf: &mut [u8], id: u8, wake: bool, value: u8) -> Option<usize> {
    build_packet(buf, CommandId::CompSet, DataType::Progress, &[id, wake as u8, value])
}

/// Build a `COMP_SET` packet for a percentage component: `id(1B) + flags(1B) + u8`.
pub fn build_comp_set_percentage(buf: &mut [u8], id: u8, wake: bool, value: u8) -> Option<usize> {
    build_packet(buf, CommandId::CompSet, DataType::Percentage, &[id, wake as u8, value])
}

/// Build a `COMP_SET` packet for a checkbox component: `id(1B) + flags(1B) + u8(0/1)`.
pub fn build_comp_set_checkbox(buf: &mut [u8], id: u8, wake: bool, on: bool) -> Option<usize> {
    build_packet(buf, CommandId::CompSet, DataType::Checkbox, &[id, wake as u8, on as u8])
}

/// Build a `COMP_SET` packet for an icon component: `id(1B) + flags(1B) + u16 LE`.
pub fn build_comp_set_icon(buf: &mut [u8], id: u8, wake: bool, icon_id: u16) -> Option<usize> {
    let mut payload = [0u8; 4];
    payload[0] = id;
    payload[1] = wake as u8;
    payload[2..4].copy_from_slice(&icon_id.to_le_bytes());
    build_packet(buf, CommandId::CompSet, DataType::IconId, &payload)
}

/// Build a `CONFIG_CHANGED` packet from a `PadConfig`.
pub fn build_config_changed(buf: &mut [u8], config: &PadConfig) -> Option<usize> {
    let mut payload = [0u8; PadConfig::WIRE_SIZE];
    config.encode(&mut payload)?;
    build_packet(buf, CommandId::ConfigChanged, DataType::PadConfig, &payload)
}

/// Build a `STATUS_RESP` packet from a `PadConfig`.
pub fn build_status_resp(buf: &mut [u8], config: &PadConfig) -> Option<usize> {
    let mut payload = [0u8; PadConfig::WIRE_SIZE];
    config.encode(&mut payload)?;
    build_packet(buf, CommandId::StatusResp, DataType::PadConfig, &payload)
}

/// Build a `SET_DISPLAY` text packet: `slot_id(1B) + UTF-8 string`.
pub fn build_set_text(buf: &mut [u8], slot: u8, text: &str) -> Option<usize> {
    let text_bytes = text.as_bytes();
    if 1 + text_bytes.len() > MAX_PAYLOAD_SIZE {
        return None;
    }
    let mut payload = [0u8; MAX_PAYLOAD_SIZE];
    payload[0] = slot;
    payload[1..1 + text_bytes.len()].copy_from_slice(text_bytes);
    build_packet(
        buf,
        CommandId::SetDisplay,
        DataType::Text,
        &payload[..1 + text_bytes.len()],
    )
}

/// Build a `SET_DISPLAY` numeric packet: `slot_id(1B) + i32 LE`.
pub fn build_set_numeric(buf: &mut [u8], slot: u8, value: i32) -> Option<usize> {
    let mut payload = [0u8; 5];
    payload[0] = slot;
    payload[1..5].copy_from_slice(&value.to_le_bytes());
    build_packet(buf, CommandId::SetDisplay, DataType::Numeric, &payload)
}

/// Build a `SET_DISPLAY` progress packet: `slot_id(1B) + u8(0-100)`.
pub fn build_set_progress(buf: &mut [u8], slot: u8, value: u8) -> Option<usize> {
    let payload = [slot, value.min(100)];
    build_packet(buf, CommandId::SetDisplay, DataType::Progress, &payload)
}

/// Build a `SET_DISPLAY` clear packet: `slot_id(1B)`.
pub fn build_set_clear(buf: &mut [u8], slot: u8) -> Option<usize> {
    build_packet(buf, CommandId::SetDisplay, DataType::Clear, &[slot])
}

/// Build a `GET_CAPABILITIES` request packet (no payload).
pub fn build_get_capabilities(buf: &mut [u8]) -> Option<usize> {
    build_packet(buf, CommandId::GetCapabilities, DataType::DeviceInfo, &[])
}

/// Build a `CAPABILITIES_RESP` packet from a `DeviceCapabilities`.
pub fn build_capabilities_resp(buf: &mut [u8], caps: &DeviceCapabilities) -> Option<usize> {
    let mut payload = [0u8; DeviceCapabilities::WIRE_SIZE];
    caps.encode(&mut payload)?;
    build_packet(
        buf,
        CommandId::CapabilitiesResp,
        DataType::DeviceInfo,
        &payload,
    )
}

/// Build an `ACK` packet.
pub fn build_ack(buf: &mut [u8]) -> Option<usize> {
    build_packet(buf, CommandId::Ack, DataType::Text, &[])
}

/// Build a `SHOW_DIALOG` packet: `dialog_id(1B) + flags(1B, reserved=0) + UTF-8 text`.
///
/// The device shows the text in a confirm/cancel dialog. Text is ASCII-only on
/// current firmware (no CJK font) and must fit `MAX_PAYLOAD_SIZE - 2` bytes.
/// Build a `SHOW_DIALOG` packet: `dialog_id(1B) + kind(1B) + UTF-8 title`.
pub fn build_show_dialog(buf: &mut [u8], dialog_id: u8, kind: DialogKind, title: &str) -> Option<usize> {
    let text_bytes = title.as_bytes();
    if 2 + text_bytes.len() > MAX_PAYLOAD_SIZE {
        return None;
    }
    let mut payload = [0u8; MAX_PAYLOAD_SIZE];
    payload[0] = dialog_id;
    payload[1] = kind as u8;
    payload[2..2 + text_bytes.len()].copy_from_slice(text_bytes);
    build_packet(
        buf,
        CommandId::ShowDialog,
        DataType::Text,
        &payload[..2 + text_bytes.len()],
    )
}

/// Build a `DIALOG_OPTION` packet: `dialog_id(1B) + index(1B) + UTF-8 label`.
pub fn build_dialog_option(buf: &mut [u8], dialog_id: u8, index: u8, label: &str) -> Option<usize> {
    let text_bytes = label.as_bytes();
    if 2 + text_bytes.len() > MAX_PAYLOAD_SIZE {
        return None;
    }
    let mut payload = [0u8; MAX_PAYLOAD_SIZE];
    payload[0] = dialog_id;
    payload[1] = index;
    payload[2..2 + text_bytes.len()].copy_from_slice(text_bytes);
    build_packet(
        buf,
        CommandId::DialogOption,
        DataType::Text,
        &payload[..2 + text_bytes.len()],
    )
}

/// Build a `DIALOG_RESULT` packet: `dialog_id(1B) + selection(1B) + code(1B)`.
/// `selection` = 选中的选项 index（0-based；Cancel/Timeout 时无意义）。
pub fn build_dialog_result(
    buf: &mut [u8],
    dialog_id: u8,
    selection: u8,
    result: DialogResultCode,
) -> Option<usize> {
    let payload = [dialog_id, selection, result as u8];
    build_packet(buf, CommandId::DialogResult, DataType::Text, &payload)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let header = PacketHeader {
            cmd: CommandId::SetDisplay,
            data_type: DataType::Text,
            payload_len: 42,
        };
        let mut buf = [0u8; 64];
        assert_eq!(header.encode(&mut buf), Some(HEADER_SIZE));
        let decoded = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header, decoded);
    }

    #[test]
    fn header_decode_too_short() {
        let buf = [0u8; 3];
        assert_eq!(PacketHeader::decode(&buf), Err(DecodeError::BufferTooShort));
    }

    #[test]
    fn header_decode_unknown_cmd() {
        let buf = [0xFF, 0x01, 0x00, 0x00];
        assert_eq!(
            PacketHeader::decode(&buf),
            Err(DecodeError::UnknownCommand(0xFF))
        );
    }

    #[test]
    fn header_decode_payload_too_large() {
        // payload_len = 0xFFFF > MAX_PAYLOAD_SIZE
        let buf = [0x01, 0x01, 0xFF, 0xFF];
        assert_eq!(
            PacketHeader::decode(&buf),
            Err(DecodeError::PayloadTooLarge)
        );
    }

    #[test]
    fn pad_config_round_trip() {
        let config = PadConfig {
            active_pad: 2,
            enabled_functions: function_bits::VOLUME | function_bits::TIME,
        };
        let mut buf = [0u8; 3];
        assert_eq!(config.encode(&mut buf), Some(3));
        let decoded = PadConfig::decode(&buf).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn build_text_packet() {
        let mut buf = [0u8; 64];
        let n = build_set_text(&mut buf, 0, "Hello").unwrap();
        assert_eq!(n, HEADER_SIZE + 1 + 5); // header + slot + "Hello"

        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::SetDisplay);
        assert_eq!(header.data_type, DataType::Text);
        assert_eq!(header.payload_len, 6); // slot(1) + text(5)
        assert_eq!(buf[HEADER_SIZE], 0); // slot_id
        assert_eq!(&buf[HEADER_SIZE + 1..HEADER_SIZE + 6], b"Hello");
    }

    #[test]
    fn build_numeric_packet() {
        let mut buf = [0u8; 64];
        let n = build_set_numeric(&mut buf, 1, -12345).unwrap();
        assert_eq!(n, HEADER_SIZE + 5);

        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::SetDisplay);
        assert_eq!(header.data_type, DataType::Numeric);
        assert_eq!(buf[HEADER_SIZE], 1); // slot_id
        let value = i32::from_le_bytes([
            buf[HEADER_SIZE + 1],
            buf[HEADER_SIZE + 2],
            buf[HEADER_SIZE + 3],
            buf[HEADER_SIZE + 4],
        ]);
        assert_eq!(value, -12345);
    }

    #[test]
    fn build_progress_clamps() {
        let mut buf = [0u8; 64];
        let n = build_set_progress(&mut buf, 0, 200).unwrap();
        assert_eq!(n, HEADER_SIZE + 2);
        assert_eq!(buf[HEADER_SIZE + 1], 100); // clamped
    }

    #[test]
    fn build_config_changed_packet() {
        let config = PadConfig {
            active_pad: 1,
            enabled_functions: function_bits::FOLLOW_PC | function_bits::SUBSCRIBERS,
        };
        let mut buf = [0u8; 64];
        let n = build_config_changed(&mut buf, &config).unwrap();
        assert_eq!(n, HEADER_SIZE + PadConfig::WIRE_SIZE);

        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::ConfigChanged);
        assert_eq!(header.data_type, DataType::PadConfig);

        let decoded = PadConfig::decode(&buf[HEADER_SIZE..]).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    fn ping_pong_round_trip() {
        let mut buf = [0u8; 64];
        let n = build_ping(&mut buf).unwrap();
        assert_eq!(n, HEADER_SIZE);
        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::Ping);
        assert_eq!(header.payload_len, 0);

        let n = build_pong(&mut buf).unwrap();
        assert_eq!(n, HEADER_SIZE);
        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::Pong);
    }

    #[test]
    fn command_id_all_variants() {
        assert_eq!(CommandId::from_u8(0x01), Some(CommandId::SetDisplay));
        assert_eq!(CommandId::from_u8(0x02), Some(CommandId::GetStatus));
        assert_eq!(CommandId::from_u8(0x03), Some(CommandId::StatusResp));
        assert_eq!(CommandId::from_u8(0x04), Some(CommandId::ConfigChanged));
        assert_eq!(CommandId::from_u8(0x05), Some(CommandId::Ack));
        assert_eq!(CommandId::from_u8(0x06), Some(CommandId::GetCapabilities));
        assert_eq!(CommandId::from_u8(0x07), Some(CommandId::CapabilitiesResp));
        assert_eq!(CommandId::from_u8(0x10), Some(CommandId::Ping));
        assert_eq!(CommandId::from_u8(0x11), Some(CommandId::Pong));
        assert_eq!(CommandId::from_u8(0x20), Some(CommandId::ShowDialog));
        assert_eq!(CommandId::from_u8(0x21), Some(CommandId::DialogResult));
        assert_eq!(CommandId::from_u8(0x00), None);
        assert_eq!(CommandId::from_u8(0x99), None);
    }

    #[test]
    fn data_type_all_variants() {
        assert_eq!(DataType::from_u8(0x01), Some(DataType::Text));
        assert_eq!(DataType::from_u8(0x02), Some(DataType::Numeric));
        assert_eq!(DataType::from_u8(0x03), Some(DataType::Progress));
        assert_eq!(DataType::from_u8(0x04), Some(DataType::IconId));
        assert_eq!(DataType::from_u8(0x05), Some(DataType::KeyValue));
        assert_eq!(DataType::from_u8(0x06), Some(DataType::Clear));
        assert_eq!(DataType::from_u8(0x10), Some(DataType::PadConfig));
        assert_eq!(DataType::from_u8(0x11), Some(DataType::DeviceInfo));
        assert_eq!(DataType::from_u8(0x00), None);
    }

    #[test]
    fn device_capabilities_round_trip() {
        let caps = DeviceCapabilities {
            protocol_version: PROTOCOL_VERSION,
            firmware_major: 0,
            firmware_minor: 2,
            firmware_patch: 0,
        };
        let mut buf = [0u8; DeviceCapabilities::WIRE_SIZE];
        assert_eq!(caps.encode(&mut buf), Some(4));
        let decoded = DeviceCapabilities::decode(&buf).unwrap();
        assert_eq!(caps, decoded);
    }

    #[test]
    fn device_capabilities_decode_too_short() {
        let buf = [0u8; 3]; // needs 4
        assert!(DeviceCapabilities::decode(&buf).is_none());
    }

    #[test]
    fn build_capabilities_request_packet() {
        let mut buf = [0u8; 64];
        let n = build_get_capabilities(&mut buf).unwrap();
        assert_eq!(n, HEADER_SIZE); // no payload
        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::GetCapabilities);
        assert_eq!(header.data_type, DataType::DeviceInfo);
        assert_eq!(header.payload_len, 0);
    }

    #[test]
    fn build_capabilities_resp_round_trip() {
        let caps = DeviceCapabilities {
            protocol_version: PROTOCOL_VERSION,
            firmware_major: 1,
            firmware_minor: 3,
            firmware_patch: 7,
        };
        let mut buf = [0u8; 64];
        let n = build_capabilities_resp(&mut buf, &caps).unwrap();
        assert_eq!(n, HEADER_SIZE + DeviceCapabilities::WIRE_SIZE);

        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::CapabilitiesResp);
        assert_eq!(header.data_type, DataType::DeviceInfo);
        assert_eq!(header.payload_len as usize, DeviceCapabilities::WIRE_SIZE);

        let decoded = DeviceCapabilities::decode(
            &buf[HEADER_SIZE..HEADER_SIZE + header.payload_len as usize],
        )
        .unwrap();
        assert_eq!(decoded, caps);
    }

    // ---- Dialog (ShowDialog / DialogResult) ----

    #[test]
    fn build_show_dialog_packet() {
        let mut buf = [0u8; 64];
        let n = build_show_dialog(&mut buf, 7, DialogKind::ConfirmCancel, "Reboot?").unwrap();
        assert_eq!(n, HEADER_SIZE + 2 + 7); // header + id + kind + "Reboot?"

        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::ShowDialog);
        assert_eq!(header.data_type, DataType::Text);
        assert_eq!(header.payload_len, 2 + 7);
        assert_eq!(buf[HEADER_SIZE], 7); // dialog_id
        assert_eq!(buf[HEADER_SIZE + 1], DialogKind::ConfirmCancel as u8); // kind
        assert_eq!(&buf[HEADER_SIZE + 2..HEADER_SIZE + 9], b"Reboot?");
    }

    #[test]
    fn build_show_dialog_text_too_long() {
        let mut buf = [0u8; 64];
        let ok = "a".repeat(MAX_PAYLOAD_SIZE - 2); // exactly 58 bytes
        assert!(build_show_dialog(&mut buf, 1, DialogKind::ConfirmCancel, &ok).is_some());

        let too_long = "a".repeat(MAX_PAYLOAD_SIZE - 1); // 59 bytes
        assert_eq!(build_show_dialog(&mut buf, 1, DialogKind::ConfirmCancel, &too_long), None);
    }

    #[test]
    fn build_show_dialog_empty_text() {
        let mut buf = [0u8; 64];
        let n = build_show_dialog(&mut buf, 0, DialogKind::Choice, "").unwrap();
        assert_eq!(n, HEADER_SIZE + 2);
    }

    #[test]
    fn build_dialog_result_packet() {
        let mut buf = [0u8; 64];
        let n = build_dialog_result(&mut buf, 7, 0, DialogResultCode::Confirm).unwrap();
        assert_eq!(n, HEADER_SIZE + 3); // id + selection + code

        let header = PacketHeader::decode(&buf).unwrap();
        assert_eq!(header.cmd, CommandId::DialogResult);
        assert_eq!(header.payload_len, 3);
        assert_eq!(buf[HEADER_SIZE], 7); // dialog_id
        assert_eq!(buf[HEADER_SIZE + 1], 0); // Confirm
    }

    #[test]
    fn dialog_result_code_all_variants() {
        assert_eq!(DialogResultCode::from_u8(0), Some(DialogResultCode::Confirm));
        assert_eq!(DialogResultCode::from_u8(1), Some(DialogResultCode::Cancel));
        assert_eq!(DialogResultCode::from_u8(2), Some(DialogResultCode::Timeout));
        assert_eq!(DialogResultCode::from_u8(3), None);
        assert_eq!(DialogResultCode::from_u8(0xFF), None);
    }
}
