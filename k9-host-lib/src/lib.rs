// INPUT:  client, transport, ai_quota modules
// OUTPUT: Public API re-exports — K9Client, Transport, BleTransport, UsbTransport, ai_quota
// POS:    Crate root — facade that unifies all host-side communication primitives

pub mod client;
pub mod transport;

pub use client::{ClientError, K9Client};
pub use transport::{Transport, TransportError};

#[cfg(feature = "ble")]
pub use transport::ble::BleTransport;
#[cfg(feature = "usb")]
pub use transport::usb::UsbTransport;

#[cfg(feature = "ai-quota")]
pub mod ai_quota;
