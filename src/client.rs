//! HTTP client for the DATAJUD public API.
//!
//! The API exposes, per tribunal index `api_publica_<alias>`:
//! * `POST /_search` — full query DSL (`query`/`size`/`sort`/`search_after`/`aggs`;
//!   Elasticsearch-compatible syntax as documented by the CNJ).
//! * `POST /_count`  — document counts.
//!
//! Authentication: `Authorization: APIKey <public key>`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::config::Config;
use crate::error::DatajudError;

/// HTTP client with authentication, timeout and retry handling.
#[derive(Debug, Clone)]
pub struct DatajudClient {
    /// Shared HTTP client (connection pool, timeout, user agent).
    http: reqwest::Client,
    /// Effective configuration (API key, base URL, timeout, retries).
    config: Arc<Config>,
}

impl DatajudClient {
    /// Builds a client from the effective configuration.
    ///
    /// # Errors
    ///
    /// Returns [`DatajudError::Network`] when the underlying HTTP client
    /// cannot be constructed (TLS backend failure).
    #[inline]
    pub fn new(config: Config) -> Result<Self, DatajudError> {
        let http = match reqwest::Client::builder()
            .timeout(config.timeout)
            .connect_timeout(Duration::from_secs(20))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(concat!("datajud-mcp/", env!("CARGO_PKG_VERSION")))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                return Err(DatajudError::Network {
                    message: format!("failed to build HTTP client: {error}"),
                });
            }
        };
        Ok(Self {
            http,
            config: Arc::new(config),
        })
    }

    /// Returns the effective configuration.
    #[must_use]
    #[inline]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// `POST {base}/{index}/_search` — returns the raw JSON response.
    ///
    /// # Errors
    ///
    /// Returns [`DatajudError::Network`] on transport failures,
    /// [`DatajudError::Http`] on non-success status codes and
    /// [`DatajudError::InvalidJson`] on unparsable responses.
    #[inline]
    pub async fn search(&self, index: &str, body: &Value) -> Result<Value, DatajudError> {
        let url = format!("{}/{}/_search", self.config.base_url, index);
        return self.post_json(&url, body).await;
    }

    /// `POST {base}/{index}/_count` — returns `{"count": N, ...}`.
    ///
    /// # Errors
    ///
    /// Same failure modes as [`Self::search`].
    #[inline]
    pub async fn count(&self, index: &str, body: &Value) -> Result<Value, DatajudError> {
        let url = format!("{}/{}/_count", self.config.base_url, index);
        return self.post_json(&url, body).await;
    }

    /// `POST {base}/{index}/_search` with `size: 0` and `match_all` — cheap liveness probe.
    ///
    /// Returns the raw response together with the elapsed wall-clock time.
    ///
    /// # Errors
    ///
    /// Same failure modes as [`Self::search`].
    #[inline]
    pub async fn ping(&self, index: &str) -> Result<(Value, Duration), DatajudError> {
        let started = Instant::now();
        let body = json!({ "size": 0_u64, "query": { "match_all": {} } });
        let response = self.search(index, &body).await?;
        Ok((response, started.elapsed()))
    }

    /// Posts a JSON body with retry/backoff handling for retryable failures.
    async fn post_json(&self, url: &str, body: &Value) -> Result<Value, DatajudError> {
        let mut attempt = 0_u32;
        loop {
            match self.post_json_once(url, body).await {
                Ok(value) => return Ok(value),
                Err(error) if attempt < self.config.max_retries && is_retryable(&error) => {
                    let backoff = Duration::from_millis(800_u64.saturating_mul(2_u64.pow(attempt)));
                    warn!(
                        attempt,
                        wait_ms = u64::try_from(backoff.as_millis()).unwrap_or(u64::MAX),
                        error = %error,
                        "retryable DATAJUD failure — backing off"
                    );
                    sleep(backoff).await;
                    attempt = attempt.saturating_add(1);
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Single POST attempt without retries.
    async fn post_json_once(&self, url: &str, body: &Value) -> Result<Value, DatajudError> {
        debug!(%url, "DATAJUD request");
        let response = match self
            .http
            .post(url)
            .header("Authorization", format!("APIKey {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => return Err(error.into()),
        };

        let status = response.status();
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => return Err(error.into()),
        };

        if !status.is_success() {
            return Err(DatajudError::Http {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }

        serde_json::from_slice(&bytes).map_err(|error| {
            let snippet: String = String::from_utf8_lossy(&bytes).chars().take(200).collect();
            DatajudError::InvalidJson {
                message: format!("{error}; body starts with: {snippet}"),
            }
        })
    }
}

/// Network errors and 429/5xx responses are worth retrying.
const fn is_retryable(error: &DatajudError) -> bool {
    match *error {
        DatajudError::Network { .. } | DatajudError::InvalidJson { .. } => true,
        DatajudError::Http { status, .. } => matches!(status, 429 | 500..=599),
        DatajudError::UnknownTribunal { .. } | DatajudError::InvalidInput { .. } => false,
    }
}
