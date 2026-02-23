// INPUT:  credentials, error, quota sub-modules
// OUTPUT: Public API re-exports — read_claude/codex_credentials, fetch_claude/codex_quota, QuotaInfo, AiQuotaError
// POS:    AI quota module root — facade for credential reading and quota fetching

pub mod credentials;
pub mod error;
pub mod quota;

pub use credentials::{read_claude_credentials, read_codex_credentials};
pub use error::AiQuotaError;
pub use quota::{fetch_claude_quota, fetch_codex_quota, QuotaInfo};
