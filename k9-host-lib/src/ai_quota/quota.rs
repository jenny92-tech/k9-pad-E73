// INPUT:  reqwest (HTTP client), ClaudeOAuth/CodexTokens credentials, serde (JSON deserialize)
// OUTPUT: fetch_claude_quota(), fetch_codex_quota() -> QuotaInfo (utilization percentage)
// POS:    Quota fetcher — calls Claude/Codex usage APIs and normalizes results into QuotaInfo

use serde::Deserialize;

use super::credentials::{ClaudeOAuth, CodexTokens};
use super::error::AiQuotaError;

// ---------------------------------------------------------------------------
// Claude usage API
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ClaudeUsageResponse {
    /// List of rate-limit windows with utilization info.
    windows: Vec<UsageWindow>,
}

#[derive(Debug, Deserialize)]
struct UsageWindow {
    /// e.g. "five_hour", "seven_day"
    #[allow(dead_code)]
    name: String,
    /// Utilization as a fraction 0.0 – 1.0.
    utilization: f64,
}

/// Fetched quota information for a single AI tool.
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    /// Tool name (e.g. "claude", "codex").
    pub tool: String,
    /// Utilization percentage 0–100 of the tightest (most-used) window.
    pub utilization_pct: f64,
}

impl QuotaInfo {
    /// Convert to a 0-100 progress value for the OLED bar.
    pub fn as_progress(&self) -> u8 {
        (self.utilization_pct.clamp(0.0, 100.0)) as u8
    }

    /// Human-readable string for text display.
    pub fn as_display_text(&self) -> String {
        format!("{} {:.0}%", self.tool, self.utilization_pct)
    }
}

/// Fetch Claude Code subscription quota usage.
///
/// Calls `GET https://api.anthropic.com/api/oauth/usage` with the OAuth bearer token.
pub async fn fetch_claude_quota(cred: &ClaudeOAuth) -> Result<QuotaInfo, AiQuotaError> {
    if !cred.is_valid() {
        return Err(AiQuotaError::CredentialParse("token expired".into()));
    }

    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(&cred.access_token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AiQuotaError::ApiResponse(format!("HTTP {status}: {body}")));
    }

    let usage: ClaudeUsageResponse = resp.json().await?;

    // Pick the window with the highest utilization (tightest constraint).
    let max_util = usage
        .windows
        .iter()
        .map(|w| w.utilization)
        .fold(0.0_f64, f64::max);

    Ok(QuotaInfo {
        tool: "claude".into(),
        utilization_pct: max_util * 100.0,
    })
}

/// Fetch Codex CLI quota usage (best-effort).
///
/// Calls `GET https://chatgpt.com/backend-api/wham/usage` with the OAuth bearer token.
/// This endpoint may change; failures are non-fatal.
pub async fn fetch_codex_quota(tokens: &CodexTokens) -> Result<QuotaInfo, AiQuotaError> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(&tokens.access_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AiQuotaError::ApiResponse(format!("HTTP {status}: {body}")));
    }

    // The response shape is best-effort — try to extract a utilization number.
    let body: serde_json::Value = resp.json().await?;

    // Expected shape: { "usage_pct": <number> } or similar.
    // Fall back to 0 if unparseable (non-fatal).
    let pct = body
        .get("usage_pct")
        .and_then(|v| v.as_f64())
        .or_else(|| {
            body.get("utilization")
                .and_then(|v| v.as_f64())
                .map(|u| u * 100.0)
        })
        .unwrap_or(0.0);

    Ok(QuotaInfo {
        tool: "codex".into(),
        utilization_pct: pct,
    })
}
