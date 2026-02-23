// INPUT:  Transport trait, shared-datachannel-proto (packet builders + decoders)
// OUTPUT: K9Client<T> — high-level async API for push_text/numeric/progress, ping, get_status, get_capabilities
// POS:    Application-level client — serializes request-response pairs over any Transport impl

use k9_datachannel_proto::{
    self as proto, CommandId, DataType, DeviceCapabilities, PacketHeader, PadConfig, HEADER_SIZE,
    MAX_PACKET_SIZE,
};
use log::debug;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::transport::{Transport, TransportError};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Text too long (max {max} bytes, got {got})")]
    TextTooLong { max: usize, got: usize },
}

/// High-level client for communicating with the K9-Pad.
///
/// All request-response methods are serialized by an internal mutex to prevent
/// concurrent calls from interleaving requests and responses.
pub struct K9Client<T: Transport> {
    transport: T,
    /// Serializes request-response pairs so concurrent callers don't interleave.
    request_lock: Mutex<()>,
}

impl<T: Transport> K9Client<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            request_lock: Mutex::new(()),
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

    /// Request the keyboard's current status/configuration.
    pub async fn get_status(&self) -> Result<PadConfig, ClientError> {
        let _guard = self.request_lock.lock().await;

        // Send GetStatus request
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_packet(&mut buf, CommandId::GetStatus, DataType::PadConfig, &[])
            .ok_or_else(|| ClientError::Protocol("Failed to build status request".into()))?;
        self.transport.send(&buf[..n]).await?;

        // Wait for StatusResp
        let response = self.transport.receive().await?;
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
        let _guard = self.request_lock.lock().await;

        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_ping(&mut buf)
            .ok_or_else(|| ClientError::Protocol("Failed to build ping".into()))?;
        self.transport.send(&buf[..n]).await?;

        let response = self.transport.receive().await?;
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
        let _guard = self.request_lock.lock().await;

        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_get_capabilities(&mut buf)
            .ok_or_else(|| ClientError::Protocol("Failed to build capabilities request".into()))?;
        self.transport.send(&buf[..n]).await?;

        let response = self.transport.receive().await?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::Mutex as TokioMutex;

    // ---- MockTransport ----

    /// Mock transport for unit-testing K9Client without real hardware.
    struct MockTransport {
        connected: AtomicBool,
        sent_data: TokioMutex<Vec<Vec<u8>>>,
        recv_queue: TokioMutex<VecDeque<Result<Vec<u8>, TransportError>>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                connected: AtomicBool::new(true),
                sent_data: TokioMutex::new(Vec::new()),
                recv_queue: TokioMutex::new(VecDeque::new()),
            }
        }

        /// Queue a response that `receive()` will return.
        async fn queue_recv(&self, resp: Result<Vec<u8>, TransportError>) {
            self.recv_queue.lock().await.push_back(resp);
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
            Ok(())
        }

        async fn receive(&self) -> Result<Vec<u8>, TransportError> {
            if !self.is_connected() {
                return Err(TransportError::NotConnected);
            }
            self.recv_queue
                .lock()
                .await
                .pop_front()
                .unwrap_or(Err(TransportError::Timeout))
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
        assert!(matches!(result, Err(ClientError::Protocol(_))));
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
