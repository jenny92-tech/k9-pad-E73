use serde::Deserialize;
use std::path::PathBuf;

use super::error::AiQuotaError;

// ---------------------------------------------------------------------------
// Claude Code credentials
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ClaudeCredFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: ClaudeOAuth,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeOAuth {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
}

impl ClaudeOAuth {
    /// Returns true if the token has not yet expired.
    /// `expires_at` is milliseconds since epoch.
    pub fn is_valid(&self) -> bool {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.expires_at > now_ms
    }
}

/// Read Claude Code OAuth credentials.
///
/// Strategy:
/// 1. Try `~/.claude/.credentials.json` (file-based storage).
/// 2. Fallback to macOS Keychain (`Claude Code-credentials` service).
pub fn read_claude_credentials() -> Result<ClaudeOAuth, AiQuotaError> {
    // Attempt 1: file-based credentials
    if let Some(home) = dirs::home_dir() {
        let cred_path: PathBuf = home.join(".claude").join(".credentials.json");
        if cred_path.exists() {
            let data = std::fs::read_to_string(&cred_path).map_err(|e| {
                AiQuotaError::CredentialNotFound(format!("{}: {e}", cred_path.display()))
            })?;
            let parsed: ClaudeCredFile = serde_json::from_str(&data)
                .map_err(|e| AiQuotaError::CredentialParse(format!("credentials.json: {e}")))?;
            return Ok(parsed.claude_ai_oauth);
        }
    }

    // Attempt 2: macOS Keychain
    #[cfg(target_os = "macos")]
    {
        return read_claude_from_keychain();
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(AiQuotaError::CredentialNotFound(
            "no ~/.claude/.credentials.json and keychain not available on this platform".into(),
        ))
    }
}

/// Read Claude Code credentials from macOS Keychain.
#[cfg(target_os = "macos")]
fn read_claude_from_keychain() -> Result<ClaudeOAuth, AiQuotaError> {
    let output = std::process::Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .map_err(|e| AiQuotaError::Keychain(format!("failed to run security command: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AiQuotaError::Keychain(format!(
            "security command failed: {stderr}"
        )));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json_str = json_str.trim();

    let parsed: ClaudeCredFile = serde_json::from_str(json_str)
        .map_err(|e| AiQuotaError::CredentialParse(format!("keychain JSON: {e}")))?;

    Ok(parsed.claude_ai_oauth)
}

// ---------------------------------------------------------------------------
// Codex CLI credentials
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    tokens: Option<CodexTokens>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodexTokens {
    pub access_token: String,
    pub account_id: Option<String>,
}

/// Read Codex CLI credentials from `~/.codex/auth.json`.
pub fn read_codex_credentials() -> Result<CodexTokens, AiQuotaError> {
    let home = dirs::home_dir()
        .ok_or_else(|| AiQuotaError::CredentialNotFound("cannot determine home dir".into()))?;

    let auth_path = home.join(".codex").join("auth.json");
    if !auth_path.exists() {
        return Err(AiQuotaError::CredentialNotFound(format!(
            "{} not found",
            auth_path.display()
        )));
    }

    let data = std::fs::read_to_string(&auth_path)
        .map_err(|e| AiQuotaError::CredentialNotFound(format!("{}: {e}", auth_path.display())))?;

    let parsed: CodexAuthFile = serde_json::from_str(&data)
        .map_err(|e| AiQuotaError::CredentialParse(format!("auth.json: {e}")))?;

    parsed
        .tokens
        .ok_or_else(|| AiQuotaError::CredentialParse("auth.json: missing 'tokens' field".into()))
}
