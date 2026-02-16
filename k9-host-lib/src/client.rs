use k9_datachannel_proto::{
    self as proto, CommandId, DataType, PacketHeader, PadConfig, HEADER_SIZE, MAX_PACKET_SIZE,
};
use log::debug;
use thiserror::Error;

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
pub struct K9Client<T: Transport> {
    transport: T,
}

impl<T: Transport> K9Client<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
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
        // Send GetStatus request
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let n = proto::build_packet(
            &mut buf,
            CommandId::GetStatus,
            DataType::PadConfig,
            &[],
        )
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

    /// Get a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }
}
