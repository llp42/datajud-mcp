//! # datajud-mcp
//!
//! MCP (Model Context Protocol) server for the **DATAJUD public API** (CNJ) —
//! metadata of Brazilian judiciary cases (capas processuais and movimentos)
//! from all 91 tribunal indices.
//!
//! API documentation: <https://datajud-wiki.cnj.jus.br/api-publica/>.

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
pub mod query;
pub mod server;
pub mod tribunals;
