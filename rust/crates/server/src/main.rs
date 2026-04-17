use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

mod format;
mod handler;

use handler::ConfluenceServer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let server = ConfluenceServer::from_env()?;
    tracing::info!(url = %server.confluence_url(), "starting Confluence MCP server");

    let service = server.serve(stdio()).await.inspect_err(|e| {
        tracing::error!("serve error: {e:?}");
    })?;
    service.waiting().await?;
    Ok(())
}
