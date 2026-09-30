// INPUT:  k9-host-lib (AnyTransport, BleTransport, UsbTransport, K9Client), test_state, std::sync::mpsc
// OUTPUT: start_test_thread() — tokio 线程处理测试控制台命令
// POS:    测试桥接层 — 独立 tokio 线程，接收 TestCommand 执行 BLE/USB 操作，返回 TestEvent

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use log::{error, info};

use k9_host_lib::{AnyTransport, BleTransport, ClientError, K9Client, Transport, UsbTransport};

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
    // 最近一次成功连接的传输类型（断线后自动重连用）
    let mut last_transport: Option<TransportType> = None;

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
                        last_transport = Some(transport_type);
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
                    Err(e) => {
                        send_err(&event_tx, &format!("Ping failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
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
                    Err(e) => {
                        send_err(&event_tx, &format!("Get status failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
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
                    Err(e) => {
                        send_err(&event_tx, &format!("Get capabilities failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
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
                    Err(e) => {
                        send_err(&event_tx, &format!("Push text failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
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
                    Err(e) => {
                        send_err(&event_tx, &format!("Push numeric failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
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
                    Err(e) => {
                        send_err(&event_tx, &format!("Push progress failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
                }
            }

            TestCommand::ClearSlot(slot) => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.clear_slot(slot).await {
                    Ok(()) => send_log(&event_tx, &format!("Clear slot {slot} → OK")),
                    Err(e) => {
                        send_err(&event_tx, &format!("Clear slot failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
                }
            }

            TestCommand::CompLayout => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                // 2×2 + 状态栏 + 4 组件声明
                let _ = c.set_layout(2, 2, true).await;
                let _ = c.comp_layout(1, 0, 0, k9_datachannel_proto::CompKind::Text, "Time").await;
                let _ = c.comp_layout(2, 0, 1, k9_datachannel_proto::CompKind::Progress, "Vol").await;
                let _ = c.comp_layout(3, 1, 0, k9_datachannel_proto::CompKind::Numeric, "Subs").await;
                let _ = c.comp_layout(4, 1, 1, k9_datachannel_proto::CompKind::Percentage, "AI").await;
                send_log(&event_tx, "组件布局已下发 (2x2 + 状态栏)");
            }

            TestCommand::CompSetText { id, ref text, wake } => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.comp_set_text(id, wake, text).await {
                    Ok(()) => send_log(&event_tx, &format!("Comp #{id} text=\"{text}\" wake={wake} → OK")),
                    Err(e) => {
                        send_err(&event_tx, &format!("comp_set_text failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
                }
            }

            TestCommand::CompSetNumeric { id, value, wake } => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.comp_set_numeric(id, wake, value).await {
                    Ok(()) => send_log(&event_tx, &format!("Comp #{id} numeric={value} wake={wake} → OK")),
                    Err(e) => {
                        send_err(&event_tx, &format!("comp_set_numeric failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
                }
            }

            TestCommand::CompSetU8 { id, value, wake } => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                // Progress 组件用 u8 编码
                match c.comp_set_u8(id, wake, value).await {
                    Ok(()) => send_log(&event_tx, &format!("Comp #{id} u8={value} wake={wake} → OK")),
                    Err(e) => {
                        send_err(&event_tx, &format!("comp_set_u8 failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
                }
            }

            TestCommand::CompSetPercent { id, value, wake } => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.comp_set_percentage(id, wake, value).await {
                    Ok(()) => send_log(&event_tx, &format!("Comp #{id} percentage={value} wake={wake} → OK")),
                    Err(e) => {
                        send_err(&event_tx, &format!("comp_set_percentage failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
                }
            }

            TestCommand::CompSetCheckbox { id, on, wake } => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                match c.comp_set_checkbox(id, wake, on).await {
                    Ok(()) => send_log(&event_tx, &format!("Comp #{id} checkbox={on} wake={wake} → OK")),
                    Err(e) => {
                        send_err(&event_tx, &format!("comp_set_checkbox failed: {e}"));
                        reconnect_if_dead(&mut client, &last_transport, &event_tx, &e).await;
                    }
                }
            }

            TestCommand::ShowConfirmDialog => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                // ConfirmCancel：Allow/Deny
                let _ = c.show_dialog(0, k9_datachannel_proto::DialogKind::ConfirmCancel, "Permission Request").await;
                let _ = c.dialog_option(0, 0, "Allow").await;
                let _ = c.dialog_option(0, 1, "Deny").await;
                send_log(&event_tx, "授权弹窗已下发 (Allow/Deny)，滚轮选 + Select 确认");
            }

            TestCommand::ShowChoiceDialog => {
                let Some(ref c) = client else {
                    send_err(&event_tx, "Not connected");
                    continue;
                };
                // Choice：3 个选项
                let _ = c.show_dialog(0, k9_datachannel_proto::DialogKind::Choice, "AI 请选择").await;
                let _ = c.dialog_option(0, 0, "方案 A").await;
                let _ = c.dialog_option(0, 1, "方案 B").await;
                let _ = c.dialog_option(0, 2, "方案 C").await;
                send_log(&event_tx, "AI 多选弹窗已下发 (3 选项)");
            }

            TestCommand::InjectHook { ref json, blocking } => {
                // 刘海面板流程自测：向本机 hook server 注入一条 hook 事件。
                // 走真实 socket 链路（与 k9-hook-bridge 同协议），在独立线程执行，
                // 避免阻塞测试命令循环（阻塞事件要等用户应答才返回）。
                let json = json.clone();
                let tx = event_tx.clone();
                std::thread::spawn(move || {
                    match inject_hook(&json, blocking) {
                        Ok(msg) => send_log(&tx, &msg),
                        Err(e) => send_err(&tx, &format!("Hook inject failed: {e}")),
                    }
                });
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

/// 命令执行失败且属于传输层错误（连接已死）→ 丢弃死 client、通知 UI（按钮回 Disconnected）
/// 并自动重连最后一次使用的传输。非传输层错误（协议/数据问题如 TextTooLong）不动连接。
async fn reconnect_if_dead(
    client: &mut Option<K9Client<AnyTransport>>,
    last_transport: &Option<TransportType>,
    event_tx: &mpsc::Sender<TestEvent>,
    err: &ClientError,
) {
    if !matches!(err, ClientError::Transport(_)) {
        return;
    }
    // 连接已死：丢弃死 client，按钮状态回 Disconnected
    *client = None;
    let _ = event_tx.send(TestEvent::Disconnected);
    send_log(event_tx, "Connection lost — reconnecting...");
    let Some(tt) = last_transport else {
        send_err(event_tx, "No transport to reconnect to (click Connect first)");
        return;
    };
    let label = match tt {
        TransportType::Ble => "BLE",
        TransportType::Usb => "USB",
    };
    let result = match tt {
        TransportType::Ble => BleTransport::connect(Duration::from_secs(10))
            .await
            .map(AnyTransport::Ble),
        TransportType::Usb => UsbTransport::auto_connect().await.map(AnyTransport::Usb),
    };
    match result {
        Ok(t) => {
            *client = Some(K9Client::new(t));
            let _ = event_tx.send(TestEvent::Connected);
            send_log(event_tx, &format!("Reconnected via {label}"));
        }
        Err(e) => send_err(event_tx, &format!("Reconnect failed: {e}")),
    }
}

/// 向本机 hook server 注入一条 hook 事件（与 k9-hook-bridge 同协议：
/// 写 JSON → shutdown(Write) 半关闭 → 阻塞事件读响应到 EOF）。
fn inject_hook(json: &str, blocking: bool) -> Result<String, String> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let path = crate::hook_server::socket_path();
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("connect {} failed: {e}", path.display()))?;
    stream
        .write_all(json.as_bytes())
        .map_err(|e| format!("write failed: {e}"))?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|e| format!("shutdown failed: {e}"))?;

    if !blocking {
        // 非阻塞事件 server 回 `{}`，读一下确认即可
        let mut resp = String::new();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        let _ = stream.read_to_string(&mut resp);
        return Ok(format!("Hook sent ({}B): {}", json.len(), event_name_of(json)));
    }

    // 阻塞事件：等用户应答（看门狗兜底 300s，超时略宽）
    stream
        .set_read_timeout(Some(Duration::from_secs(330)))
        .map_err(|e| e.to_string())?;
    let mut resp = String::new();
    stream
        .read_to_string(&mut resp)
        .map_err(|e| format!("read response failed: {e}"))?;
    if resp.is_empty() {
        return Err("empty response (server closed without answer)".into());
    }
    Ok(format!("Hook response: {resp}"))
}

/// 从注入 JSON 里抠事件名用于日志显示（不做完整解析，仅展示）
fn event_name_of(json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.get("hook_event_name")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| "<unknown>".into())
}

fn send_err(tx: &mpsc::Sender<TestEvent>, msg: &str) {
    error!("Test: {msg}");
    let _ = tx.send(TestEvent::Error(msg.to_string()));
}
