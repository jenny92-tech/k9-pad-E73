// INPUT:  serialport (CDC serial), shared-datachannel-proto (packet header decoding), Transport trait
// OUTPUT: UsbTransport — connects to K9-Pad via USB CDC serial, sends/receives framed packets
// POS:    USB transport impl — auto-detects K9-Pad by VID/PID, reads header+payload framed data

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use log::info;
use serialport::SerialPort;
use tokio::sync::Mutex;

use super::{Transport, TransportError};

/// K9-Pad USB VID (from keyboard.toml vendor_id).
const K9_USB_VID: u16 = 0x4C4B;
/// K9-Pad USB PID (from keyboard.toml product_id).
const K9_USB_PID: u16 = 0x4643;

/// USB CDC serial transport for K9-Pad.
pub struct UsbTransport {
    port: Mutex<Box<dyn SerialPort>>,
    connected: AtomicBool,
}

impl UsbTransport {
    /// List available serial ports that might be a K9-Pad device.
    pub fn list_ports() -> Vec<String> {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.port_name)
            .collect()
    }

    /// Open a USB CDC connection to the specified serial port.
    pub fn connect(port_name: &str, baud_rate: u32) -> Result<Self, TransportError> {
        let port = serialport::new(port_name, baud_rate)
            .timeout(Duration::from_secs(2))
            .open()
            .map_err(|e| TransportError::ConnectionFailed(format!("Serial open: {e}")))?;

        info!("Connected to K9-Pad via USB at {port_name}");

        Ok(Self {
            port: Mutex::new(port),
            connected: AtomicBool::new(true),
        })
    }

    /// Try to auto-detect a K9-Pad USB device from available serial ports.
    pub fn auto_connect() -> Result<Self, TransportError> {
        let ports = serialport::available_ports()
            .map_err(|e| TransportError::ConnectionFailed(format!("Port enumeration: {e}")))?;

        for port_info in &ports {
            if let serialport::SerialPortType::UsbPort(usb_info) = &port_info.port_type {
                // Primary: match on USB VID/PID (reliable, hardware-level)
                if usb_info.vid == K9_USB_VID && usb_info.pid == K9_USB_PID {
                    return Self::connect(&port_info.port_name, 115200);
                }
                // Fallback: match on product name string (for development/custom firmware)
                let product = usb_info.product.as_deref().unwrap_or("");
                if product.contains("K9") || product.contains("k9") {
                    return Self::connect(&port_info.port_name, 115200);
                }
            }
        }

        Err(TransportError::ConnectionFailed(
            "No K9-Pad USB device found".into(),
        ))
    }
}

impl Transport for UsbTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::NotConnected);
        }
        let mut port = self.port.lock().await;
        port.write_all(data)
            .map_err(|e| TransportError::SendFailed(format!("{e}")))
    }

    async fn receive(&self) -> Result<Vec<u8>, TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::NotConnected);
        }
        let mut port = self.port.lock().await;
        let mut buf = vec![0u8; k9_datachannel_proto::MAX_PACKET_SIZE];
        // Read header first (4 bytes), then the payload
        port.read_exact(&mut buf[..k9_datachannel_proto::HEADER_SIZE])
            .map_err(|e| TransportError::ReceiveFailed(format!("Header read: {e}")))?;

        let header = k9_datachannel_proto::PacketHeader::decode(&buf)
            .map_err(|e| TransportError::ReceiveFailed(format!("Header decode: {e:?}")))?;

        let payload_len = header.payload_len as usize;
        let total_len = k9_datachannel_proto::HEADER_SIZE + payload_len;
        if total_len > k9_datachannel_proto::MAX_PACKET_SIZE {
            return Err(TransportError::ReceiveFailed(format!(
                "Payload length {payload_len} exceeds max packet size"
            )));
        }
        if payload_len > 0 {
            port.read_exact(&mut buf[k9_datachannel_proto::HEADER_SIZE..total_len])
                .map_err(|e| TransportError::ReceiveFailed(format!("Payload read: {e}")))?;
        }

        buf.truncate(total_len);
        Ok(buf)
    }

    async fn disconnect(&self) -> Result<(), TransportError> {
        self.connected.store(false, Ordering::Relaxed);
        // SerialPort is dropped when the Mutex guard is dropped
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}
