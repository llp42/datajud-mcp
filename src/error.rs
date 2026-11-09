//! Error types for DATAJUD API access, with actionable hints for the caller.

use std::error::Error as StdError;
use std::fmt;

use crate::tribunals;

/// Errors produced while talking to the DATAJUD public API or validating input.
#[derive(Debug)]
#[non_exhaustive]
pub enum DatajudError {
    /// The HTTP request itself failed (DNS, TLS, timeout, reset).
    Network {
        /// Human-readable description of the transport failure.
        message: String,
    },
    /// The API answered with a non-success status code.
    Http {
        /// HTTP status code returned by the API.
        status: u16,
        /// Response body as returned by the API (may be an error page).
        body: String,
    },
    /// The response body was not the expected JSON.
    InvalidJson {
        /// Parsing failure description, including a body snippet.
        message: String,
    },
    /// A tribunal alias was not found in the catalog.
    UnknownTribunal {
        /// The alias that could not be resolved.
        alias: String,
    },
    /// Caller input is invalid (bad date, bad size, unknown field...).
    InvalidInput {
        /// Description of what is wrong and what is expected.
        message: String,
    },
}

impl DatajudError {
    /// A short, actionable hint to surface alongside the error through MCP.
    #[must_use]
    #[inline]
    pub const fn hint(&self) -> Option<&'static str> {
        match *self {
            Self::Network { .. } => Some(
                "Check internet connectivity. The DATAJUD API is slow on large tribunals (queries can take 30s+); consider raising DATAJUD_TIMEOUT_SECS.",
            ),
            Self::Http { status, .. } => match status {
                401 | 403 => Some(
                    "Authentication/authorization problem. The public key may have been rotated by the CNJ — get the current key at https://datajud-wiki.cnj.jus.br/api-publica/acesso and set DATAJUD_API_KEY.",
                ),
                404 => Some(
                    "Index not found — verify the tribunal alias (see the list_tribunals tool).",
                ),
                429 => Some(
                    "Rate limited by the API. Reduce request frequency and retry later (the terms of use prohibit abusive consumption).",
                ),
                500..=599 => Some(
                    "DATAJUD server-side error — retry later, or narrow the query (large match_all scans are expensive).",
                ),
                _ => None,
            },
            Self::InvalidJson { .. } => Some(
                "The API returned a non-JSON payload (possibly an HTML error page). Retry, or report the raw body.",
            ),
            Self::UnknownTribunal { .. } => Some(
                "Use the list_tribunals tool to see the 91 valid aliases (e.g. 'tjsp', 'trf1', 'tre-sp', 'tst').",
            ),
            Self::InvalidInput { .. } => {
                Some("Fix the input according to the tool schema and the list_fields glossary.")
            }
        }
    }
}

impl fmt::Display for DatajudError {
    #[inline]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network { message } => write!(formatter, "network error: {message}"),
            Self::Http { status, body } => {
                let snippet: String = body.chars().take(600).collect();
                write!(formatter, "DATAJUD API returned HTTP {status}: {snippet}")
            }
            Self::InvalidJson { message } => write!(formatter, "invalid JSON response: {message}"),
            Self::UnknownTribunal { alias } => {
                write!(formatter, "unknown tribunal alias: '{alias}'")?;
                tribunals::suggest(alias).map_or(Ok(()), |suggestion| {
                    write!(formatter, " — did you mean '{}'?", suggestion.alias)
                })
            }
            Self::InvalidInput { message } => write!(formatter, "invalid input: {message}"),
        }
    }
}

impl StdError for DatajudError {}

impl From<reqwest::Error> for DatajudError {
    #[inline]
    fn from(value: reqwest::Error) -> Self {
        if value.is_timeout() {
            return Self::Network {
                message: format!("request timed out: {value}"),
            };
        }
        Self::Network {
            message: value.to_string(),
        }
    }
}

impl From<serde_json::Error> for DatajudError {
    #[inline]
    fn from(value: serde_json::Error) -> Self {
        Self::InvalidJson {
            message: value.to_string(),
        }
    }
}
