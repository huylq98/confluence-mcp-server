use anyhow::Result;
use confluence_core::{Client, Config};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool_handler, tool_router, ServerHandler,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct ConfluenceServer {
    pub(crate) client: Arc<Client>,
    pub(crate) config: Arc<Config>,
    tool_router: ToolRouter<ConfluenceServer>,
}

#[tool_router]
impl ConfluenceServer {
    pub fn from_env() -> Result<Self> {
        let config = Config::from_env();
        config.validate()?;
        let client = Client::new(config.clone())?;
        Ok(Self {
            client: Arc::new(client),
            config: Arc::new(config),
            tool_router: Self::tool_router(),
        })
    }

    pub fn confluence_url(&self) -> &str {
        &self.config.confluence_url
    }
}

#[tool_handler]
impl ServerHandler for ConfluenceServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "confluence-server".into(),
                title: None,
                version: env!("CARGO_PKG_VERSION").into(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Confluence Server integration. Use these tools to search wiki pages, \
                read page content, list spaces, and fetch comments/attachments from a \
                self-hosted Confluence instance. When a user pastes a Confluence URL, \
                always use get_page_by_url to fetch the page content directly from the link.".into()
            ),
        }
    }
}
