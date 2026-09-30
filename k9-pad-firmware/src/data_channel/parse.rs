// INPUT:  k9_datachannel_proto, super::{DisplayCommand, DialogRequest}
// OUTPUT: parse_display_packet(), parse_dialog_packet(), handle_control_packet()
// POS:    协议解析：BLE 包 → DisplayCommand / DialogRequest 或控制响应（含 GetCapabilities）

use heapless::String;
use k9_datachannel_proto::*;

use super::{CompValue, DialogRequest, DisplayCommand};

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

/// 解析通用组件命令（LAYOUT_CFG / COMP_LAYOUT / COMP_SET），返回对应 DisplayCommand。
pub fn parse_component_packet(buf: &[u8]) -> Option<DisplayCommand> {
    let header = PacketHeader::decode(buf).ok()?;
    let payload = &buf[HEADER_SIZE..HEADER_SIZE + header.payload_len as usize];

    match header.cmd {
        CommandId::LayoutCfg => {
            // rows(1B) + cols(1B) + show_status(1B)
            if payload.len() < 3 {
                return None;
            }
            Some(DisplayCommand::SetLayout {
                rows: payload[0],
                cols: payload[1],
                show_status: payload[2] != 0,
            })
        }
        CommandId::CompLayout => {
            // id(1B) + row(1B) + col(1B) + kind(1B) + label_len(1B) + label
            if payload.len() < 5 {
                return None;
            }
            let kind = CompKind::from_u8(payload[3])?;
            let label_len = payload[4] as usize;
            if 5 + label_len > payload.len() {
                return None;
            }
            let label_bytes = &payload[5..5 + label_len];
            let label_str = core::str::from_utf8(label_bytes).ok()?;
            let mut label = String::new();
            for c in label_str.chars() {
                if label.push(c).is_err() {
                    break;
                }
            }
            Some(DisplayCommand::SetComp {
                id: payload[0],
                row: payload[1],
                col: payload[2],
                kind,
                label,
            })
        }
        CommandId::DialogOption => {
            // dialog_id(1B) + index(1B) + label
            if payload.len() < 3 {
                return None;
            }
            let label_str = core::str::from_utf8(&payload[2..]).ok()?;
            let mut label = String::new();
            for c in label_str.chars() {
                if label.push(c).is_err() {
                    break;
                }
            }
            Some(DisplayCommand::DialogOption {
                id: payload[0],
                index: payload[1],
                label,
            })
        }
        CommandId::CompSet => {
            // id + flags 至少 2 字节，否则越界 panic（no_std abort → 设备复位）
            if payload.len() < 2 {
                return None;
            }
            let id = payload[0];
            let wake = payload[1] != 0;
            let data = &payload[2..];
            // 值编码按 DataType 字段区分（与 CompKind 对应）
            let value = match header.data_type {
                DataType::Text => {
                    let s = core::str::from_utf8(data).ok()?;
                    let mut text = String::new();
                    for c in s.chars() {
                        if text.push(c).is_err() {
                            break;
                        }
                    }
                    CompValue::Text(text)
                }
                DataType::Numeric => {
                    if data.len() < 4 {
                        return None;
                    }
                    CompValue::Numeric(i32::from_le_bytes([data[0], data[1], data[2], data[3]]))
                }
                DataType::Progress => {
                    if data.is_empty() {
                        return None;
                    }
                    CompValue::Progress(data[0].min(100))
                }
                DataType::Percentage => {
                    if data.is_empty() {
                        return None;
                    }
                    CompValue::Percentage(data[0].min(100))
                }
                DataType::Checkbox => {
                    if data.is_empty() {
                        return None;
                    }
                    CompValue::Checkbox(data[0] != 0)
                }
                DataType::IconId => {
                    if data.len() < 2 {
                        return None;
                    }
                    CompValue::Icon(u16::from_le_bytes([data[0], data[1]]))
                }
                _ => return None,
            };
            Some(DisplayCommand::SetCompValue { id, wake, value })
        }
        _ => None,
    }
}

/// 解析一个完整的 64 字节包，返回 DialogRequest（如果是 SHOW_DIALOG）
///
/// payload 布局：`dialog_id(1B) + flags(1B, 保留) + UTF-8 text`。
/// 文本做 UTF-8 校验并按 heapless::String<56> 截断（同 parse_display_packet Text 分支）。
pub fn parse_dialog_packet(buf: &[u8]) -> Option<DialogRequest> {
    let header = PacketHeader::decode(buf).ok()?;

    if header.cmd != CommandId::ShowDialog {
        return None;
    }

    let payload = &buf[HEADER_SIZE..HEADER_SIZE + header.payload_len as usize];
    if payload.len() < 2 {
        return None;
    }

    let id = payload[0];
    // payload[1] = DialogKind
    let kind = DialogKind::from_u8(payload[1])?;
    let data = &payload[2..];

    let title_str = core::str::from_utf8(data).ok()?;
    let mut s = String::new();
    // Truncate if too long for heapless::String
    for c in title_str.chars() {
        if s.push(c).is_err() {
            break;
        }
    }
    Some(DialogRequest { id, kind, title: s })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dialog_packet_round_trip() {
        let mut buf = [0u8; 64];
        let n = build_show_dialog(&mut buf, 7, DialogKind::ConfirmCancel, "Reboot?").unwrap();
        let req = parse_dialog_packet(&buf[..n]).expect("should parse");
        assert_eq!(req.id, 7);
        assert_eq!(req.title.as_str(), "Reboot?");
    }

    #[test]
    fn parse_dialog_packet_ignores_other_commands() {
        // SetDisplay 包不应被识别为 ShowDialog
        let mut buf = [0u8; 64];
        let payload = [0u8, b'h', b'i'];
        let n = build_packet(&mut buf, CommandId::SetDisplay, DataType::Text, &payload).unwrap();
        assert!(parse_dialog_packet(&buf[..n]).is_none());

        // Ping 也不应被识别
        let mut buf = [0u8; 64];
        let n = build_packet(&mut buf, CommandId::Ping, DataType::Text, &[]).unwrap();
        assert!(parse_dialog_packet(&buf[..n]).is_none());
    }

    #[test]
    fn parse_dialog_packet_rejects_short_payload() {
        // payload 只有 dialog_id，缺 kind 字节
        let mut buf = [0u8; 64];
        let n = build_packet(&mut buf, CommandId::ShowDialog, DataType::Text, &[42]).unwrap();
        assert!(parse_dialog_packet(&buf[..n]).is_none());
    }

    #[test]
    fn parse_dialog_packet_truncates_long_text() {
        // 协议允许 58 字节文本，heapless::String<56> 只能装 56 字节 → 截断
        let mut buf = [0u8; 64];
        let text = "a".repeat(MAX_PAYLOAD_SIZE - 2); // 58 bytes
        let n = build_show_dialog(&mut buf, 1, DialogKind::Choice, &text).unwrap();
        let req = parse_dialog_packet(&buf[..n]).expect("should parse");
        assert_eq!(req.id, 1);
        assert_eq!(req.title.len(), 56);
    }

    #[test]
    fn parse_dialog_packet_rejects_invalid_utf8() {
        let mut buf = [0u8; 64];
        let payload = [1u8, 1, 0xFF, 0xFE]; // id + kind + 非法 UTF-8
        let n = build_packet(&mut buf, CommandId::ShowDialog, DataType::Text, &payload).unwrap();
        assert!(parse_dialog_packet(&buf[..n]).is_none());
    }

    #[test]
    fn parse_dialog_packet_empty_text() {
        let mut buf = [0u8; 64];
        let n = build_show_dialog(&mut buf, 3, DialogKind::ConfirmCancel, "").unwrap();
        let req = parse_dialog_packet(&buf[..n]).expect("should parse");
        assert_eq!(req.id, 3);
        assert_eq!(req.title.as_str(), "");
    }
}
