// INPUT:  (no_std core only)
// OUTPUT: CommandId, DataType, Packet, parse/serialize API
// POS:    BLE 数据通道协议 crate，固件和主机共用
#![cfg_attr(not(test), no_std)]

/// Maximum total packet size (header + payload), aligned with BLE characteristic size and USB CDC.
pub const MAX_PACKET_SIZE: usize = 64;

/// Protocol revision number. Increment when breaking changes are made.
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
            0x10 => Some(Self::PadConfig),
            0x11 => Some(Self::DeviceInfo),
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
    pub enabled_functions: u16,
}

/// Bit positions within `PadConfig::enabled_functions`.
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
}
