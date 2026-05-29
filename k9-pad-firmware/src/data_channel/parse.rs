// INPUT:  k9_datachannel_proto, super::DisplayCommand
// OUTPUT: parse_display_packet(), handle_control_packet()
// POS:    协议解析：BLE 包 → DisplayCommand 或控制响应（含 GetCapabilities）

use heapless::String;
use k9_datachannel_proto::*;

use super::DisplayCommand;

// Firmware version — 编译期从 Cargo.toml 的 `version` 派生，避免手抄漂移。
const fn parse_u8(s: &str) -> u8 {
    let bytes = s.as_bytes();
    let mut result: u8 = 0;
    let mut i = 0;
    while i < bytes.len() {
        result = result * 10 + (bytes[i] - b'0');
        i += 1;
    }
    result
}
const FW_MAJOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MAJOR"));
const FW_MINOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MINOR"));
const FW_PATCH: u8 = parse_u8(env!("CARGO_PKG_VERSION_PATCH"));

/// 解析一个完整的 64 字节包，返回 DisplayCommand（如果是 SET_DISPLAY）
pub fn parse_display_packet(buf: &[u8]) -> Option<DisplayCommand> {
    let header = PacketHeader::decode(buf).ok()?;

    if header.cmd != CommandId::SetDisplay {
        return None;
    }

    let payload = &buf[HEADER_SIZE..HEADER_SIZE + header.payload_len as usize];
    if payload.is_empty() {
        return None;
    }

    let slot = payload[0];
    let data = &payload[1..];

    match header.data_type {
        DataType::Text => {
            let text_str = core::str::from_utf8(data).ok()?;
            let mut s = String::new();
            // Truncate if too long for heapless::String
            for c in text_str.chars() {
                if s.push(c).is_err() {
                    break;
                }
            }
            Some(DisplayCommand::SetText { slot, text: s })
        }
        DataType::Numeric => {
            if data.len() < 4 {
                return None;
            }
            let value = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Some(DisplayCommand::SetNumeric { slot, value })
        }
        DataType::Progress => {
            if data.is_empty() {
                return None;
            }
            Some(DisplayCommand::SetProgress {
                slot,
                value: data[0].min(100),
            })
        }
        DataType::IconId => {
            if data.len() < 2 {
                return None;
            }
            let icon_id = u16::from_le_bytes([data[0], data[1]]);
            Some(DisplayCommand::SetIcon { slot, icon_id })
        }
        DataType::Clear => Some(DisplayCommand::Clear { slot }),
        _ => None,
    }
}

/// 处理非显示命令（PING, GET_STATUS, GET_CAPABILITIES 等）
///
/// `current_config` is the latest `PadConfig` tracked by the data channel task,
/// used to reply to `GetStatus` with real device state.
pub fn handle_control_packet(buf: &[u8], current_config: &PadConfig) -> Option<[u8; 64]> {
    let header = PacketHeader::decode(buf).ok()?;

    match header.cmd {
        CommandId::Ping => {
            let mut resp = [0u8; 64];
            build_pong(&mut resp)?;
            Some(resp)
        }
        CommandId::GetStatus => {
            let mut resp = [0u8; 64];
            build_status_resp(&mut resp, current_config)?;
            Some(resp)
        }
        CommandId::GetCapabilities => {
            let caps = DeviceCapabilities {
                protocol_version: PROTOCOL_VERSION,
                firmware_major: FW_MAJOR,
                firmware_minor: FW_MINOR,
                firmware_patch: FW_PATCH,
            };
            let mut resp = [0u8; 64];
            build_capabilities_resp(&mut resp, &caps)?;
            Some(resp)
        }
        _ => None,
    }
}
