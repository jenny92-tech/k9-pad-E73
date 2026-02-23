// INPUT:  bluest (BLE), shared-datachannel-proto UUIDs
// OUTPUT: BleTransport implementing Transport trait
// POS:    BLE transport layer — handles discovery (connected + scan) and GATT I/O

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bluest::{Adapter, Characteristic, Device};
use futures::StreamExt;
use log::{debug, info};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{Transport, TransportError};

/// Custom BLE service UUID for K9-Pad data channel.
/// Must match the UUID in RMK's data_channel_service.rs.
pub const K9_SERVICE_UUID: Uuid = Uuid::from_u128(0xe9dc0001_7374_7265_616d_6b3970616400);

/// Characteristic UUID for host -> device writes (RX from device perspective).
pub const K9_RX_CHAR_UUID: Uuid = Uuid::from_u128(0xe9dc0002_7374_7265_616d_6b3970616400);

/// Characteristic UUID for device -> host notifications (TX from device perspective).
pub const K9_TX_CHAR_UUID: Uuid = Uuid::from_u128(0xe9dc0003_7374_7265_616d_6b3970616400);

/// Maximum number of buffered BLE notifications before dropping new ones.
const MAX_RECV_BUF_SIZE: usize = 32;

/// BLE transport using bluest.
///
/// Supports both already-connected devices (via `retrieveConnectedPeripherals` on macOS)
/// and scanning for new devices.
pub struct BleTransport {
    adapter: Adapter,
    device: Device,
    rx_char: Characteristic,
    connected: AtomicBool,
    /// Whether the notification stream task is still alive.
    stream_alive: Arc<AtomicBool>,
    recv_buf: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl BleTransport {
    /// Find a K9-Pad device and connect to it.
    ///
    /// First checks already-connected peripherals (via service UUID),
    /// then falls back to scanning for `timeout` duration.
    pub async fn connect(timeout: Duration) -> Result<Self, TransportError> {
        let adapter = Adapter::default()
            .await
            .ok_or_else(|| TransportError::ConnectionFailed("No BLE adapter found".into()))?;

        adapter
            .wait_available()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Adapter not available: {e}")))?;

        // Try already-connected devices first (handles paired keyboards on macOS)
        let device = match Self::find_connected(&adapter).await {
            Some(d) => d,
            None => {
                info!("No already-connected K9-Pad found, scanning...");
                Self::scan_and_find(&adapter, timeout).await?
            }
        };

        // Connect if not already connected
        if !device.is_connected().await {
            adapter
                .connect_device(&device)
                .await
                .map_err(|e| TransportError::ConnectionFailed(format!("Connect failed: {e}")))?;
        }

        // Discover services and characteristics
        let services = device
            .discover_services()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Service discovery: {e}")))?;

        let k9_service = services
            .iter()
            .find(|s| s.uuid() == K9_SERVICE_UUID)
            .ok_or_else(|| {
                TransportError::ConnectionFailed("K9 data channel service not found".into())
            })?;

        let chars = k9_service.discover_characteristics().await.map_err(|e| {
            TransportError::ConnectionFailed(format!("Characteristic discovery: {e}"))
        })?;

        let rx_char = chars
            .iter()
            .find(|c| c.uuid() == K9_RX_CHAR_UUID)
            .cloned()
            .ok_or_else(|| {
                TransportError::ConnectionFailed("RX characteristic not found".into())
            })?;

        let tx_char = chars
            .iter()
            .find(|c| c.uuid() == K9_TX_CHAR_UUID)
            .cloned()
            .ok_or_else(|| {
                TransportError::ConnectionFailed("TX characteristic not found".into())
            })?;

        // Subscribe to TX notifications — tx_char is moved into the spawned task
        let recv_buf = Arc::new(Mutex::new(Vec::new()));
        let recv_buf_clone = recv_buf.clone();
        let stream_alive = Arc::new(AtomicBool::new(true));
        let stream_alive_clone = stream_alive.clone();

        tokio::spawn(async move {
            match tx_char.notify().await {
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(data) => {
                                let mut buf = recv_buf_clone.lock().await;
                                if buf.len() < MAX_RECV_BUF_SIZE {
                                    buf.push(data);
                                } else {
                                    debug!("Notification buffer full, dropping message");
                                }
                            }
                            Err(e) => {
                                debug!("Notification stream error: {e}");
                                break;
                            }
                        }
                    }
                    debug!("Notification stream ended");
                }
                Err(e) => {
                    debug!("Failed to subscribe to TX notifications: {e}");
                }
            }
            stream_alive_clone.store(false, Ordering::SeqCst);
        });

        info!("Connected to K9-Pad via BLE");

        Ok(Self {
            adapter,
            device,
            rx_char,
            connected: AtomicBool::new(true),
            stream_alive,
            recv_buf,
        })
    }

    /// Standard BLE HID service UUID (0x1812).
    /// Used to find keyboards already connected to the OS.
    const BLE_HID_SERVICE_UUID: Uuid = Uuid::from_u128(0x1812);

    /// Check already-connected peripherals for a K9-Pad device.
    ///
    /// Tries two strategies:
    /// 1. By K9 data channel service UUID (if previously discovered)
    /// 2. By BLE HID service UUID + name matching (for OS-paired keyboards)
    async fn find_connected(adapter: &Adapter) -> Option<Device> {
        info!("Checking for already-connected K9-Pad...");

        // Strategy 1: Find by K9 custom service UUID
        if let Ok(devices) = adapter
            .connected_devices_with_services(&[K9_SERVICE_UUID])
            .await
        {
            info!("Found {} device(s) with K9 service", devices.len());
            if let Some(device) = devices.into_iter().next() {
                let name = device.name().unwrap_or_default();
                info!("Found K9-Pad by service UUID: {name}");
                return Some(device);
            }
        }

        // Strategy 2: Find by HID service UUID + name matching
        if let Ok(devices) = adapter
            .connected_devices_with_services(&[Self::BLE_HID_SERVICE_UUID])
            .await
        {
            info!(
                "Found {} connected HID device(s), filtering by name...",
                devices.len()
            );
            for device in devices {
                let name = device.name().unwrap_or_default();
                debug!("  HID device: {name}");
                if name.contains("K9") || name.contains("k9") {
                    info!("Found K9-Pad by HID service + name: {name}");
                    return Some(device);
                }
            }
        }

        None
    }

    /// Scan for a K9-Pad device by name.
    async fn scan_and_find(adapter: &Adapter, timeout: Duration) -> Result<Device, TransportError> {
        info!("Scanning for K9-Pad BLE device...");

        let mut scan = adapter
            .scan(&[])
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Scan start: {e}")))?;

        let result = tokio::time::timeout(timeout, async {
            while let Some(adv_device) = scan.next().await {
                let name = adv_device.device.name().unwrap_or_default();
                debug!("Found device: {name}");
                if name.contains("K9") || name.contains("k9") {
                    return Some(adv_device.device);
                }
            }
            None
        })
        .await;

        match result {
            Ok(Some(device)) => Ok(device),
            _ => Err(TransportError::Timeout),
        }
    }
}

impl Transport for BleTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::NotConnected);
        }
        self.rx_char
            .write_without_response(data)
            .await
            .map_err(|e| TransportError::SendFailed(format!("{e}")))
    }

    async fn receive(&self) -> Result<Vec<u8>, TransportError> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }
        // Poll the notification buffer with a timeout
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            {
                let mut buf = self.recv_buf.lock().await;
                if !buf.is_empty() {
                    return Ok(buf.remove(0));
                }
            }
            // Check if the notification stream died while we were waiting
            if !self.stream_alive.load(Ordering::SeqCst) {
                return Err(TransportError::ReceiveFailed(
                    "BLE notification stream terminated".into(),
                ));
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(TransportError::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn disconnect(&self) -> Result<(), TransportError> {
        self.connected.store(false, Ordering::Relaxed);
        self.adapter
            .disconnect_device(&self.device)
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Disconnect: {e}")))
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed) && self.stream_alive.load(Ordering::SeqCst)
    }
}
