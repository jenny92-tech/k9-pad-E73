// INPUT:  tokio, k9-datachannel-proto
// OUTPUT: VolumeProvider
// POS:    系统音量数据提供者
use log::debug;
use tokio::sync::mpsc;

use super::{DisplayData, DisplayUpdate, Provider};
use k9_datachannel_proto::function_bits;

/// Provider that monitors system volume and pushes it as a progress value.
pub struct VolumeProvider {
    slot: u8,
}

impl VolumeProvider {
    pub fn new(slot: u8) -> Self {
        Self { slot }
    }

    /// Get the current system volume as a percentage (0-100).
    #[cfg(target_os = "macos")]
    async fn get_volume() -> Option<u8> {
        let output = tokio::process::Command::new("osascript")
            .args(["-e", "output volume of (get volume settings)"])
            .output()
            .await
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.trim().parse::<u8>().ok()
    }

    #[cfg(not(target_os = "macos"))]
    async fn get_volume() -> Option<u8> {
        log::warn!("Volume monitoring not implemented for this platform");
        None
    }
}

impl Provider for VolumeProvider {
    fn name(&self) -> &str {
        "volume"
    }

    fn function_bit(&self) -> u16 {
        function_bits::VOLUME
    }

    async fn start(&mut self, tx: mpsc::Sender<DisplayUpdate>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        let mut last_volume: Option<u8> = None;

        loop {
            interval.tick().await;

            if let Some(volume) = Self::get_volume().await {
                if last_volume != Some(volume) {
                    debug!("Volume changed: {volume}%");
                    last_volume = Some(volume);

                    if tx
                        .send(DisplayUpdate {
                            slot: self.slot,
                            data: DisplayData::Progress(volume),
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}
