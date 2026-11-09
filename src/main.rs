//! Entry point: runs the DATAJUD MCP server over stdio.
//!
//! Note: stdout carries the MCP JSON-RPC protocol — all logging goes to stderr.

use rmcp::ServiceExt as _;
use rmcp::transport::stdio;
use std::error::Error as StdError;
use std::io;

use tracing_subscriber::EnvFilter;

use datajud_mcp::client::DatajudClient;
use datajud_mcp::config::Config;
use datajud_mcp::server::DatajudServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError>> {
    // Log to stderr only: stdout is the protocol channel.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| return EnvFilter::new("info")),
        )
        .with_writer(io::stderr)
        .init();

    let config = Config::from_env();
    tracing::info!(
        base_url = %config.base_url,
        timeout_secs = config.timeout.as_secs(),
        max_retries = config.max_retries,
        "starting datajud-mcp (DATAJUD public API — CNJ)"
    );

    let client = match DatajudClient::new(config) {
        Ok(built) => built,
        Err(error) => {
            let boxed: Box<dyn StdError> = Box::new(error);
            return Err(boxed);
        }
    };
    let server = DatajudServer::new(client);

    let service = match server.serve(stdio()).await {
        Ok(connected) => connected,
        Err(error) => {
            let boxed: Box<dyn StdError> = Box::new(error);
            return Err(boxed);
        }
    };
    match service.waiting().await {
        Ok(_quit_reason) => return Ok(()),
        Err(error) => {
            let boxed: Box<dyn StdError> = Box::new(error);
            return Err(boxed);
        }
    }
}
