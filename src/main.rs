//! Entry point: runs the DATAJUD MCP server over stdio.
//!
//! Note: stdout carries the MCP JSON-RPC protocol — all logging goes to stderr.

use rmcp::ServiceExt as _;
use rmcp::transport::stdio;
use std::error::Error as StdError;
use std::fmt;
use std::io;

use tracing_subscriber::EnvFilter;

use datajud_mcp::client::DatajudClient;
use datajud_mcp::config::Config;
use datajud_mcp::server::DatajudServer;

/// Fatal startup error: its `Debug` prints the human message, so `main`
/// reports a single clean line instead of a struct dump.
struct FatalError(String);

impl fmt::Debug for FatalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl fmt::Display for FatalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl StdError for FatalError {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError>> {
    // Log to stderr only: stdout is the protocol channel.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(io::stderr)
        .init();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            let fatal: Box<dyn StdError> = Box::new(FatalError(error.to_string()));
            return Err(fatal);
        }
    };
    tracing::info!(
        base_url = %config.base_url,
        timeout_secs = config.timeout.as_secs(),
        max_retries = config.max_retries,
        "starting datajud-mcp (DATAJUD public API — CNJ)"
    );

    let client = DatajudClient::new(config)?;
    let server = DatajudServer::new(client);

    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
