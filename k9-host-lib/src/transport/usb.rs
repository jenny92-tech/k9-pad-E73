// INPUT:  hidapi (Raw HID), shared-datachannel-proto (packet header decoding), Transport trait, tokio
// OUTPUT: UsbTransport — connects to K9-Pad via USB Raw HID, sends/receives 64-byte reports
// POS:    USB transport impl — auto-detects K9-Pad by VID/PID + usage_page 0xFF61, non-blocking I/O via spawn_blocking

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hidapi::{HidApi, HidDevice};
use log::{debug, info, warn};

use super::{Transport, TransportError};

use k9_datachannel_proto::identifiers::{DATA_CHANNEL_USAGE_PAGE, K9_USB_PID, K9_USB_VID};
/// Read timeout in milliseconds.
const READ_TIMEOUT_MS: i32 = 5000;

/// USB Raw HID transport for K9-Pad.
///
/// Uses `spawn_blocking` for all HID I/O to avoid blocking the tokio runtime.
/// The device is wrapped in `Arc<Mutex>` so it can be moved into blocking tasks.
pub struct UsbTransport {
    device: Arc<Mutex<HidDevice>>,
    connected: AtomicBool,
}

/// Info about a discovered K9-Pad HID device.
#[derive(Debug, Clone)]
pub struct HidDeviceInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub product: String,
}

impl UsbTransport {
    /// List HID devices matching K9-Pad VID/PID.
    pub fn list_devices() -> Vec<HidDeviceInfo> {
        let api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                warn!("Failed to initialize HID API: {e}");
                return Vec::new();
            }
        };

        api.device_list()
            .filter(|d| d.vendor_id() == K9_USB_VID && d.product_id() == K9_USB_PID)
            .map(|d| HidDeviceInfo {
                path: d.path().to_string_lossy().into_owned(),
                vendor_id: d.vendor_id(),
                product_id: d.product_id(),
                usage_page: d.usage_page(),
                product: d.product_string().unwrap_or("").to_string(),
            })
            .collect()
    }

    /// Try to auto-detect a K9-Pad USB data channel device.
    ///
    /// Filters HID devices by VID/PID and usage_page=0xFF61 (data channel),
    /// then probes with a PING/PONG handshake.
    pub async fn auto_connect() -> Result<Self, TransportError> {
        let api = HidApi::new()
            .map_err(|e| TransportError::ConnectionFailed(format!("HID API init: {e}")))?;

        // Find data channel interface by usage page
        let candidates: Vec<_> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == K9_USB_VID
                    && d.product_id() == K9_USB_PID
                    && d.usage_page() == DATA_CHANNEL_USAGE_PAGE
            })
            .collect();

        if candidates.is_empty() {
            // Fallback: try any matching VID/PID device
            let fallback: Vec<_> = api
                .device_list()
                .filter(|d| d.vendor_id() == K9_USB_VID && d.product_id() == K9_USB_PID)
                .collect();

            if fallback.is_empty() {
                return Err(TransportError::ConnectionFailed(
                    "No K9-Pad USB device found".into(),
                ));
            }

            info!(
                "No device with usage_page=0x{:04X} found, trying {} fallback device(s)",
                DATA_CHANNEL_USAGE_PAGE,
                fallback.len()
            );

            let mut last_err = None;
            for dev_info in &fallback {
                let path = dev_info.path().to_string_lossy().into_owned();
                match Self::open_path(&api, &path) {
                    Ok(transport) => match transport.probe().await {
                        Ok(()) => {
                            info!("Probe succeeded on {path} — using as data channel");
                            return Ok(transport);
                        }
                        Err(e) => {
                            debug!("Probe failed on {path}: {e}");
                            last_err = Some(e);
                        }
                    },
                    Err(e) => {
                        debug!("Failed to open {path}: {e}");
                        last_err = Some(e);
                    }
                }
            }

            return Err(last_err.unwrap_or_else(|| {
                TransportError::ConnectionFailed("No K9-Pad USB device responded to probe".into())
            }));
        }

        info!(
            "Found {} K9-Pad data channel device(s) (usage_page=0x{:04X})",
            candidates.len(),
            DATA_CHANNEL_USAGE_PAGE
        );

        let mut last_err = None;
        for dev_info in &candidates {
            let path = dev_info.path().to_string_lossy().into_owned();
            match Self::open_path(&api, &path) {
                Ok(transport) => match transport.probe().await {
                    Ok(()) => {
                        info!("Probe succeeded on {path} — using as data channel");
                        return Ok(transport);
                    }
                    Err(e) => {
                        debug!("Probe failed on {path}: {e}");
                        last_err = Some(e);
                    }
                },
                Err(e) => {
                    debug!("Failed to open {path}: {e}");
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            TransportError::ConnectionFailed("No K9-Pad USB device responded to probe".into())
        }))
    }

    /// Open a HID device by its system path.
    fn open_path(api: &HidApi, path: &str) -> Result<Self, TransportError> {
        let device = api
            .open_path(&std::ffi::CString::new(path).unwrap())
            .map_err(|e| TransportError::ConnectionFailed(format!("HID open {path}: {e}")))?;

        // Set non-blocking mode off (we use timeouts instead)
        device
            .set_blocking_mode(true)
            .map_err(|e| TransportError::ConnectionFailed(format!("Set blocking: {e}")))?;

        info!("Opened HID device: {path}");

        Ok(Self {
            device: Arc::new(Mutex::new(device)),
            connected: AtomicBool::new(true),
        })
    }

    /// Verify the device speaks the K9 data channel protocol via PING/PONG handshake.
    pub async fn probe(&self) -> Result<(), TransportError> {
        // Send PING
        let mut buf = [0u8; k9_datachannel_proto::MAX_PACKET_SIZE];
        let n = k9_datachannel_proto::build_ping(&mut buf)
            .ok_or_else(|| TransportError::ConnectionFailed("Failed to build ping".into()))?;
        self.send(&buf[..n]).await?;

        // Wait for PONG
        let response = self.receive().await?;
        let header = k9_datachannel_proto::PacketHeader::decode(&response).map_err(|e| {
            TransportError::ConnectionFailed(format!("Invalid probe response: {e:?}"))
        })?;

        if header.cmd != k9_datachannel_proto::CommandId::Pong {
            return Err(TransportError::ConnectionFailed(format!(
                "Expected Pong, got {:?}",
                header.cmd
            )));
        }

        debug!("USB probe: PING/PONG successful");
        Ok(())
    }
}

impl Transport for UsbTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::NotConnected);
        }
        let device = self.device.clone();
        let mut report = [0u8; 65]; // report ID (0x00) + 64 bytes data
        let len = data.len().min(64);
        report[1..1 + len].copy_from_slice(&data[..len]);

        tokio::task::spawn_blocking(move || {
            let device = device
                .lock()
                .map_err(|_| TransportError::SendFailed("Lock poisoned".into()))?;
            device
                .write(&report)
                .map_err(|e| TransportError::SendFailed(format!("{e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| TransportError::SendFailed(format!("Task join: {e}")))?
    }

    async fn receive(&self) -> Result<Vec<u8>, TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::NotConnected);
        }
        let device = self.device.clone();
        tokio::task::spawn_blocking(move || {
            let device = device
                .lock()
                .map_err(|_| TransportError::ReceiveFailed("Lock poisoned".into()))?;
            let mut buf = [0u8; 64];
            let n = device
                .read_timeout(&mut buf, READ_TIMEOUT_MS)
                .map_err(|e| TransportError::ReceiveFailed(format!("HID read: {e}")))?;

            if n == 0 {
                return Err(TransportError::Timeout);
            }

            // Parse header to determine actual packet length
            if n < k9_datachannel_proto::HEADER_SIZE {
                return Err(TransportError::ReceiveFailed(format!(
                    "Short read: {n} bytes, need at least {} for header",
                    k9_datachannel_proto::HEADER_SIZE
                )));
            }

            let header = k9_datachannel_proto::PacketHeader::decode(&buf).map_err(|e| {
                TransportError::ReceiveFailed(format!("Header decode: {e:?}"))
            })?;

            let total_len = k9_datachannel_proto::HEADER_SIZE + header.payload_len as usize;
            if total_len > n {
                return Err(TransportError::ReceiveFailed(format!(
                    "Packet claims {total_len} bytes but only read {n}"
                )));
            }

            Ok(buf[..total_len].to_vec())
        })
        .await
        .map_err(|e| TransportError::ReceiveFailed(format!("Task join: {e}")))?
    }

    async fn disconnect(&self) -> Result<(), TransportError> {
        self.connected.store(false, Ordering::Relaxed);
        // HidDevice is dropped when the Arc is dropped
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}
