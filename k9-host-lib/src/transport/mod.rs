// INPUT:  ble, usb sub-modules (feature-gated)
// OUTPUT: Transport trait + TransportError + AnyTransport — async send/receive/disconnect abstraction
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

/// Enum dispatch wrapper for using BLE or USB transport interchangeably.
///
/// Since `Transport` uses `impl Future` returns (not object-safe), this enum
/// manually delegates to the concrete type at runtime.
#[cfg(all(feature = "ble", feature = "usb"))]
pub enum AnyTransport {
    Ble(ble::BleTransport),
    Usb(usb::UsbTransport),
}

#[cfg(all(feature = "ble", feature = "usb"))]
impl Transport for AnyTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        match self {
            Self::Ble(t) => t.send(data).await,
            Self::Usb(t) => t.send(data).await,
        }
    }

    async fn receive(&self) -> Result<Vec<u8>, TransportError> {
        match self {
            Self::Ble(t) => t.receive().await,
            Self::Usb(t) => t.receive().await,
        }
    }

    async fn disconnect(&self) -> Result<(), TransportError> {
        match self {
            Self::Ble(t) => t.disconnect().await,
            Self::Usb(t) => t.disconnect().await,
        }
    }

    fn is_connected(&self) -> bool {
        match self {
            Self::Ble(t) => t.is_connected(),
            Self::Usb(t) => t.is_connected(),
        }
    }
}
