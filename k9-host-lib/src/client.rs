// INPUT:  Transport trait (shared via Arc), shared-datachannel-proto (packet builders + decoders), tokio sync primitives
// OUTPUT: K9Client<T> — async API (push_*, ping, get_status, get_capabilities, show_dialog) + DeviceEvent broadcast subscription
// POS:    Application-level client — background reader task demultiplexes responses (oneshot) and device events (broadcast)

use std::sync::Arc;
use std::time::Duration;

use k9_datachannel_proto::{
    self as proto, CommandId, DataType, DeviceCapabilities, DialogResultCode, PacketHeader,
    PadConfig, HEADER_SIZE, MAX_PACKET_SIZE,
};
use log::{debug, warn};
use thiserror::Error;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;

use crate::transport::{Transport, TransportError};

/// How long a request waits for its response before giving up.
/// Aligned with the BLE transport's 5s receive polling timeout.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Capacity of the device-event broadcast channel.
const EVENT_CHANNEL_CAPACITY: usize = 16;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Text too long (max {max} bytes, got {got})")]
    TextTooLong { max: usize, got: usize },
}

/// A device-initiated event, delivered to all `subscribe_events()` receivers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// User changed the config in the device menu (`ConfigChanged` packet).
    ConfigChanged(PadConfig),
    /// The user answered (or timed out on) a dialog shown via `show_dialog`.
    /// `selection` = 选中的选项 index（0-based）。
    DialogResult {
        id: u8,
        selection: u8,
        result: DialogResultCode,
    },
}

/// Slot where the in-flight request's response channel lives (at most one at a
/// time, serialized by `request_lock`).
type PendingResponse = Arc<Mutex<Option<oneshot::Sender<Vec<u8>>>>>;

/// High-level client for communicating with the K9-Pad.
///
/// A background reader task continuously receives packets from the transport and
/// demultiplexes them: response packets (StatusResp / CapabilitiesResp / Pong /
/// Ack) are forwarded to the currently waiting request via a oneshot channel,
/// while device-initiated events (ConfigChanged / DialogResult) are broadcast to
/// all `subscribe_events()` receivers. Request-response methods are serialized
/// by an internal mutex so concurrent callers don't interleave.
pub struct K9Client<T: Transport> {
    transport: Arc<T>,
    /// Serializes request-response pairs so concurrent callers don't interleave.
    request_lock: Mutex<()>,
    /// Response channel of the currently waiting request, if any.
    pending: PendingResponse,
    /// Broadcast channel for device-initiated events.
    event_tx: broadcast::Sender<DeviceEvent>,
    /// Background receive/demux task; aborted when the client is dropped.
    reader_task: JoinHandle<()>,
}

impl<T: Transport + 'static> K9Client<T> {
    /// Create a client around `transport`.
    ///
    /// Must be called from within a tokio runtime context — the background
    /// reader task is spawned immediately.
    pub fn new(transport: T) -> Self {
        let transport = Arc::new(transport);
        let pending: PendingResponse = Arc::new(Mutex::new(None));
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let reader_task = tokio::spawn(reader_loop(
            Arc::clone(&transport),
            Arc::clone(&pending),
            event_tx.clone(),
        ));
        Self {
            transport,
            request_lock: Mutex::new(()),
            pending,
            event_tx,
            reader_task,
        }
    }

    /// Push a text string to a display slot.
    pub async fn push_text(&self, slot: u8, text: &str) -> Result<(), ClientError> {
        let max_text = proto::MAX_PAYLOAD_SIZE - 1; // 1 byte for slot_id
        if text.len() > max_text {
            return Err(ClientError::TextTooLong {
                max: max_text,
                got: text.len(),
            });
        }
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_set_text(&mut buf, slot, text)
            .ok_or_else(|| ClientError::Protocol("Failed to build text packet".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Pushed text to slot {slot}: {text}");
        Ok(())
    }

    /// Push a numeric value to a display slot.
    pub async fn push_numeric(&self, slot: u8, value: i32) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_set_numeric(&mut buf, slot, value)
            .ok_or_else(|| ClientError::Protocol("Failed to build numeric packet".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Pushed numeric to slot {slot}: {value}");
        Ok(())
    }

    /// Push a progress value (0-100) to a display slot.
    pub async fn push_progress(&self, slot: u8, value: u8) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_set_progress(&mut buf, slot, value)
            .ok_or_else(|| ClientError::Protocol("Failed to build progress packet".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Pushed progress to slot {slot}: {value}%");
        Ok(())
    }

    /// Clear a display slot.
    pub async fn clear_slot(&self, slot: u8) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_set_clear(&mut buf, slot)
            .ok_or_else(|| ClientError::Protocol("Failed to build clear packet".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Cleared slot {slot}");
        Ok(())
    }

    // ------------------------------------------------------------------
    // 通用组件布局 API（LAYOUT_CFG / COMP_LAYOUT / COMP_SET）
    // ------------------------------------------------------------------

    /// 声明网格布局：rows×cols + 状态栏开关。
    pub async fn set_layout(&self, rows: u8, cols: u8, show_status: bool) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_layout_cfg(&mut buf, rows, cols, show_status)
            .ok_or_else(|| ClientError::Protocol("Failed to build layout_cfg".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Layout {rows}x{cols} status={show_status}");
        Ok(())
    }

    /// 声明一个组件（id、网格坐标、类型、标签）。
    pub async fn comp_layout(
        &self,
        id: u8,
        row: u8,
        col: u8,
        kind: proto::CompKind,
        label: &str,
    ) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_comp_layout(&mut buf, id, row, col, kind, label)
            .ok_or_else(|| ClientError::Protocol("Failed to build comp_layout".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Comp #{id} {kind:?} @({row},{col}) \"{label}\"");
        Ok(())
    }

    /// 更新组件值（Text）。
    pub async fn comp_set_text(&self, id: u8, wake: bool, text: &str) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_comp_set_text(&mut buf, id, wake, text)
            .ok_or_else(|| ClientError::Protocol("Failed to build comp_set_text".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Comp #{id} text={text}");
        Ok(())
    }

    /// 更新组件值（Numeric）。
    pub async fn comp_set_numeric(&self, id: u8, wake: bool, value: i32) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_comp_set_numeric(&mut buf, id, wake, value)
            .ok_or_else(|| ClientError::Protocol("Failed to build comp_set_numeric".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Comp #{id} numeric={value}");
        Ok(())
    }

    /// 更新组件值（Progress / Percentage / Checkbox 共用 u8 编码）。
    pub async fn comp_set_u8(&self, id: u8, wake: bool, value: u8) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_comp_set_u8(&mut buf, id, wake, value)
            .ok_or_else(|| ClientError::Protocol("Failed to build comp_set_u8".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Comp #{id} u8={value}");
        Ok(())
    }

    /// 更新组件值（Percentage）。
    pub async fn comp_set_percentage(&self, id: u8, wake: bool, value: u8) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_comp_set_percentage(&mut buf, id, wake, value)
            .ok_or_else(|| ClientError::Protocol("Failed to build comp_set_percentage".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Comp #{id} percentage={value}");
        Ok(())
    }

    /// 更新组件值（Checkbox）。
    pub async fn comp_set_checkbox(&self, id: u8, wake: bool, on: bool) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_comp_set_checkbox(&mut buf, id, wake, on)
            .ok_or_else(|| ClientError::Protocol("Failed to build comp_set_checkbox".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Comp #{id} checkbox={on}");
        Ok(())
    }

    /// 更新组件值（Icon）。
    pub async fn comp_set_icon(&self, id: u8, wake: bool, icon_id: u16) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_comp_set_icon(&mut buf, id, wake, icon_id)
            .ok_or_else(|| ClientError::Protocol("Failed to build comp_set_icon".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Comp #{id} icon={icon_id}");
        Ok(())
    }

    /// Show a confirm/cancel dialog with `text` on the device screen.
    ///
    /// Fire-and-forget: the user's choice arrives later as
    /// `DeviceEvent::DialogResult` via `subscribe_events()`.
    ///
    /// `text` must be ASCII-only (the firmware has no CJK font) and at most
    /// `MAX_PAYLOAD_SIZE - 2` = 58 bytes (dialog_id + flags take 2 bytes).
    pub async fn show_dialog(&self, dialog_id: u8, kind: proto::DialogKind, title: &str) -> Result<(), ClientError> {
        let max_text = proto::MAX_PAYLOAD_SIZE - 2; // dialog_id + kind
        if title.len() > max_text {
            return Err(ClientError::TextTooLong {
                max: max_text,
                got: title.len(),
            });
        }
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_show_dialog(&mut buf, dialog_id, kind, title)
            .ok_or_else(|| ClientError::Protocol("Failed to build show_dialog packet".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Requested dialog {dialog_id}: {title}");
        Ok(())
    }

    /// 声明弹窗的一个选项（label 最长 ~58B）。
    pub async fn dialog_option(&self, dialog_id: u8, index: u8, label: &str) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_dialog_option(&mut buf, dialog_id, index, label)
            .ok_or_else(|| ClientError::Protocol("Failed to build dialog_option packet".into()))?;
        self.transport.send(&buf[..n]).await?;
        debug!("Dialog {dialog_id} option {index}: {label}");
        Ok(())
    }

    /// Subscribe to device-initiated events (config changes, dialog results).
    ///
    /// Events broadcast before the subscription are not replayed; lagging
    /// receivers may miss events if more than 16 pile up.
    pub fn subscribe_events(&self) -> broadcast::Receiver<DeviceEvent> {
        self.event_tx.subscribe()
    }

    /// Request the keyboard's current status/configuration.
    pub async fn get_status(&self) -> Result<PadConfig, ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_packet(&mut buf, CommandId::GetStatus, DataType::PadConfig, &[])
            .ok_or_else(|| ClientError::Protocol("Failed to build status request".into()))?;
        let response = self.request(&buf[..n]).await?;

        let header = PacketHeader::decode(&response)
            .map_err(|e| ClientError::Protocol(format!("Invalid response header: {e:?}")))?;

        if header.cmd != CommandId::StatusResp || header.data_type != DataType::PadConfig {
            return Err(ClientError::Protocol(format!(
                "Unexpected response: cmd={:?} type={:?}",
                header.cmd, header.data_type
            )));
        }

        let payload = &response[HEADER_SIZE..HEADER_SIZE + header.payload_len as usize];
        PadConfig::decode(payload)
            .ok_or_else(|| ClientError::Protocol("Failed to decode PadConfig".into()))
    }

    /// Send a ping and wait for pong.
    pub async fn ping(&self) -> Result<(), ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_ping(&mut buf)
            .ok_or_else(|| ClientError::Protocol("Failed to build ping".into()))?;
        let response = self.request(&buf[..n]).await?;

        let header = PacketHeader::decode(&response)
            .map_err(|e| ClientError::Protocol(format!("Invalid pong header: {e:?}")))?;

        if header.cmd != CommandId::Pong {
            return Err(ClientError::Protocol(format!(
                "Expected Pong, got {:?}",
                header.cmd
            )));
        }

        debug!("Ping-pong successful");
        Ok(())
    }

    /// Request device capabilities (protocol version, firmware version).
    pub async fn get_capabilities(&self) -> Result<DeviceCapabilities, ClientError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_get_capabilities(&mut buf)
            .ok_or_else(|| ClientError::Protocol("Failed to build capabilities request".into()))?;
        let response = self.request(&buf[..n]).await?;

        let header = PacketHeader::decode(&response)
            .map_err(|e| ClientError::Protocol(format!("Invalid response header: {e:?}")))?;

        if header.cmd != CommandId::CapabilitiesResp || header.data_type != DataType::DeviceInfo {
            return Err(ClientError::Protocol(format!(
                "Unexpected response: cmd={:?} type={:?}",
                header.cmd, header.data_type
            )));
        }

        let payload = &response[HEADER_SIZE..HEADER_SIZE + header.payload_len as usize];
        DeviceCapabilities::decode(payload)
            .ok_or_else(|| ClientError::Protocol("Failed to decode DeviceCapabilities".into()))
    }

    /// Get a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Send a request packet and wait for its response packet.
    ///
    /// Serialized by `request_lock`; the response arrives via the oneshot slot
    /// filled by the reader task. The slot is cleared whatever the outcome.
    async fn request(&self, packet: &[u8]) -> Result<Vec<u8>, ClientError> {
        let _guard = self.request_lock.lock().await;

        let (tx, rx) = oneshot::channel();
        *self.pending.lock().await = Some(tx);

        let result = self.send_and_await(packet, rx).await;

        *self.pending.lock().await = None;
        result
    }

    async fn send_and_await(
        &self,
        packet: &[u8],
        rx: oneshot::Receiver<Vec<u8>>,
    ) -> Result<Vec<u8>, ClientError> {
        self.transport.send(packet).await?;
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(ClientError::Protocol("Response channel closed".into())),
            Err(_) => Err(ClientError::Transport(TransportError::Timeout)),
        }
    }
}

impl<T: Transport> Drop for K9Client<T> {
    fn drop(&mut self) {
        self.reader_task.abort();
    }
}

/// Background receive loop: reads packets from the transport and demultiplexes
/// them into request responses and device events.
///
/// The BLE transport's `receive()` polls with a 5s timeout, so an idle
/// `Timeout` is normal and the loop continues. Any other error (e.g.
/// `NotConnected`) terminates the task.
async fn reader_loop<T: Transport>(
    transport: Arc<T>,
    pending: PendingResponse,
    event_tx: broadcast::Sender<DeviceEvent>,
) {
    loop {
        match transport.receive().await {
            Ok(packet) => dispatch_packet(&packet, &pending, &event_tx).await,
            Err(TransportError::Timeout) => continue,
            Err(e) => {
                warn!("Reader task exiting on transport error: {e}");
                break;
            }
        }
    }
}

/// Route one received packet: responses go to the waiting request, events are
/// broadcast, anything undecodable/unexpected is logged and dropped.
async fn dispatch_packet(
    packet: &[u8],
    pending: &PendingResponse,
    event_tx: &broadcast::Sender<DeviceEvent>,
) {
    let header = match PacketHeader::decode(packet) {
        Ok(h) => h,
        Err(e) => {
            warn!("Dropping undecodable packet: {e:?}");
            return;
        }
    };
    let payload_end = HEADER_SIZE + header.payload_len as usize;
    if packet.len() < payload_end {
        warn!("Dropping truncated packet: cmd={:?}", header.cmd);
        return;
    }
    let payload = &packet[HEADER_SIZE..payload_end];

    match header.cmd {
        // Response class — forward to the waiting request, if any.
        CommandId::StatusResp | CommandId::CapabilitiesResp | CommandId::Pong | CommandId::Ack => {
            let waiter = pending.lock().await.take();
            match waiter {
                Some(tx) => {
                    // A send error means the request already timed out; fine.
                    let _ = tx.send(packet.to_vec());
                }
                None => warn!("Dropping unsolicited response: cmd={:?}", header.cmd),
            }
        }
        // Event class — decode payload and broadcast.
        CommandId::ConfigChanged => match PadConfig::decode(payload) {
            Some(config) => {
                let _ = event_tx.send(DeviceEvent::ConfigChanged(config));
            }
            None => warn!("Dropping malformed ConfigChanged payload"),
        },
        CommandId::DialogResult => {
            if payload.len() < 3 {
                warn!("Dropping truncated DialogResult payload");
                return;
            }
            match DialogResultCode::from_u8(payload[2]) {
                Some(result) => {
                    let _ = event_tx.send(DeviceEvent::DialogResult {
                        id: payload[0],
                        selection: payload[1],
                        result,
                    });
                }
                None => warn!("Dropping DialogResult with unknown code {}", payload[2]),
            }
        }
        other => warn!("Dropping unexpected packet: cmd={other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::Mutex as TokioMutex;

    // ---- MockTransport ----

    /// Mock transport for unit-testing K9Client without real hardware.
    ///
    /// Two inbound paths mimic real device behavior:
    /// - `queue_recv`: responses delivered only after the next `send()`
    ///   (request-response semantics — the device answers requests).
    /// - `push_incoming`: packets delivered immediately
    ///   (device-initiated events).
    struct MockTransport {
        connected: AtomicBool,
        sent_data: TokioMutex<Vec<Vec<u8>>>,
        pending_responses: TokioMutex<VecDeque<Result<Vec<u8>, TransportError>>>,
        recv_queue: TokioMutex<VecDeque<Result<Vec<u8>, TransportError>>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                connected: AtomicBool::new(true),
                sent_data: TokioMutex::new(Vec::new()),
                pending_responses: TokioMutex::new(VecDeque::new()),
                recv_queue: TokioMutex::new(VecDeque::new()),
            }
        }

        /// Queue a response that `receive()` will return after the next `send()`.
        async fn queue_recv(&self, resp: Result<Vec<u8>, TransportError>) {
            self.pending_responses.lock().await.push_back(resp);
        }

        /// Make a packet available to `receive()` immediately (device-initiated).
        async fn push_incoming(&self, packet: Vec<u8>) {
            self.recv_queue.lock().await.push_back(Ok(packet));
        }

        /// Return all data passed to `send()`.
        async fn sent_data(&self) -> Vec<Vec<u8>> {
            self.sent_data.lock().await.clone()
        }
    }

    impl Transport for MockTransport {
        async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
            if !self.is_connected() {
                return Err(TransportError::NotConnected);
            }
            self.sent_data.lock().await.push(data.to_vec());
            // The device answers requests: queued responses become receivable.
            let mut pending = self.pending_responses.lock().await;
            self.recv_queue.lock().await.append(&mut pending);
            Ok(())
        }

        async fn receive(&self) -> Result<Vec<u8>, TransportError> {
            if !self.is_connected() {
                return Err(TransportError::NotConnected);
            }
            // Poll briefly instead of returning Timeout instantly, so the
            // reader task doesn't busy-spin on an empty queue.
            for _ in 0..50 {
                if let Some(resp) = self.recv_queue.lock().await.pop_front() {
                    return resp;
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

    // ---- Helpers ----

    /// Build a mock Pong response packet.
    fn pong_packet() -> Vec<u8> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_pong(&mut buf).unwrap();
        buf[..n].to_vec()
    }

    /// Build a mock StatusResp response packet for the given config.
    fn status_resp_packet(config: &PadConfig) -> Vec<u8> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_status_resp(&mut buf, config).unwrap();
        buf[..n].to_vec()
    }

    /// Build a mock Ack response packet.
    fn ack_packet() -> Vec<u8> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_ack(&mut buf).unwrap();
        buf[..n].to_vec()
    }

    /// Build a mock CapabilitiesResp packet.
    fn capabilities_resp_packet(caps: &DeviceCapabilities) -> Vec<u8> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_capabilities_resp(&mut buf, caps).unwrap();
        buf[..n].to_vec()
    }

    /// Build a mock DialogResult event packet.
    fn dialog_result_packet(id: u8, result: DialogResultCode) -> Vec<u8> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_dialog_result(&mut buf, id, 0, result).unwrap();
        buf[..n].to_vec()
    }

    /// Build a mock ConfigChanged event packet.
    fn config_changed_packet(config: &PadConfig) -> Vec<u8> {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_config_changed(&mut buf, config).unwrap();
        buf[..n].to_vec()
    }

    /// Receive the next event with a timeout so a broken dispatch fails fast.
    async fn recv_event(rx: &mut broadcast::Receiver<DeviceEvent>) -> DeviceEvent {
        tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out waiting for device event")
            .expect("event channel closed")
    }

    // ---- push_text ----

    #[tokio::test]
    async fn push_text_success() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        assert!(client.push_text(0, "Hello").await.is_ok());

        let sent = client.transport().sent_data().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0][0], CommandId::SetDisplay as u8);
        assert_eq!(sent[0][1], DataType::Text as u8);
    }

    #[tokio::test]
    async fn push_text_too_long() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        let long_text = "a".repeat(proto::MAX_PAYLOAD_SIZE); // 60 bytes, exceeds max (59)
        let result = client.push_text(0, &long_text).await;
        assert!(matches!(
            result,
            Err(ClientError::TextTooLong { max: 59, got: 60 })
        ));
    }

    #[tokio::test]
    async fn push_text_exact_max_length() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        let text = "a".repeat(proto::MAX_PAYLOAD_SIZE - 1); // exactly 59 bytes
        assert!(client.push_text(0, &text).await.is_ok());
    }

    #[tokio::test]
    async fn push_text_empty() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        assert!(client.push_text(0, "").await.is_ok());
    }

    // ---- push_numeric ----

    #[tokio::test]
    async fn push_numeric_positive() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        assert!(client.push_numeric(1, 42).await.is_ok());

        let sent = client.transport().sent_data().await;
        assert_eq!(sent[0][0], CommandId::SetDisplay as u8);
        assert_eq!(sent[0][1], DataType::Numeric as u8);
    }

    #[tokio::test]
    async fn push_numeric_negative() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        assert!(client.push_numeric(1, -999).await.is_ok());
    }

    // ---- push_progress ----

    #[tokio::test]
    async fn push_progress_success() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        assert!(client.push_progress(2, 75).await.is_ok());

        let sent = client.transport().sent_data().await;
        assert_eq!(sent[0][0], CommandId::SetDisplay as u8);
        assert_eq!(sent[0][1], DataType::Progress as u8);
    }

    // ---- clear_slot ----

    #[tokio::test]
    async fn clear_slot_success() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        assert!(client.clear_slot(3).await.is_ok());

        let sent = client.transport().sent_data().await;
        assert_eq!(sent[0][0], CommandId::SetDisplay as u8);
        assert_eq!(sent[0][1], DataType::Clear as u8);
    }

    // ---- show_dialog ----

    #[tokio::test]
    async fn show_dialog_sends_packet() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        assert!(client.show_dialog(7, proto::DialogKind::ConfirmCancel, "Reboot?").await.is_ok());

        let sent = client.transport().sent_data().await;
        assert_eq!(sent.len(), 1);

        let header = PacketHeader::decode(&sent[0]).unwrap();
        assert_eq!(header.cmd, CommandId::ShowDialog);
        assert_eq!(sent[0][0], 0x20);
        assert_eq!(header.data_type, DataType::Text);
        assert_eq!(header.payload_len as usize, 2 + 7); // id + kind + "Reboot?"
        assert_eq!(sent[0][HEADER_SIZE], 7); // dialog_id
        assert_eq!(sent[0][HEADER_SIZE + 1], proto::DialogKind::ConfirmCancel as u8); // kind
        assert_eq!(&sent[0][HEADER_SIZE + 2..], b"Reboot?");
    }

    #[tokio::test]
    async fn show_dialog_text_length_limits() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        let ok_text = "a".repeat(proto::MAX_PAYLOAD_SIZE - 2); // exactly 58 bytes
        assert!(client.show_dialog(1, proto::DialogKind::ConfirmCancel, &ok_text).await.is_ok());

        let long_text = "a".repeat(proto::MAX_PAYLOAD_SIZE - 1); // 59 bytes
        let result = client.show_dialog(1, proto::DialogKind::ConfirmCancel, &long_text).await;
        assert!(matches!(
            result,
            Err(ClientError::TextTooLong { max: 58, got: 59 })
        ));
    }

    // ---- ping ----

    #[tokio::test]
    async fn ping_success() {
        let mock = MockTransport::new();
        mock.queue_recv(Ok(pong_packet())).await;

        let client = K9Client::new(mock);
        assert!(client.ping().await.is_ok());

        // Verify a Ping packet was sent
        let sent = client.transport().sent_data().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0][0], CommandId::Ping as u8);
    }

    #[tokio::test]
    async fn ping_unexpected_response() {
        let mock = MockTransport::new();
        mock.queue_recv(Ok(ack_packet())).await; // Ack instead of Pong

        let client = K9Client::new(mock);
        let result = client.ping().await;
        assert!(matches!(result, Err(ClientError::Protocol(_))));
    }

    #[tokio::test]
    async fn ping_receive_timeout() {
        let mock = MockTransport::new();
        mock.queue_recv(Err(TransportError::Timeout)).await;

        let client = K9Client::new(mock);
        let result = client.ping().await;
        assert!(matches!(
            result,
            Err(ClientError::Transport(TransportError::Timeout))
        ));
    }

    #[tokio::test]
    async fn ping_invalid_response_header() {
        let mock = MockTransport::new();
        mock.queue_recv(Ok(vec![0xFF, 0xFF, 0, 0])).await; // invalid command byte

        let client = K9Client::new(mock);
        let result = client.ping().await;
        // Undecodable packets can't be attributed to a request, so the reader
        // task drops them and the request times out instead of seeing the
        // malformed packet directly.
        assert!(matches!(
            result,
            Err(ClientError::Transport(TransportError::Timeout))
        ));
    }

    // ---- get_status ----

    #[tokio::test]
    async fn get_status_success() {
        let config = PadConfig {
            active_pad: 1,
            enabled_functions: 0x07,
        };
        let mock = MockTransport::new();
        mock.queue_recv(Ok(status_resp_packet(&config))).await;

        let client = K9Client::new(mock);
        let result = client.get_status().await.unwrap();

        assert_eq!(result.active_pad, 1);
        assert_eq!(result.enabled_functions, 0x07);

        // Verify a GetStatus packet was sent
        let sent = client.transport().sent_data().await;
        assert_eq!(sent[0][0], CommandId::GetStatus as u8);
    }

    #[tokio::test]
    async fn get_status_wrong_command() {
        let mock = MockTransport::new();
        mock.queue_recv(Ok(pong_packet())).await; // Pong instead of StatusResp

        let client = K9Client::new(mock);
        let result = client.get_status().await;
        assert!(matches!(result, Err(ClientError::Protocol(_))));
    }

    #[tokio::test]
    async fn get_status_truncated_payload() {
        let mock = MockTransport::new();
        // StatusResp header but payload too short for PadConfig (needs 3 bytes)
        let mut buf = [0u8; MAX_PACKET_SIZE];
        buf[0] = CommandId::StatusResp as u8;
        buf[1] = DataType::PadConfig as u8;
        buf[2] = 1; // payload_len = 1 (too short, PadConfig needs 3)
        buf[3] = 0;
        buf[4] = 0xFF;
        mock.queue_recv(Ok(buf[..5].to_vec())).await;

        let client = K9Client::new(mock);
        let result = client.get_status().await;
        assert!(matches!(result, Err(ClientError::Protocol(_))));
    }

    // ---- get_capabilities ----

    #[tokio::test]
    async fn get_capabilities_success() {
        let caps = DeviceCapabilities {
            protocol_version: proto::PROTOCOL_VERSION,
            firmware_major: 0,
            firmware_minor: 2,
            firmware_patch: 0,
        };
        let mock = MockTransport::new();
        mock.queue_recv(Ok(capabilities_resp_packet(&caps))).await;

        let client = K9Client::new(mock);
        let result = client.get_capabilities().await.unwrap();

        assert_eq!(result.protocol_version, proto::PROTOCOL_VERSION);
        assert_eq!(result.firmware_major, 0);
        assert_eq!(result.firmware_minor, 2);
        assert_eq!(result.firmware_patch, 0);

        // Verify a GetCapabilities packet was sent
        let sent = client.transport().sent_data().await;
        assert_eq!(sent[0][0], CommandId::GetCapabilities as u8);
    }

    #[tokio::test]
    async fn get_capabilities_timeout_fallback() {
        let mock = MockTransport::new();
        mock.queue_recv(Err(TransportError::Timeout)).await;

        let client = K9Client::new(mock);
        let result = client.get_capabilities().await;
        assert!(matches!(
            result,
            Err(ClientError::Transport(TransportError::Timeout))
        ));
    }

    #[tokio::test]
    async fn get_capabilities_wrong_response() {
        let mock = MockTransport::new();
        mock.queue_recv(Ok(pong_packet())).await;

        let client = K9Client::new(mock);
        let result = client.get_capabilities().await;
        assert!(matches!(result, Err(ClientError::Protocol(_))));
    }

    // ---- device events ----

    #[tokio::test]
    async fn dialog_result_event_broadcast() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);
        let mut rx = client.subscribe_events();

        client
            .transport()
            .push_incoming(dialog_result_packet(3, DialogResultCode::Confirm))
            .await;

        let event = recv_event(&mut rx).await;
        assert_eq!(
            event,
            DeviceEvent::DialogResult {
                id: 3,
                selection: 0,
                result: DialogResultCode::Confirm
            }
        );
    }

    #[tokio::test]
    async fn config_changed_event_broadcast() {
        let config = PadConfig {
            active_pad: 2,
            enabled_functions: 0x0B,
        };
        let mock = MockTransport::new();
        let client = K9Client::new(mock);
        let mut rx = client.subscribe_events();

        client
            .transport()
            .push_incoming(config_changed_packet(&config))
            .await;

        let event = recv_event(&mut rx).await;
        assert_eq!(event, DeviceEvent::ConfigChanged(config));
    }

    #[tokio::test]
    async fn event_does_not_disturb_ping() {
        let mock = MockTransport::new();
        mock.queue_recv(Ok(pong_packet())).await;

        let client = K9Client::new(mock);
        let mut rx = client.subscribe_events();

        // Device pushes an event while the ping is in flight.
        client
            .transport()
            .push_incoming(dialog_result_packet(9, DialogResultCode::Cancel))
            .await;

        assert!(client.ping().await.is_ok());

        let event = recv_event(&mut rx).await;
        assert_eq!(
            event,
            DeviceEvent::DialogResult {
                id: 9,
                selection: 0,
                result: DialogResultCode::Cancel
            }
        );
    }

    // ---- disconnected state ----

    #[tokio::test]
    async fn send_when_disconnected() {
        let mock = MockTransport::new();
        mock.connected.store(false, Ordering::Relaxed);

        let client = K9Client::new(mock);
        let result = client.push_text(0, "test").await;
        assert!(matches!(
            result,
            Err(ClientError::Transport(TransportError::NotConnected))
        ));
    }

    #[tokio::test]
    async fn multiple_sends_accumulate() {
        let mock = MockTransport::new();
        let client = K9Client::new(mock);

        client.push_text(0, "A").await.unwrap();
        client.push_numeric(1, 42).await.unwrap();
        client.push_progress(2, 50).await.unwrap();
        client.clear_slot(3).await.unwrap();

        let sent = client.transport().sent_data().await;
        assert_eq!(sent.len(), 4);
    }
}
