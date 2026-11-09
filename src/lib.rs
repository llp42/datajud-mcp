//! # datajud-mcp
//!
//! MCP (Model Context Protocol) server for the **DATAJUD public API** (CNJ) —
//! metadata of Brazilian judiciary cases (capas processuais and movimentos)
//! from all 91 tribunal indices.
//!
//! API documentation: <https://datajud-wiki.cnj.jus.br/api-publica/>.
//!
//! # Usage as a library
//!
//! The binary is an MCP server; the library underneath can also be used to
//! query the API directly:
//!
//! ```no_run
//! use datajud_mcp::client::DatajudClient;
//! use datajud_mcp::config::Config;
//! use serde_json::json;
//!
//! # async fn demo() -> Result<(), datajud_mcp::error::DatajudError> {
//! // Reads DATAJUD_API_KEY (required) and optional DATAJUD_* overrides.
//! let client = DatajudClient::new(Config::from_env()?)?;
//!
//! // Search one case by its CNJ number on the TRF1 index.
//! let body = json!({ "query": { "match": { "numeroProcesso": "00008323520184013202" } } });
//! let response = client.search("api_publica_trf1", &body).await?;
//! println!("total hits: {}", response["hits"]["total"]["value"]);
//! # Ok(())
//! # }
//! ```
//!
//! Note: consuming the API implies accepting the CNJ terms of use —
//! legal, non-commercial purposes only (see `datajud://terms-of-use`).

// Test-only expectations: panicking assertions, direct unwraps and private
// test functions are idiomatic in tests and fire at least once in the test
// build (unfulfilled expectations are denied via the `unused` lint group).
#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::implicit_return,
        clippy::default_numeric_fallback,
        reason = "test code: panicking assertions, direct indexing and private test fns are idiomatic in tests"
    )
)]

pub mod client;
pub mod config;
pub mod error;
pub mod glossary;
pub mod process_number;
pub mod query;
pub mod server;
pub mod tribunals;
