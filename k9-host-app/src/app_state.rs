// INPUT:  gpui, shared-datachannel-proto (DeviceCapabilities, PadConfig)
// OUTPUT: AppState (Global), ConnectionStatus, AppEvent
// POS:    应用状态定义 — GPUI Global 状态容器，用于跨运行时同步 BLE 连接状态与设备信息

use gpui::Global;
use k9_datachannel_proto::{DeviceCapabilities, PadConfig};

/// BLE connection status.
#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Disconnected
    }
}

/// Global application state shared between tokio bridge and GPUI UI.
#[derive(Default)]
pub struct AppState {
    pub connection: ConnectionStatus,
    pub device_caps: Option<DeviceCapabilities>,
    pub pad_config: Option<PadConfig>,
}

impl Global for AppState {}

/// Events sent from the tokio thread to the GPUI bridge loop.
pub enum AppEvent {
    ConnectionChanged(ConnectionStatus),
    DeviceCaps(DeviceCapabilities),
    PadConfigUpdated(PadConfig),
    Shutdown,
}
