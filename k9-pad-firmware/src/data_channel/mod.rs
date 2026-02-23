// INPUT:  k9_datachannel_proto, embassy_sync, heapless
// OUTPUT: DisplayCommand, DisplayDataCache, DisplaySlotData, DISPLAY_DATA channel, CONFIG_CHANGED watch
// POS:    BLE 数据通道模块入口，类型定义 + 通道 statics + re-export parse/task
// data_channel — 数据通道处理 + 配置上报
//
// 从主机接收显示数据（通过 BLE GATT 或 USB CDC），
// 解析协议包后分发 DisplayCommand 到显示循环。
// 当用户在菜单中更改配置时，发送 CONFIG_CHANGED 通知主机。

mod parse;
mod task;

pub use parse::{handle_control_packet, parse_display_packet};
#[cfg(not(test))]
pub use task::run_data_channel;

use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;
use heapless::String;
use k9_datachannel_proto::PadConfig;

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
