// INPUT:  k9-host-lib (K9Client, Transport), k9_datachannel_proto
// OUTPUT: mock_dialog example — 无硬件模拟设备，验证 host 端 ShowDialog/DialogResult 闭环
// POS:    开发调试工具：在终端里跑通「下发弹窗 → 设备自动应答 → 事件回传」全链路
//
// 虚拟设备行为（模拟固件 v2）：
// - 收到 SHOW_DIALOG → 打印文本，1 秒后自动回 DIALOG_RESULT(Confirm)（模拟用户按 Yes）
// - 收到 GET_STATUS / GET_CAPABILITIES / PING → 正常应答
// - 收到 SET_DISPLAY → 打印 slot 内容（模拟上屏）
// - 启动 3 秒后主动推一次 CONFIG_CHANGED（模拟设备菜单改配置）
//
// 运行：cargo run -p k9-host-lib --example mock_dialog

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use k9_datachannel_proto::{
    self as proto, CommandId, DataType, DeviceCapabilities, DialogResultCode, PacketHeader,
    PadConfig, HEADER_SIZE, MAX_PACKET_SIZE,
};
use k9_host_lib::{DeviceEvent, K9Client, Transport, TransportError};
use tokio::sync::Mutex;

/// 模拟固件的虚拟设备 transport：send 时解析请求并异步生成响应/事件。
struct VirtualDevice {
    connected: AtomicBool,
    incoming: std::sync::Arc<Mutex<VecDeque<Vec<u8>>>>,
}

impl VirtualDevice {
    fn new() -> Self {
        Self {
            connected: AtomicBool::new(true),
            incoming: std::sync::Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    async fn push_incoming(&self, packet: Vec<u8>) {
        self.incoming.lock().await.push_back(packet);
    }
}

impl Transport for VirtualDevice {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }
        let header = PacketHeader::decode(data)
            .map_err(|e| TransportError::SendFailed(format!("bad packet: {e:?}")))?;
        let payload = &data[HEADER_SIZE..HEADER_SIZE + header.payload_len as usize];

        let mut buf = [0u8; MAX_PACKET_SIZE];
        match header.cmd {
            CommandId::ShowDialog => {
                let id = payload[0];
                let text = std::str::from_utf8(&payload[2..]).unwrap_or("<invalid utf8>");
                println!("[device] dialog #{id} shown: \"{text}\" (user will press Yes in 1s)");
                let n = proto::build_dialog_result(&mut buf, id, 0, DialogResultCode::Confirm).unwrap();
                let resp = buf[..n].to_vec();
                let queue = self.incoming.clone();
                // 模拟用户在设备上思考 1 秒后按确认。
                // 真实设备路径是固件按键 → DATA_CHANNEL_TX notify，
                // 这里推入接收队列等价于 BLE notification。
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    queue.lock().await.push_back(resp);
                });
            }
            CommandId::Ping => {
                let n = proto::build_pong(&mut buf).unwrap();
                self.push_incoming(buf[..n].to_vec()).await;
            }
            CommandId::GetStatus => {
                let config = PadConfig { active_pad: 0, enabled_functions: 0xFFFF };
                let n = proto::build_status_resp(&mut buf, &config).unwrap();
                self.push_incoming(buf[..n].to_vec()).await;
            }
            CommandId::GetCapabilities => {
                let caps = DeviceCapabilities {
                    protocol_version: proto::PROTOCOL_VERSION,
                    firmware_major: 0,
                    firmware_minor: 2,
                    firmware_patch: 0,
                };
                let n = proto::build_capabilities_resp(&mut buf, &caps).unwrap();
                self.push_incoming(buf[..n].to_vec()).await;
            }
            CommandId::SetDisplay => {
                let slot = payload[0];
                match header.data_type {
                    DataType::Text => println!(
                        "[device] slot {slot} text: \"{}\"",
                        std::str::from_utf8(&payload[1..]).unwrap_or("?")
                    ),
                    DataType::Progress => println!("[device] slot {slot} progress: {}%", payload[1]),
                    _ => println!("[device] slot {slot} data type {:?}", header.data_type),
                }
            }
            _ => println!("[device] ignored cmd {:?}", header.cmd),
        }
        Ok(())
    }

    async fn receive(&self) -> Result<Vec<u8>, TransportError> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }
        // 轮询 50ms 后报 Timeout（与 BLE transport 的空转语义一致，reader task 会继续）
        for _ in 0..50 {
            if let Some(p) = self.incoming.lock().await.pop_front() {
                return Ok(p);
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        Err(TransportError::Timeout)
    }

    async fn disconnect(&self) -> Result<(), TransportError> {
        self.connected.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    println!("== mock_dialog: 虚拟设备端到端演练 ==\n");

    let client = std::sync::Arc::new(K9Client::new(VirtualDevice::new()));
    let mut events = client.subscribe_events();

    println!("[host] ping ...");
    client.ping().await.expect("ping failed");
    println!("[host] pong ok\n");

    let caps = client.get_capabilities().await.expect("caps failed");
    println!(
        "[host] device FW {}.{}.{} protocol v{}\n",
        caps.firmware_major, caps.firmware_minor, caps.firmware_patch, caps.protocol_version
    );

    println!("[host] push display data ...");
    client.push_text(0, "12:34").await.unwrap();
    client.push_progress(1, 75).await.unwrap();
    println!();

    println!("[host] show_dialog #1 ...");
    client.show_dialog(1, proto::DialogKind::ConfirmCancel, "Confirm on device?").await.unwrap();

    println!("[host] waiting for dialog result ...");
    match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
        Ok(Ok(DeviceEvent::DialogResult { id, selection: 0, result })) => {
            println!("[host] dialog #{id} result: {result:?}");
            assert_eq!(id, 1);
            assert_eq!(result, DialogResultCode::Confirm);
        }
        other => {
            eprintln!("[host] unexpected: {other:?}");
            std::process::exit(1);
        }
    }

    println!("\n== 闭环验证通过 ==");
}
