//! Runtime configuration, loaded from environment variables.
//!
//! | Variable               | Required | Default                                  | Meaning             |
//! |------------------------|----------|------------------------------------------|---------------------|
//! | `DATAJUD_API_KEY`      | **yes**  | —                                        | `APIKey` credential |
//! | `DATAJUD_BASE_URL`     | no       | `https://api-publica.datajud.cnj.jus.br` | API base URL        |
//! | `DATAJUD_TIMEOUT_SECS` | no       | `180`                                    | per-request timeout |
//! | `DATAJUD_MAX_RETRIES`  | no       | `2`                                      | retries on failures |
//!
//! The API key is deliberately **not** embedded in the code or passed as a
//! tool parameter: configuration belongs in the environment (set once in the
//! MCP client's `env` block), never in model-visible arguments or transcripts.

use std::env;
use std::time::Duration;

use crate::error::DatajudError;

/// Error message returned when `DATAJUD_API_KEY` is missing or empty.
pub const MISSING_API_KEY_MESSAGE: &str = "DATAJUD_API_KEY is not set. Get the public API key from https://datajud-wiki.cnj.jus.br/api-publica/acesso and add it to your MCP client configuration, e.g. \"env\": { \"DATAJUD_API_KEY\": \"...\" }";

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
    /// Credential sent as `Authorization: APIKey <key>` (from `DATAJUD_API_KEY`).
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
            api_key: String::new(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }
}

impl Config {
    /// Builds the configuration from the environment.
    ///
    /// `DATAJUD_API_KEY` is required; the remaining variables override the
    /// defaults.
    ///
    /// # Errors
    ///
    /// Returns [`DatajudError::InvalidInput`] when `DATAJUD_API_KEY` is unset
    /// or empty.
    #[inline]
    pub fn from_env() -> Result<Self, DatajudError> {
        let mut config = Self::default();

        let Ok(api_key_raw) = env::var("DATAJUD_API_KEY") else {
            return Err(DatajudError::InvalidInput {
                message: MISSING_API_KEY_MESSAGE.to_owned(),
            });
        };
        let api_key = api_key_raw.trim().to_owned();
        if api_key.is_empty() {
            return Err(DatajudError::InvalidInput {
                message: MISSING_API_KEY_MESSAGE.to_owned(),
            });
        }
        config.api_key = api_key;

        if let Ok(base_url_raw) = env::var("DATAJUD_BASE_URL") {
            let base_url = base_url_raw.trim().trim_end_matches('/').to_owned();
            if !base_url.is_empty() {
                config.base_url = base_url;
            }
        }
        if let Ok(timeout_raw) = env::var("DATAJUD_TIMEOUT_SECS") {
            if let Ok(secs) = timeout_raw.trim().parse::<u64>() {
                config.timeout = Duration::from_secs(secs.clamp(5, 3600));
            }
        }
        if let Ok(retries_raw) = env::var("DATAJUD_MAX_RETRIES") {
            if let Ok(count) = retries_raw.trim().parse::<u32>() {
                config.max_retries = count.min(10);
            }
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_base_url_matches_wiki() {
        let config = Config::default();
        assert_eq!(config.base_url, DEFAULT_BASE_URL, "wiki base URL");
        assert!(config.api_key.is_empty(), "no API key is baked in");
    }

    #[test]
    fn missing_key_message_points_to_the_wiki() {
        assert!(
            MISSING_API_KEY_MESSAGE.contains("DATAJUD_API_KEY"),
            "the message must name the variable"
        );
        assert!(
            MISSING_API_KEY_MESSAGE.contains("datajud-wiki.cnj.jus.br"),
            "the message must link where to get the key"
        );
    }
}
