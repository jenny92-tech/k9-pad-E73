// INPUT:  tokio, k9-host-lib (ai-quota), k9-datachannel-proto
// OUTPUT: AiQuotaProvider
// POS:    AI 工具订阅配额数据提供者 (Claude Code, Codex CLI)
use std::cmp::Ordering;

use log::{debug, warn};
use tokio::sync::mpsc;

use super::{DisplayData, DisplayUpdate, Provider};
use k9_datachannel_proto::function_bits;
use k9_host_lib::ai_quota;

/// Provider that monitors AI tool subscription quota usage.
///
/// Polls every 5 minutes, reads credentials from local storage,
/// fetches usage from the respective APIs, and sends the highest
/// utilization as a progress value (0-100) to the keyboard display.
pub struct AiQuotaProvider {
    slot: u8,
}

impl AiQuotaProvider {
    pub fn new(slot: u8) -> Self {
        Self { slot }
    }

    /// Attempt to fetch Claude Code quota; returns None on any error.
    async fn poll_claude() -> Option<ai_quota::QuotaInfo> {
        let cred = match ai_quota::read_claude_credentials() {
            Ok(c) => c,
            Err(e) => {
                debug!("claude credentials unavailable: {e}");
                return None;
            }
        };

        match ai_quota::fetch_claude_quota(&cred).await {
            Ok(info) => Some(info),
            Err(e) => {
                warn!("claude quota fetch failed: {e}");
                None
            }
        }
    }

    /// Attempt to fetch Codex CLI quota; returns None on any error.
    async fn poll_codex() -> Option<ai_quota::QuotaInfo> {
        let tokens = match ai_quota::read_codex_credentials() {
            Ok(t) => t,
            Err(e) => {
                debug!("codex credentials unavailable: {e}");
                return None;
            }
        };

        match ai_quota::fetch_codex_quota(&tokens).await {
            Ok(info) => Some(info),
            Err(e) => {
                warn!("codex quota fetch failed: {e}");
                None
            }
        }
    }
}

impl Provider for AiQuotaProvider {
    fn name(&self) -> &str {
        "ai_quota"
    }

    fn function_bit(&self) -> u16 {
        function_bits::AI_QUOTA
    }

    async fn start(&mut self, tx: mpsc::Sender<DisplayUpdate>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
        let mut last_progress: Option<u8> = None;

        loop {
            interval.tick().await;

            // Fetch both in parallel; use whichever succeeds.
            let (claude, codex) = tokio::join!(Self::poll_claude(), Self::poll_codex());

            // Pick the highest utilization among available tools.
            let best = [claude, codex].into_iter().flatten().max_by(|a, b| {
                a.utilization_pct
                    .partial_cmp(&b.utilization_pct)
                    .unwrap_or(Ordering::Equal)
            });

            if let Some(info) = best {
                let progress = info.as_progress();
                if last_progress != Some(progress) {
                    debug!("AI quota update: {}", info.as_display_text());
                    last_progress = Some(progress);

                    if tx
                        .send(DisplayUpdate {
                            slot: self.slot,
                            data: DisplayData::Progress(progress),
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
