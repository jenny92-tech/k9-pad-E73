// INPUT:  k9_datachannel_proto, embassy_sync, heapless
// OUTPUT: DisplayCommand, DisplayDataCache, DisplaySlotData, DISPLAY_DATA channel, CONFIG_CHANGED watch
// POS:    BLE 数据通道协议解析，主机推送数据 → DisplayCommand → 显示循环
// data_channel.rs - 数据通道处理 + 配置上报
//
// 从主机接收显示数据（通过 BLE GATT 或 USB CDC），
// 解析协议包后分发 DisplayCommand 到显示循环。
// 当用户在菜单中更改配置时，发送 CONFIG_CHANGED 通知主机。

use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;
use heapless::String;
use k9_datachannel_proto::*;

// ---------------------------------------------------------------------------
// Display commands — 显示循环从这里读取
// ---------------------------------------------------------------------------

/// 从主机推送过来的显示命令
#[derive(Clone, Debug)]
pub enum DisplayCommand {
    SetText { slot: u8, text: String<56> },
    SetNumeric { slot: u8, value: i32 },
    SetProgress { slot: u8, value: u8 },
    SetIcon { slot: u8, icon_id: u16 },
    Clear { slot: u8 },
}

/// 单个 slot 的缓存数据
#[derive(Clone, Debug)]
pub enum DisplaySlotData {
    Text(String<56>),
    Numeric(i32),
    Progress(u8),
    Icon(u16),
}

/// 缓存所有 slot 的最新数据，供显示循环读取
pub struct DisplayDataCache {
    pub slots: [Option<DisplaySlotData>; 8],
}

impl DisplayDataCache {
    pub const fn new() -> Self {
        Self { slots: [const { None }; 8] }
    }

    pub fn apply(&mut self, cmd: &DisplayCommand) {
        match cmd {
            DisplayCommand::SetText { slot, text } => {
                if (*slot as usize) < self.slots.len() {
                    self.slots[*slot as usize] = Some(DisplaySlotData::Text(text.clone()));
                }
            }
            DisplayCommand::SetNumeric { slot, value } => {
                if (*slot as usize) < self.slots.len() {
                    self.slots[*slot as usize] = Some(DisplaySlotData::Numeric(*value));
                }
            }
            DisplayCommand::SetProgress { slot, value } => {
                if (*slot as usize) < self.slots.len() {
                    self.slots[*slot as usize] = Some(DisplaySlotData::Progress(*value));
                }
            }
            DisplayCommand::SetIcon { slot, icon_id } => {
                if (*slot as usize) < self.slots.len() {
                    self.slots[*slot as usize] = Some(DisplaySlotData::Icon(*icon_id));
                }
            }
            DisplayCommand::Clear { slot } => {
                if (*slot as usize) < self.slots.len() {
                    self.slots[*slot as usize] = None;
                }
            }
        }
    }

    /// 返回有数据的 slot 数量
    pub fn active_count(&self) -> u8 {
        self.slots.iter().filter(|s| s.is_some()).count() as u8
    }
}

// ---------------------------------------------------------------------------
// 通道定义
// ---------------------------------------------------------------------------

/// 显示命令通道：data_channel task → display loop
pub static DISPLAY_DATA: Channel<ThreadModeRawMutex, DisplayCommand, 4> = Channel::new();

/// 配置状态 Watch：wououi 菜单 → data_channel task
/// 当用户在菜单中更改 Pad 或功能配置时更新
pub static DATA_CHANNEL_CONFIG: Watch<ThreadModeRawMutex, PadConfig, 2> = Watch::new();

// ---------------------------------------------------------------------------
// 协议解析
// ---------------------------------------------------------------------------

/// 解析一个完整的 64 字节包，返回 DisplayCommand（如果是 SET_DISPLAY）
fn parse_display_packet(buf: &[u8]) -> Option<DisplayCommand> {
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

/// 处理非显示命令（PING, GET_STATUS 等）
fn handle_control_packet(buf: &[u8]) -> Option<[u8; 64]> {
    let header = PacketHeader::decode(buf).ok()?;

    match header.cmd {
        CommandId::Ping => {
            let mut resp = [0u8; 64];
            build_pong(&mut resp)?;
            Some(resp)
        }
        CommandId::GetStatus => {
            // 读取当前配置并回复
            // 配置通过 DATA_CHANNEL_CONFIG watch 获取
            // 这里用默认值，实际值由 display loop 推送
            let config = PadConfig::default();
            let mut resp = [0u8; 64];
            build_status_resp(&mut resp, &config)?;
            Some(resp)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 主任务
// ---------------------------------------------------------------------------

/// 数据通道处理主任务
///
/// 从 RMK 的 DATA_CHANNEL_RX 接收主机数据，解析协议，
/// 分发 DisplayCommand 到 DISPLAY_DATA channel。
/// 同时监听菜单配置变化，发送 CONFIG_CHANGED 到 DATA_CHANNEL_TX。
#[cfg(not(test))]
pub async fn run_data_channel() -> ! {
    use rmk::data_channel::{DATA_CHANNEL_RX, DATA_CHANNEL_TX};

    defmt::info!("Data channel task started");

    // 配置变化监听
    let mut config_rx = DATA_CHANNEL_CONFIG.receiver().unwrap();

    loop {
        // 同时等待：主机数据 或 配置变化
        match rmk::embassy_futures::select::select(
            DATA_CHANNEL_RX.receive(),
            config_rx.changed(),
        )
        .await
        {
            // 收到主机数据
            rmk::embassy_futures::select::Either::First(rx_buf) => {
                // 尝试解析为显示命令
                if let Some(cmd) = parse_display_packet(&rx_buf) {
                    let _ = DISPLAY_DATA.try_send(cmd);
                }

                // 尝试处理控制命令（PING, GET_STATUS）
                if let Some(resp) = handle_control_packet(&rx_buf) {
                    let _ = DATA_CHANNEL_TX.try_send(resp);
                }
            }

            // 配置变化 → 通知主机
            rmk::embassy_futures::select::Either::Second(config) => {
                let mut buf = [0u8; 64];
                if let Some(_n) = build_config_changed(&mut buf, &config) {
                    let _ = DATA_CHANNEL_TX.try_send(buf);
                    defmt::info!(
                        "Config changed: pad={} functions=0x{:04x}",
                        config.active_pad,
                        config.enabled_functions
                    );
                }
            }
        }
    }
}
