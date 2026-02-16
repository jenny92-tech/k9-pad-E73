// INPUT:  reqwest, serde, tokio, k9-datachannel-proto
// OUTPUT: BilibiliProvider
// POS:    B站粉丝数据提供者
use log::{debug, warn};
use serde::Deserialize;
use tokio::sync::mpsc;

use super::{DisplayData, DisplayUpdate, Provider};
use k9_datachannel_proto::function_bits;

/// Provider that polls the Bilibili API for a user's follower count.
pub struct BilibiliProvider {
    slot: u8,
    uid: u64,
    interval_secs: u64,
}

#[derive(Debug, Deserialize)]
struct BiliStatResponse {
    code: i32,
    data: Option<BiliStatData>,
}

#[derive(Debug, Deserialize)]
struct BiliStatData {
    follower: i64,
}

impl BilibiliProvider {
    pub fn new(slot: u8, uid: u64, interval_secs: u64) -> Self {
        Self {
            slot,
            uid,
            interval_secs,
        }
    }
}

impl Provider for BilibiliProvider {
    fn name(&self) -> &str {
        "bilibili"
    }

    fn function_bit(&self) -> u16 {
        function_bits::SUBSCRIBERS
    }

    async fn start(&mut self, tx: mpsc::Sender<DisplayUpdate>) -> anyhow::Result<()> {
        let client = reqwest::Client::builder()
            .user_agent("K9-Host/0.1")
            .build()?;

        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(self.interval_secs));

        loop {
            interval.tick().await;

            let url = format!(
                "https://api.bilibili.com/x/relation/stat?vmid={}",
                self.uid
            );

            match client.get(&url).send().await {
                Ok(resp) => match resp.json::<BiliStatResponse>().await {
                    Ok(stat) => {
                        if stat.code == 0 {
                            if let Some(data) = stat.data {
                                debug!("Bilibili follower count: {}", data.follower);
                                let _ = tx
                                    .send(DisplayUpdate {
                                        slot: self.slot,
                                        data: DisplayData::Numeric(data.follower as i32),
                                    })
                                    .await;
                            }
                        } else {
                            warn!("Bilibili API error code: {}", stat.code);
                        }
                    }
                    Err(e) => warn!("Failed to parse Bilibili response: {e}"),
                },
                Err(e) => warn!("Bilibili request failed: {e}"),
            }
        }
    }
}
