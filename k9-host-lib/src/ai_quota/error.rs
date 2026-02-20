use std::fmt;

#[derive(Debug)]
pub enum AiQuotaError {
    /// Credential file not found or unreadable.
    CredentialNotFound(String),
    /// Failed to parse credential data.
    CredentialParse(String),
    /// HTTP request to quota API failed.
    Request(reqwest::Error),
    /// Quota API returned unexpected response.
    ApiResponse(String),
    /// Keychain access failed (macOS).
    Keychain(String),
}

impl fmt::Display for AiQuotaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CredentialNotFound(msg) => write!(f, "credential not found: {msg}"),
            Self::CredentialParse(msg) => write!(f, "credential parse error: {msg}"),
            Self::Request(e) => write!(f, "HTTP request failed: {e}"),
            Self::ApiResponse(msg) => write!(f, "API response error: {msg}"),
            Self::Keychain(msg) => write!(f, "keychain error: {msg}"),
        }
    }
}

impl std::error::Error for AiQuotaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for AiQuotaError {
    fn from(e: reqwest::Error) -> Self {
        Self::Request(e)
    }
}
