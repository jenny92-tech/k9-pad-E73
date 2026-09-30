// INPUT:  app_state (ConnectionStatus), shared-datachannel-proto (DeviceCapabilities, PadConfig), chrono
// OUTPUT: TestCommand, TestEvent, TestState, LogEntry — 测试页面的状态与通信类型
// POS:    测试控制台状态定义 — 定义 GPUI ↔ tokio 间测试命令和事件的数据结构

use crate::app_state::ConnectionStatus;
use k9_datachannel_proto::{DeviceCapabilities, PadConfig};

/// Transport type selection for the test console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    Ble,
    Usb,
}

impl Default for TransportType {
    fn default() -> Self {
        Self::Ble
    }
}

/// Commands sent from GPUI UI to the tokio test bridge thread.
pub enum TestCommand {
    Connect(TransportType),
    Disconnect,
    Ping,
    GetStatus,
    GetCapabilities,
    PushText { slot: u8, text: String },
    PushNumeric { slot: u8, value: i32 },
    PushProgress { slot: u8, value: u8 },
    ClearSlot(u8),
    /// 声明 2×2 组件布局 + 4 个组件（Time/Vol/Subs/AI）
    CompLayout,
    /// 设置组件值（Text）
    CompSetText { id: u8, text: String, wake: bool },
    /// 设置组件值（Numeric）
    CompSetNumeric { id: u8, value: i32, wake: bool },
    /// 设置组件值（Progress）
    CompSetU8 { id: u8, value: u8, wake: bool },
    /// 设置组件值（Percentage）
    CompSetPercent { id: u8, value: u8, wake: bool },
    /// 设置组件值（Checkbox）
    CompSetCheckbox { id: u8, on: bool, wake: bool },
    /// 授权弹窗测试（ConfirmCancel：Allow/Deny）
    ShowConfirmDialog,
    /// AI 多选弹窗测试（Choice：3 个选项）
    ShowChoiceDialog,
    /// 向本机 hook server 注入一条 hook 事件（刘海面板流程自测）。
    /// blocking=true 时等待审批响应并把响应 JSON 打到日志。
    InjectHook { json: String, blocking: bool },
}

/// Events sent from the tokio test bridge thread back to GPUI.
pub enum TestEvent {
    Connected,
    Disconnected,
    Error(String),
    Log(String),
    DeviceCaps(DeviceCapabilities),
    PadConfig(PadConfig),
}

/// A single log entry in the test console.
pub struct LogEntry {
    pub time: String,
    pub message: String,
    pub is_error: bool,
}

/// State for the test console page.
pub struct TestState {
    pub transport_type: TransportType,
    pub connection: ConnectionStatus,
    pub device_caps: Option<DeviceCapabilities>,
    pub pad_config: Option<PadConfig>,
    pub logs: Vec<LogEntry>,
}

impl Default for TestState {
    fn default() -> Self {
        Self {
            transport_type: TransportType::default(),
            connection: ConnectionStatus::Disconnected,
            device_caps: None,
            pad_config: None,
            logs: Vec::new(),
        }
    }
}

const MAX_LOG_ENTRIES: usize = 100;

impl TestState {
    pub fn add_log(&mut self, message: String, is_error: bool) {
        let time = chrono::Local::now().format("%H:%M:%S").to_string();
        self.logs.push(LogEntry {
            time,
            message,
            is_error,
        });
        if self.logs.len() > MAX_LOG_ENTRIES {
            self.logs.remove(0);
        }
    }
}
