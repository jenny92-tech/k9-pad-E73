// INPUT:  k9-host-lib (BleTransport, K9Client), providers, app_state, tokio, std::sync::mpsc
// OUTPUT: start_tokio_thread(), bridge_loop() — tokio-GPUI 跨运行时桥接
// POS:    运行时桥接层 — 在独立 OS 线程启动 tokio runtime，通过 std::sync::mpsc 向 GPUI 传递状态事件

use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use gpui::AsyncApp;
use log::{error, info, warn};

use k9_datachannel_proto::function_bits;
use k9_host_lib::{BleTransport, K9Client, Transport};

use crate::app_state::{AppEvent, AppState, ConnectionStatus};
use crate::providers::ai_quota::AiQuotaProvider;
use crate::providers::bilibili::BilibiliProvider;
use crate::providers::time::TimeProvider;
use crate::providers::volume::VolumeProvider;
use crate::providers::{DisplayData, DisplayUpdate, Provider};

/// Start the tokio runtime on a dedicated OS thread.
///
/// Returns the event receiver (for the GPUI bridge loop) and the thread handle.
pub fn start_tokio_thread() -> (mpsc::Receiver<AppEvent>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(tokio_main(tx));
    });

    (rx, handle)
}

/// Main logic running inside the tokio runtime.
async fn tokio_main(event_tx: mpsc::Sender<AppEvent>) {
    let _ = event_tx.send(AppEvent::ConnectionChanged(ConnectionStatus::Connecting));

    // BLE connection retry loop
    let transport = loop {
        info!("Scanning for K9-Pad...");
        match BleTransport::connect(Duration::from_secs(10)).await {
            Ok(t) => break t,
            Err(e) => {
                let msg = format!("BLE connect failed: {e}");
                warn!("{msg}");
                let _ =
                    event_tx.send(AppEvent::ConnectionChanged(ConnectionStatus::Error(msg)));
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    };

    let client = Arc::new(K9Client::new(transport));

    // Fetch device capabilities
    match client.get_capabilities().await {
        Ok(caps) => {
            info!(
                "Device: FW {}.{}.{} | Protocol v{}",
                caps.firmware_major,
                caps.firmware_minor,
                caps.firmware_patch,
                caps.protocol_version
            );
            let _ = event_tx.send(AppEvent::DeviceCaps(caps));
        }
        Err(e) => warn!("Failed to get capabilities: {e}"),
    }

    // Fetch pad config (enabled functions)
    let enabled_functions = match client.get_status().await {
        Ok(config) => {
            info!(
                "Pad config: active={} enabled=0x{:04X}",
                config.active_pad, config.enabled_functions
            );
            let enabled = config.enabled_functions;
            let _ = event_tx.send(AppEvent::PadConfigUpdated(config));
            enabled
        }
        Err(e) => {
            warn!("Failed to get status: {e}, enabling all providers");
            0xFFFF
        }
    };

    let _ = event_tx.send(AppEvent::ConnectionChanged(ConnectionStatus::Connected));

    // Provider dispatch channel
    let (provider_tx, mut provider_rx) = tokio::sync::mpsc::channel::<DisplayUpdate>(64);

    spawn_providers(enabled_functions, provider_tx);

    // Dispatcher loop: forward provider updates to the keyboard
    while let Some(update) = provider_rx.recv().await {
        let result = match &update.data {
            DisplayData::Text(text) => client.push_text(update.slot, text).await,
            DisplayData::Numeric(value) => client.push_numeric(update.slot, *value).await,
            DisplayData::Progress(value) => client.push_progress(update.slot, *value).await,
        };

        if let Err(e) = result {
            error!("Push failed: {e}");
            if !client.transport().is_connected() {
                let _ = event_tx.send(AppEvent::ConnectionChanged(ConnectionStatus::Error(
                    "Device disconnected".into(),
                )));
                break;
            }
        }
    }

    let _ = event_tx.send(AppEvent::Shutdown);
}

/// Spawn provider tasks based on the enabled function bitmask.
fn spawn_providers(enabled: u16, tx: tokio::sync::mpsc::Sender<DisplayUpdate>) {
    if enabled & function_bits::TIME != 0 {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut p = TimeProvider::new(0, "%H:%M".into());
            if let Err(e) = p.start(tx).await {
                warn!("Time provider exited: {e}");
            }
        });
    }

    if enabled & function_bits::VOLUME != 0 {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut p = VolumeProvider::new(1);
            if let Err(e) = p.start(tx).await {
                warn!("Volume provider exited: {e}");
            }
        });
    }

    if enabled & function_bits::SUBSCRIBERS != 0 {
        let tx = tx.clone();
        tokio::spawn(async move {
            // TODO: make uid configurable
            let mut p = BilibiliProvider::new(2, 0, 300);
            if let Err(e) = p.start(tx).await {
                warn!("Bilibili provider exited: {e}");
            }
        });
    }

    if enabled & function_bits::AI_QUOTA != 0 {
        tokio::spawn(async move {
            let mut p = AiQuotaProvider::new(3);
            if let Err(e) = p.start(tx).await {
                warn!("AI quota provider exited: {e}");
            }
        });
    }
}

/// GPUI-side bridge loop: drains events from the tokio thread and updates AppState.
///
/// Runs as a GPUI foreground async task, polling the std::sync::mpsc receiver
/// at 50ms intervals to avoid blocking the UI thread.
pub async fn bridge_loop(rx: mpsc::Receiver<AppEvent>, cx: &mut AsyncApp) {
    loop {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;

        // Drain all pending events
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    let should_exit = matches!(event, AppEvent::Shutdown);

                    let _ = cx.update_global::<AppState, _>(|state, _cx| match event {
                        AppEvent::ConnectionChanged(status) => state.connection = status,
                        AppEvent::DeviceCaps(caps) => state.device_caps = Some(caps),
                        AppEvent::PadConfigUpdated(config) => state.pad_config = Some(config),
                        AppEvent::Shutdown => state.connection = ConnectionStatus::Disconnected,
                    });

                    if should_exit {
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
    }
}
