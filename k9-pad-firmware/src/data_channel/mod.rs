// INPUT:  k9_datachannel_proto, embassy_sync, heapless
// OUTPUT: DisplayCommand, DisplayDataCache, DisplaySlotData, DialogRequest, DialogOutcome,
//         DISPLAY_DATA / DIALOG_DATA / DIALOG_RESULT channel, CONFIG_CHANGED watch
// POS:    BLE 数据通道模块入口，类型定义 + 通道 statics + re-export parse/task
// data_channel — 数据通道处理 + 配置上报 + 主机确认弹窗
//
// 从主机接收显示数据（通过 BLE GATT 或 USB CDC），
// 解析协议包后分发 DisplayCommand 到显示循环。
// 当用户在菜单中更改配置时，发送 CONFIG_CHANGED 通知主机。
// ShowDialog 请求经 DIALOG_DATA 下发到显示循环，用户选择结果经 DIALOG_RESULT 回传主机。

mod parse;
mod task;

pub use parse::{handle_control_packet, parse_dialog_packet, parse_display_packet};
#[cfg(not(test))]
pub use task::run_data_channel;

// ThreadModeRawMutex 仅 Cortex-M 可用；host 端 cfg(test) 用 CriticalSectionRawMutex 顶替
// （测试只构造 Channel statics，不会真正加锁）
#[cfg(not(test))]
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
#[cfg(test)]
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;
use heapless::String;
use k9_datachannel_proto::{CompKind, DialogKind, PadConfig};

// ---------------------------------------------------------------------------
// Display commands — 显示循环从这里读取
// ---------------------------------------------------------------------------

/// 从主机推送过来的显示命令
#[derive(Clone, Debug)]
pub enum DisplayCommand {
    // -- 旧 slot 命令（保留兼容；host 新架构用下面组件命令）--
    SetText { slot: u8, text: String<56> },
    SetNumeric { slot: u8, value: i32 },
    SetProgress { slot: u8, value: u8 },
    SetIcon { slot: u8, icon_id: u16 },
    Clear { slot: u8 },
    // -- 通用组件命令（LAYOUT_CFG / COMP_LAYOUT / COMP_SET）--
    /// 布局配置：网格 rows×cols + 状态栏开关
    SetLayout { rows: u8, cols: u8, show_status: bool },
    /// 声明组件：id + 类型 + 标签 + 网格坐标
    SetComp {
        id: u8,
        kind: CompKind,
        label: String<16>,
        row: u8,
        col: u8,
    },
    /// 更新组件值（kind 决定 value 含义）；wake=true 时屏幕关着也唤醒
    SetCompValue { id: u8, wake: bool, value: CompValue },
    /// 弹窗选项（ShowDialog 之后逐条下发，设备攒齐渲染）
    DialogOption { id: u8, index: u8, label: String<56> },
}

/// 组件值（与 CompKind 对应）
#[derive(Clone, Debug)]
pub enum CompValue {
    Text(String<56>),
    Numeric(i32),
    Progress(u8),
    Percentage(u8),
    Checkbox(bool),
    Icon(u16),
}

/// 组件实例（id 索引，最多 8 个）
#[derive(Clone, Debug)]
pub struct Comp {
    pub kind: CompKind,
    pub label: String<16>,
    pub row: u8,
    pub col: u8,
    pub value: CompValue,
}

/// 组件网格缓存：布局配置 + 组件表（供显示循环渲染）
#[derive(Clone, Debug)]
pub struct CompCache {
    pub rows: u8,
    pub cols: u8,
    pub show_status: bool,
    pub comps: [Option<Comp>; 8],
}

impl CompCache {
    pub const fn new() -> Self {
        Self {
            rows: 2,
            cols: 2,
            show_status: true,
            comps: [const { None }; 8],
        }
    }

    pub fn apply(&mut self, cmd: &DisplayCommand) {
        match cmd {
            DisplayCommand::SetLayout { rows, cols, show_status } => {
                self.rows = (*rows).clamp(1, 4);
                self.cols = (*cols).clamp(1, 4);
                self.show_status = *show_status;
            }
            DisplayCommand::SetComp { id, kind, label, row, col } => {
                if let Some(slot) = self.comps.get_mut(*id as usize) {
                    // 已有组件：更新定义（保留旧值，类型变了就重置值）
                    let value = match slot {
                        Some(c) if c.kind == *kind => c.value.clone(),
                        _ => default_comp_value(*kind),
                    };
                    *slot = Some(Comp {
                        kind: *kind,
                        label: label.clone(),
                        row: *row,
                        col: *col,
                        value,
                    });
                }
            }
            DisplayCommand::SetCompValue { id, wake: _, value } => {
                if let Some(Some(comp)) = self.comps.get_mut(*id as usize) {
                    // kind 与 value 类型匹配才更新（host 可能先推值再声明，忽略不匹配）
                    if kind_matches(&comp.kind, value) {
                        comp.value = value.clone();
                    }
                }
            }
            _ => {}
        }
    }
}

fn default_comp_value(kind: CompKind) -> CompValue {
    match kind {
        CompKind::Text => CompValue::Text(String::new()),
        CompKind::Numeric => CompValue::Numeric(0),
        CompKind::Progress => CompValue::Progress(0),
        CompKind::Percentage => CompValue::Percentage(0),
        CompKind::Checkbox => CompValue::Checkbox(false),
        CompKind::Icon => CompValue::Icon(0),
    }
}

fn kind_matches(kind: &CompKind, value: &CompValue) -> bool {
    matches!(
        (kind, value),
        (CompKind::Text, CompValue::Text(_))
            | (CompKind::Numeric, CompValue::Numeric(_))
            | (CompKind::Progress, CompValue::Progress(_))
            | (CompKind::Percentage, CompValue::Percentage(_))
            | (CompKind::Checkbox, CompValue::Checkbox(_))
            | (CompKind::Icon, CompValue::Icon(_))
    )
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
            // 新组件命令不作用于旧 slot 缓存（走 CompCache）
            _ => {}
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
// Host dialog — 主机确认弹窗（ShowDialog 下发 / DialogResult 回传）
// ---------------------------------------------------------------------------

/// 主机下发的确认弹窗请求（含类型 + 标题；选项由 DialogOption 命令单独下发）
#[derive(Clone, Debug)]
pub struct DialogRequest {
    pub id: u8,
    pub kind: DialogKind,
    pub title: String<56>,
}

/// 弹窗结果：display loop → data_channel task → 主机
/// `result` 为协议 DialogResultCode（0=Confirm 1=Cancel 2=Timeout）
#[derive(Clone, Debug)]
pub struct DialogOutcome {
    pub id: u8,
    /// 选中的选项 index（0-based；Cancel/Timeout 无意义）
    pub selection: u8,
    pub result: u8,
}

/// 弹窗请求通道：data_channel task → display loop
pub static DIALOG_DATA: Channel<ThreadModeRawMutex, DialogRequest, 2> = Channel::new();

/// 弹窗结果通道：display loop → data_channel task
/// 用 Channel 不用 Watch——结果是事件，不允许被覆盖
pub static DIALOG_RESULT: Channel<ThreadModeRawMutex, DialogOutcome, 2> = Channel::new();
