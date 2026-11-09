//! Runtime configuration, loaded from environment variables.
//!
//! | Variable               | Default                                   | Meaning              |
//! |------------------------|-------------------------------------------|----------------------|
//! | `DATAJUD_API_KEY`      | public key published in the DATAJUD wiki  | `APIKey` credential  |
//! | `DATAJUD_BASE_URL`     | `https://api-publica.datajud.cnj.jus.br`  | API base URL         |
//! | `DATAJUD_TIMEOUT_SECS` | `180`                                     | per-request timeout  |
//! | `DATAJUD_MAX_RETRIES`  | `2`                                       | retries on failures  |

use std::env;
use std::time::Duration;

/// Public API key published at <https://datajud-wiki.cnj.jus.br/api-publica/acesso>.
///
/// The CNJ may rotate this key at any time — override with `DATAJUD_API_KEY`.
pub const DEFAULT_API_KEY: &str = "cDZHYzlZa0JadVREZDJCendQbXY6SkJlTzNjLV9TRENyQk1RdnFKZGRQdw==";

/// Default base URL of the DATAJUD public API.
pub const DEFAULT_BASE_URL: &str = "https://api-publica.datajud.cnj.jus.br";

/// Default per-request timeout, in seconds. The public API is slow on large
/// tribunals, so this is deliberately generous.
pub const DEFAULT_TIMEOUT_SECS: u64 = 180;

/// Default number of retries for retryable failures (429/5xx/network).
pub const DEFAULT_MAX_RETRIES: u32 = 2;

/// Effective client configuration.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Credential sent as `Authorization: APIKey <key>`.
    pub api_key: String,
    /// Base URL of the public API (no trailing slash).
    pub base_url: String,
    /// Per-request timeout. The API is slow on large indices — keep this generous.
    pub timeout: Duration,
    /// How many times a retryable failure (429/5xx/network) is retried.
    pub max_retries: u32,
}

impl Default for Config {
    #[inline]
    fn default() -> Self {
        Self {
            api_key: DEFAULT_API_KEY.to_owned(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }
}

impl Config {
    /// Builds the configuration from the environment, falling back to the wiki defaults.
    #[must_use]
    #[inline]
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(raw) = env::var("DATAJUD_API_KEY") {
            let trimmed = raw.trim().to_owned();
            if !trimmed.is_empty() {
                config.api_key = trimmed;
            }
        }
        if let Ok(raw) = env::var("DATAJUD_BASE_URL") {
            let trimmed = raw.trim().trim_end_matches('/').to_owned();
            if !trimmed.is_empty() {
                config.base_url = trimmed;
            }
        }
        if let Ok(raw) = env::var("DATAJUD_TIMEOUT_SECS") {
            if let Ok(secs) = raw.trim().parse::<u64>() {
                config.timeout = Duration::from_secs(secs.clamp(5, 3600));
            }
        }
        if let Ok(raw) = env::var("DATAJUD_MAX_RETRIES") {
            if let Ok(count) = raw.trim().parse::<u32>() {
                config.max_retries = count.min(10);
            }
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_wiki() {
        let config = Config::default();
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert!(config.api_key.starts_with('c'));
    }
}
