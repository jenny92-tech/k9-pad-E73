// INPUT:  k9-host-lib (AnyTransport, BleTransport, UsbTransport, K9Client), test_state, std::sync::mpsc
// OUTPUT: start_test_thread() — tokio 线程处理测试控制台命令
// POS:    测试桥接层 — 独立 tokio 线程，接收 TestCommand 执行 BLE/USB 操作，返回 TestEvent

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use log::{error, info};

use k9_host_lib::{AnyTransport, BleTransport, K9Client, Transport, UsbTransport};

use crate::test_state::{TestCommand, TestEvent, TransportType};

/// Start the test bridge on a dedicated OS thread with its own tokio runtime.
///
/// Returns the event receiver (for GPUI), the command sender (for UI), and the thread handle.
pub fn start_test_thread() -> (
    mpsc::Sender<TestCommand>,
    mpsc::Receiver<TestEvent>,
    JoinHandle<()>,
) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create test tokio runtime");

        rt.block_on(test_loop(cmd_rx, event_tx));
    });

    (cmd_tx, event_rx, handle)
}

/// Main command loop running inside the tokio runtime.
async fn test_loop(cmd_rx: mpsc::Receiver<TestCommand>, event_tx: mpsc::Sender<TestEvent>) {
    let mut client: Option<K9Client<AnyTransport>> = None;

    loop {
        // Block-wait for commands (with a short timeout so we can check connection liveness)
        let cmd = match cmd_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(cmd) => cmd,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                info!("Test bridge: command channel closed, exiting");
                break;
            }
        };

        match cmd {
            TestCommand::Connect(transport_type) => {
                // Disconnect existing connection first
                if let Some(ref c) = client {
                    let _ = c.transport().disconnect().await;
                }
                client = None;

                let result = match transport_type {
                    TransportType::Ble => {
                        send_log(&event_tx, "Connecting via BLE...");
                        BleTransport::connect(Duration::from_secs(10))
                            .await
                            .map(AnyTransport::Ble)
                    }
                    TransportType::Usb => {
                        send_log(&event_tx, "Connecting via USB...");
                        UsbTransport::auto_connect()
                            .await
                            .map(AnyTransport::Usb)
                    }
                };

                match result {
                    Ok(transport) => {
                        client = Some(K9Client::new(transport));
                        let _ = event_tx.send(TestEvent::Connected);
                        let label = match transport_type {
                            TransportType::Ble => "BLE",
                            TransportType::Usb => "USB",
                        };
                        send_log(&event_tx, &format!("Connected via {label}"));
                    }
                    Err(e) => {
                        let msg = format!("Connection failed: {e}");
                        let _ = event_tx.send(TestEvent::Error(msg));
                    }
                }
            }

            TestCommand::Disconnect => {
                if let Some(ref c) = client {
                    let _ = c.transport().disconnect().await;
                }
                client = None;
                let _ = event_tx.send(TestEvent::Disconnected);
                send_log(&event_tx, "Disconnected");
            }

            TestCommand::Ping => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.ping().await {
                    Ok(()) => send_log(&event_tx, "Ping → OK (Pong received)"),
                    Err(e) => send_err(&event_tx, &format!("Ping failed: {e}")),
                }
            }

            TestCommand::GetStatus => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.get_status().await {
                    Ok(config) => {
                        let msg = format!(
                            "Get status → pad={}, fn=0x{:04X}",
                            config.active_pad, config.enabled_functions
                        );
                        send_log(&event_tx, &msg);
                        let _ = event_tx.send(TestEvent::PadConfig(config));
                    }
                    Err(e) => send_err(&event_tx, &format!("Get status failed: {e}")),
                }
            }

            TestCommand::GetCapabilities => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.get_capabilities().await {
                    Ok(caps) => {
                        let msg = format!(
                            "Get capabilities → FW {}.{}.{}, Protocol v{}",
                            caps.firmware_major,
                            caps.firmware_minor,
                            caps.firmware_patch,
                            caps.protocol_version
                        );
                        send_log(&event_tx, &msg);
                        let _ = event_tx.send(TestEvent::DeviceCaps(caps));
                    }
                    Err(e) => send_err(&event_tx, &format!("Get capabilities failed: {e}")),
                }
            }

            TestCommand::PushText { slot, ref text } => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.push_text(slot, text).await {
                    Ok(()) => {
                        send_log(&event_tx, &format!("Push text slot={slot} \"{text}\" → OK"))
                    }
                    Err(e) => send_err(&event_tx, &format!("Push text failed: {e}")),
                }
            }

            TestCommand::PushNumeric { slot, value } => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.push_numeric(slot, value).await {
                    Ok(()) => {
                        send_log(&event_tx, &format!("Push numeric slot={slot} {value} → OK"))
                    }
                    Err(e) => send_err(&event_tx, &format!("Push numeric failed: {e}")),
                }
            }

            TestCommand::PushProgress { slot, value } => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.push_progress(slot, value).await {
                    Ok(()) => send_log(
                        &event_tx,
                        &format!("Push progress slot={slot} {value}% → OK"),
                    ),
                    Err(e) => send_err(&event_tx, &format!("Push progress failed: {e}")),
                }
            }

            TestCommand::ClearSlot(slot) => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.clear_slot(slot).await {
                    Ok(()) => send_log(&event_tx, &format!("Clear slot {slot} → OK")),
                    Err(e) => send_err(&event_tx, &format!("Clear slot failed: {e}")),
                }
            }
        }
    }

    // Clean up on exit
    if let Some(ref c) = client {
        let _ = c.transport().disconnect().await;
    }
}

fn send_log(tx: &mpsc::Sender<TestEvent>, msg: &str) {
    info!("Test: {msg}");
    let _ = tx.send(TestEvent::Log(msg.to_string()));
}

fn send_err(tx: &mpsc::Sender<TestEvent>, msg: &str) {
    error!("Test: {msg}");
    let _ = tx.send(TestEvent::Error(msg.to_string()));
}
