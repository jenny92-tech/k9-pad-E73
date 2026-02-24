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
