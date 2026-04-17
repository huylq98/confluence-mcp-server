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
pub struct GetCommentsArgs {
    /// The numeric page ID.
    pub page_id: String,
    /// Maximum comments to return (default 25).
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPageByUrlArgs {
    /// Any Confluence page URL (full or relative path).
    pub url: String,
    /// Body format — 'storage' (raw XHTML) or 'view' (rendered HTML). Default 'storage'.
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPageByTitleArgs {
    /// The space key (e.g. 'DEV', 'TEAM', 'HR').
    pub space_key: String,
    /// The exact page title to look for.
    pub title: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPageArgs {
    /// The numeric page ID (e.g. '3965072').
    pub page_id: String,
    /// Body format — 'storage' (raw XHTML) or 'view' (rendered HTML). Default 'storage'.
    pub format: Option<String>,
    /// Set false to fetch only metadata. Default true.
    pub include_body: Option<bool>,
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

    #[tool(description = "Get comments on a Confluence page (inline and footer).")]
    async fn get_comments(
        &self,
        Parameters(args): Parameters<GetCommentsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(25);
        match self.client.get_child(&args.page_id, "comment", "body.view,version,extensions.inlineProperties", limit).await {
            Ok(data) => Ok(CallToolResult::success(vec![Content::text(crate::tools::get_comments::format(&data))])),
            Err(e)   => Ok(CallToolResult::success(vec![Content::text(crate::format::error_response(&e))])),
        }
    }

    #[tool(description = "Retrieve a Confluence page by its full URL. Supports all common URL formats.")]
    async fn get_page_by_url(
        &self,
        Parameters(args): Parameters<GetPageByUrlArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        use crate::tools::get_page_by_url::{resolve, UrlResolution, format_unparseable, format_tiny_url};

        let body_format = args.format.as_deref().unwrap_or("storage");
        let expand = format!("body.{body_format},version,space,metadata.labels,ancestors");

        let text = match resolve(&args.url) {
            UrlResolution::Unparseable => format_unparseable(&args.url),
            UrlResolution::TinyUrl(_)  => format_tiny_url(),
            UrlResolution::ById(id) => match self.client.get_page(&id, &expand).await {
                Ok(page) => crate::tools::get_page::format(&page, body_format, true, &self.config.confluence_url, self.config.max_content_length),
                Err(e)   => crate::format::error_response(&e),
            },
            UrlResolution::BySpaceTitle { space, title } => match self.client.get_page_by_title(&space, &title, &expand).await {
                Ok(data) => {
                    let empty = vec![];
                    let results = data.pointer("/results").and_then(|v| v.as_array()).unwrap_or(&empty);
                    if results.is_empty() {
                        format!(
                            "No page titled '{title}' found in space {space}.\nTip: Try search_confluence with: title~\"{title}\" AND space={space}"
                        )
                    } else {
                        crate::tools::get_page::format(&results[0], body_format, true, &self.config.confluence_url, self.config.max_content_length)
                    }
                }
                Err(e) => crate::format::error_response(&e),
            },
        };
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Find a Confluence page by its exact title within a space.")]
    async fn get_page_by_title(
        &self,
        Parameters(args): Parameters<GetPageByTitleArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let expand = "body.storage,version,space,metadata.labels,ancestors";
        match self.client.get_page_by_title(&args.space_key, &args.title, expand).await {
            Ok(data) => {
                let empty = vec![];
                let results = data.pointer("/results").and_then(|v| v.as_array()).unwrap_or(&empty);
                let text = if results.is_empty() {
                    crate::tools::get_page_by_title::format_not_found(&args.space_key, &args.title)
                } else {
                    crate::tools::get_page_by_title::format_found(&results[0], &args.space_key, &self.config.confluence_url, self.config.max_content_length)
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(crate::format::error_response(&e))])),
        }
    }

    #[tool(description = "Retrieve a Confluence page's full content by its numeric ID.")]
    async fn get_page(
        &self,
        Parameters(args): Parameters<GetPageArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let body_format = args.format.as_deref().unwrap_or("storage");
        let include_body = args.include_body.unwrap_or(true);
        let mut expand_parts = vec!["version", "space", "metadata.labels", "ancestors"];
        let body_expand;
        if include_body {
            body_expand = format!("body.{body_format}");
            expand_parts.push(&body_expand);
        }
        let expand = expand_parts.join(",");

        match self.client.get_page(&args.page_id, &expand).await {
            Ok(page) => Ok(CallToolResult::success(vec![Content::text(
                crate::tools::get_page::format(&page, body_format, include_body, &self.config.confluence_url, self.config.max_content_length),
            )])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(crate::format::error_response(&e))])),
        }
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
