use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use btleplug::api::{
    Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
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

/// BLE transport using btleplug.
pub struct BleTransport {
    peripheral: Peripheral,
    rx_char: Characteristic,
    #[allow(dead_code)]
    tx_char: Characteristic,
    connected: AtomicBool,
    recv_buf: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl BleTransport {
    /// Scan for a K9-Pad device and connect to it.
    ///
    /// `timeout` controls how long to scan before giving up.
    pub async fn connect(timeout: Duration) -> Result<Self, TransportError> {
        let manager = Manager::new()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("BLE manager init: {e}")))?;

        let adapters = manager
            .adapters()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("No BLE adapters: {e}")))?;

        let adapter = adapters
            .into_iter()
            .next()
            .ok_or_else(|| TransportError::ConnectionFailed("No BLE adapter found".into()))?;

        let peripheral = Self::scan_and_find(&adapter, timeout).await?;

        peripheral
            .connect()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Connect failed: {e}")))?;

        peripheral
            .discover_services()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Service discovery: {e}")))?;

        let (rx_char, tx_char) = Self::find_characteristics(&peripheral)?;

        // Subscribe to TX notifications
        peripheral
            .subscribe(&tx_char)
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Subscribe failed: {e}")))?;

        let recv_buf = Arc::new(Mutex::new(Vec::new()));

        // Spawn notification listener
        let recv_buf_clone = recv_buf.clone();
        let notif_stream = peripheral
            .notifications()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Notifications: {e}")))?;

        tokio::spawn(async move {
            let mut stream = notif_stream;
            while let Some(notification) = stream.next().await {
                let mut buf = recv_buf_clone.lock().await;
                buf.push(notification.value);
            }
        });

        info!("Connected to K9-Pad via BLE");

        Ok(Self {
            peripheral,
            rx_char,
            tx_char,
            connected: AtomicBool::new(true),
            recv_buf,
        })
    }

    async fn scan_and_find(
        adapter: &Adapter,
        timeout: Duration,
    ) -> Result<Peripheral, TransportError> {
        info!("Scanning for K9-Pad BLE device...");

        adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Scan start: {e}")))?;

        let mut events = adapter
            .events()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Adapter events: {e}")))?;

        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, events.next()).await {
                Ok(Some(CentralEvent::DeviceDiscovered(id))) => {
                    if let Ok(peripheral) = adapter.peripheral(&id).await {
                        if let Ok(Some(props)) = peripheral.properties().await {
                            let name = props.local_name.unwrap_or_default();
                            debug!("Found device: {name}");
                            if name.contains("K9") || name.contains("k9") {
                                let _ = adapter.stop_scan().await;
                                return Ok(peripheral);
                            }
                        }
                    }
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }

        let _ = adapter.stop_scan().await;
        Err(TransportError::Timeout)
    }

    fn find_characteristics(
        peripheral: &Peripheral,
    ) -> Result<(Characteristic, Characteristic), TransportError> {
        let chars = peripheral.characteristics();
        let rx = chars
            .iter()
            .find(|c| c.uuid == K9_RX_CHAR_UUID)
            .cloned()
            .ok_or_else(|| {
                TransportError::ConnectionFailed("RX characteristic not found".into())
            })?;
        let tx = chars
            .iter()
            .find(|c| c.uuid == K9_TX_CHAR_UUID)
            .cloned()
            .ok_or_else(|| {
                TransportError::ConnectionFailed("TX characteristic not found".into())
            })?;
        Ok((rx, tx))
    }
}

impl Transport for BleTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::NotConnected);
        }
        self.peripheral
            .write(&self.rx_char, data, WriteType::WithResponse)
            .await
            .map_err(|e| TransportError::SendFailed(format!("{e}")))
    }

    async fn receive(&self) -> Result<Vec<u8>, TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
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
            if tokio::time::Instant::now() >= deadline {
                return Err(TransportError::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn disconnect(&self) -> Result<(), TransportError> {
        self.connected.store(false, Ordering::Relaxed);
        self.peripheral
            .disconnect()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Disconnect: {e}")))
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}
