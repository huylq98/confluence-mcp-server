use crate::recorder::{ErrorEntry, HistoryEntry, Recorder};
use anyhow::Result;
use confluence_core::{Client, Config, ConfluenceError};
use rmcp::schemars::JsonSchema;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSpacesArgs {
    /// Filter by space type — 'global', 'personal', or 'all'. Defaults to 'global'.
    #[serde(rename = "type")]
    pub space_type: Option<String>,
    /// Maximum spaces to return (default 50).
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetAttachmentsArgs {
    /// The numeric page ID.
    pub page_id: String,
    /// Maximum attachments to return (default 50).
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
    recorder: Option<Arc<Recorder>>,
    tool_router: ToolRouter<ConfluenceServer>,
}

#[tool_router]
impl ConfluenceServer {
    pub fn from_env() -> Result<Self> {
        let mut config = Config::from_env();
        // When CONFLUENCE_PROXY_URL isn't explicitly set, inherit the Windows
        // system proxy — same PAC / static config the user's browser uses.
        // This makes the server "just work" on corporate networks where the
        // browser succeeds but a raw reqwest client (no proxy awareness) fails.
        if config
            .proxy_url
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            let detected = confluence_core::system_proxy::detect();
            if let Some(p) = detected.display() {
                tracing::info!(proxy = %p, source = "Windows system settings", "auto-applied proxy");
                config.proxy_url = Some(p);
            }
        }
        config.validate()?;
        let client = Client::new(config.clone())?;
        let recorder = Recorder::from_current_exe().map(Arc::new);
        Ok(Self {
            client: Arc::new(client),
            config: Arc::new(config),
            recorder,
            tool_router: Self::tool_router(),
        })
    }

    pub fn confluence_url(&self) -> &str {
        &self.config.confluence_url
    }

    fn now_ts() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn record(&self, tool: &'static str, args: serde_json::Value, text: &str, status: &str) {
        let Some(rec) = self.recorder.as_ref() else {
            return;
        };
        let out_chars = text.chars().count();
        rec.record_history(&HistoryEntry {
            ts: Self::now_ts(),
            tool,
            args,
            out_chars,
            tokens_est: out_chars / 4,
            status,
        });
    }

    fn record_error(&self, tool: &'static str, status: &str, message: &str) {
        let Some(rec) = self.recorder.as_ref() else {
            return;
        };
        let snippet: String = message.chars().take(300).collect();
        rec.record_error(&ErrorEntry {
            ts: Self::now_ts(),
            tool,
            status,
            message: &snippet,
        });
    }

    fn handle_err(&self, tool: &'static str, e: &ConfluenceError) -> (String, String) {
        let msg = crate::format::error_response(e);
        let code = e.status_code();
        let status = if code > 0 {
            code.to_string()
        } else {
            "error".into()
        };
        self.record_error(tool, &status, &msg);
        (msg, status)
    }

    #[tool(description = "List file attachments on a Confluence page with download URLs.")]
    async fn get_attachments(
        &self,
        Parameters(args): Parameters<GetAttachmentsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(50);
        let page_id = args.page_id.clone();
        let (text, status) = match self
            .client
            .get_child(&page_id, "attachment", "version", limit)
            .await
        {
            Ok(data) => (
                crate::tools::get_attachments::format(&data, &self.config.confluence_url),
                "ok".to_string(),
            ),
            Err(e) => self.handle_err("get_attachments", &e),
        };
        self.record(
            "get_attachments",
            serde_json::json!({"page_id": page_id}),
            &text,
            &status,
        );
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Get comments on a Confluence page (inline and footer).")]
    async fn get_comments(
        &self,
        Parameters(args): Parameters<GetCommentsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(25);
        let page_id = args.page_id.clone();
        let (text, status) = match self
            .client
            .get_child(
                &page_id,
                "comment",
                "body.view,version,extensions.inlineProperties",
                limit,
            )
            .await
        {
            Ok(data) => (crate::tools::get_comments::format(&data), "ok".to_string()),
            Err(e) => self.handle_err("get_comments", &e),
        };
        self.record(
            "get_comments",
            serde_json::json!({"page_id": page_id}),
            &text,
            &status,
        );
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        description = "Retrieve a Confluence page by its full URL. Supports all common URL formats."
    )]
    async fn get_page_by_url(
        &self,
        Parameters(args): Parameters<GetPageByUrlArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        use crate::tools::get_page_by_url::{
            format_tiny_url, format_unparseable, resolve, UrlResolution,
        };

        let body_format = args.format.as_deref().unwrap_or("storage");
        let expand = format!("body.{body_format},version,space,metadata.labels,ancestors");
        let args_json = serde_json::json!({"url": args.url});

        let (text, status) = match resolve(&args.url) {
            UrlResolution::Unparseable => (format_unparseable(&args.url), "ok".to_string()),
            UrlResolution::TinyUrl => (format_tiny_url(), "ok".to_string()),
            UrlResolution::ById(id) => match self.client.get_page(&id, &expand).await {
                Ok(page) => (
                    crate::tools::get_page::format(
                        &page,
                        body_format,
                        true,
                        &self.config.confluence_url,
                        self.config.max_content_length,
                    ),
                    "ok".to_string(),
                ),
                Err(e) => self.handle_err("get_page_by_url", &e),
            },
            UrlResolution::BySpaceTitle { space, title } => {
                match self.client.get_page_by_title(&space, &title, &expand).await {
                    Ok(data) => {
                        let empty = vec![];
                        let results = data
                            .pointer("/results")
                            .and_then(|v| v.as_array())
                            .unwrap_or(&empty);
                        let text = if results.is_empty() {
                            format!(
                            "No page titled '{title}' found in space {space}.\nTip: Try search_confluence with: title~\"{title}\" AND space={space}"
                        )
                        } else {
                            crate::tools::get_page::format(
                                &results[0],
                                body_format,
                                true,
                                &self.config.confluence_url,
                                self.config.max_content_length,
                            )
                        };
                        (text, "ok".to_string())
                    }
                    Err(e) => self.handle_err("get_page_by_url", &e),
                }
            }
        };
        self.record("get_page_by_url", args_json, &text, &status);
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Find a Confluence page by its exact title within a space.")]
    async fn get_page_by_title(
        &self,
        Parameters(args): Parameters<GetPageByTitleArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let expand = "body.storage,version,space,metadata.labels,ancestors";
        let args_json = serde_json::json!({"space_key": args.space_key, "title": args.title});
        let (text, status) = match self
            .client
            .get_page_by_title(&args.space_key, &args.title, expand)
            .await
        {
            Ok(data) => {
                let empty = vec![];
                let results = data
                    .pointer("/results")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty);
                let text = if results.is_empty() {
                    crate::tools::get_page_by_title::format_not_found(&args.space_key, &args.title)
                } else {
                    crate::tools::get_page_by_title::format_found(
                        &results[0],
                        &args.space_key,
                        &self.config.confluence_url,
                        self.config.max_content_length,
                    )
                };
                (text, "ok".to_string())
            }
            Err(e) => self.handle_err("get_page_by_title", &e),
        };
        self.record("get_page_by_title", args_json, &text, &status);
        Ok(CallToolResult::success(vec![Content::text(text)]))
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
        let page_id = args.page_id.clone();

        let (text, status) = match self.client.get_page(&page_id, &expand).await {
            Ok(page) => (
                crate::tools::get_page::format(
                    &page,
                    body_format,
                    include_body,
                    &self.config.confluence_url,
                    self.config.max_content_length,
                ),
                "ok".to_string(),
            ),
            Err(e) => self.handle_err("get_page", &e),
        };
        self.record(
            "get_page",
            serde_json::json!({"page_id": page_id}),
            &text,
            &status,
        );
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Search Confluence pages using CQL (Confluence Query Language).")]
    async fn search_confluence(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = args.limit.unwrap_or(10).clamp(1, 50);
        let (text, status) = match self
            .client
            .search(&args.cql, limit, "space,version,metadata.labels")
            .await
        {
            Ok(data) => (
                crate::tools::search_confluence::format(&data, &self.config.confluence_url),
                "ok".to_string(),
            ),
            Err(e) => self.handle_err("search_confluence", &e),
        };
        self.record("search_confluence", serde_json::json!({}), &text, &status);
        Ok(CallToolResult::success(vec![Content::text(text)]))
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
        let (text, status) = match self
            .client
            .list_spaces(space_type, limit, "description.plain")
            .await
        {
            Ok(data) => (crate::tools::list_spaces::format(&data), "ok".to_string()),
            Err(e) => self.handle_err("list_spaces", &e),
        };
        self.record("list_spaces", serde_json::json!({}), &text, &status);
        Ok(CallToolResult::success(vec![Content::text(text)]))
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
                always use get_page_by_url to fetch the page content directly from the link."
                    .into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use confluence_core::ConfluenceError;

    #[test]
    fn status_encoding_http_vs_client_error() {
        // Real HTTP status (403) -> "403"
        let http_err = ConfluenceError::Http {
            status: 403,
            message: "denied".into(),
        };
        assert_eq!(http_err.status_code(), 403);

        // Non-HTTP error path (reqwest network failure) should have status_code() == 0
        // and encode as "error" in handle_err. We assert the Http variant encoding here;
        // the "error" branch is exercised by the recorder integration (covered separately).
        let status = if http_err.status_code() > 0 {
            http_err.status_code().to_string()
        } else {
            "error".to_string()
        };
        assert_eq!(status, "403");

        // Client-side error (no HTTP status) encodes as "error".
        // Construct a simple variant that returns status_code() == 0.
        let cfg_err = ConfluenceError::Config("bad config".into());
        assert_eq!(cfg_err.status_code(), 0);
        let status = if cfg_err.status_code() > 0 {
            cfg_err.status_code().to_string()
        } else {
            "error".to_string()
        };
        assert_eq!(status, "error");
    }
}
