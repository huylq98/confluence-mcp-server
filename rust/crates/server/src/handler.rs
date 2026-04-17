use anyhow::Result;
use confluence_core::{Client, Config};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ServerHandler,
};
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSpacesArgs {
    /// Filter by space type — 'global', 'personal', or 'all'. Defaults to 'global'.
    #[serde(rename = "type")]
    pub space_type: Option<String>,
    /// Maximum spaces to return (default 50).
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// A CQL query string (e.g. 'type=page AND text~"deployment"').
    pub cql: String,
    /// Maximum results to return (1–50, default 10).
    pub limit: Option<u32>,
}

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

    #[tool(description = "Search Confluence pages using CQL (Confluence Query Language).")]
    async fn search_confluence(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(10).clamp(1, 50);
        match self.client.search(&args.cql, limit, "space,version,metadata.labels").await {
            Ok(data) => Ok(CallToolResult::success(vec![Content::text(
                crate::tools::search_confluence::format(&data, &self.config.confluence_url),
            )])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(crate::format::error_response(&e))])),
        }
    }

    #[tool(description = "List Confluence spaces the authenticated user can access.")]
    async fn list_spaces(
        &self,
        Parameters(args): Parameters<ListSpacesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let space_type = match args.space_type.as_deref() {
            Some("all") | None => None,
            Some(other) => Some(other),
        };
        let limit = args.limit.unwrap_or(50);
        match self.client.list_spaces(space_type, limit, "description.plain").await {
            Ok(data) => Ok(CallToolResult::success(vec![Content::text(crate::tools::list_spaces::format(&data))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(crate::format::error_response(&e))])),
        }
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
