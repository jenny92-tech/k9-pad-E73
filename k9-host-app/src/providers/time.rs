// INPUT:  chrono, tokio, k9-datachannel-proto
// OUTPUT: TimeProvider
// POS:    时间数据提供者
use chrono::Local;
use log::debug;
use tokio::sync::mpsc;

use super::{DisplayData, DisplayUpdate, Provider};
use k9_datachannel_proto::function_bits;

/// Provider that pushes the current time to a display slot.
pub struct TimeProvider {
    slot: u8,
    format: String,
}

impl TimeProvider {
    pub fn new(slot: u8, format: String) -> Self {
        Self { slot, format }
    }
}

impl Provider for TimeProvider {
    fn name(&self) -> &str {
        "time"
    }

    fn function_bit(&self) -> u16 {
        function_bits::TIME
    }

    async fn start(&mut self, tx: mpsc::Sender<DisplayUpdate>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

        loop {
            interval.tick().await;

            let now = Local::now();
            let time_str = now.format(&self.format).to_string();
            debug!("Time provider: {time_str}");

            if tx
                .send(DisplayUpdate {
                    slot: self.slot,
                    data: DisplayData::Text(time_str),
                })
                .await
                .is_err()
            {
                break;
            }
        }

        Ok(())
    }
}
