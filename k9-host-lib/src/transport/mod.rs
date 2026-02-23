// INPUT:  ble, usb sub-modules (feature-gated)
// OUTPUT: Transport trait + TransportError — async send/receive/disconnect abstraction
// POS:    Transport layer root — defines the contract all transports must implement

#[cfg(feature = "ble")]
pub mod ble;
#[cfg(feature = "usb")]
pub mod usb;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Send failed: {0}")]
    SendFailed(String),
    #[error("Receive failed: {0}")]
    ReceiveFailed(String),
    #[error("Not connected")]
    NotConnected,
    #[error("Timeout")]
    Timeout,
}

/// Transport abstraction for communicating with the K9-Pad device.
///
/// Implementors must be `Send + Sync` so they can be used across async tasks.
pub trait Transport: Send + Sync {
    fn send(
        &self,
        data: &[u8],
    ) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;

    fn receive(&self) -> impl std::future::Future<Output = Result<Vec<u8>, TransportError>> + Send;

    fn disconnect(&self) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;

    fn is_connected(&self) -> bool;
}
