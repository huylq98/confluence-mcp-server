# Rust Rewrite for Distribution Size Reduction — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Python+PyInstaller MCP server + wizard (~27 MB exe) with a Rust/Tauri stack producing a ~6–9 MB Windows-only installer that embeds a lean ~2–5 MB `rmcp`-based server binary.

**Architecture:** Cargo workspace under `rust/` with three crates — `confluence-core` (shared HTTP client + types), `server` (stdio MCP server built on `rmcp`), `configurator` (Tauri 2 wizard embedding the server binary). Wizard extracts server to `%LOCALAPPDATA%\ConfluenceMCP\` on Save and writes that path into `claude_desktop_config.json`. Post-install, Claude Desktop launches only the server exe.

**Tech Stack:** Rust 1.75+, `rmcp` (official Model Context Protocol SDK), `reqwest` + `rustls-tls`, `tokio`, `serde`, Tauri 2, `tauri-plugin-dialog`, `wiremock` (tests), `tempfile` (tests), UPX 4.2.4.

**Spec:** `docs/superpowers/specs/2026-04-17-rust-rewrite-size-reduction-design.md`

---

## Global Conventions

- All new code lives under `rust/` until the cutover (Task 31). Python files remain untouched throughout Phases 0–4.
- Every task ends with a commit. Commit messages follow `<scope>: <imperative summary>` (e.g. `core: add URL parser` / `server: implement list_spaces tool`). Use `rust/` as the default scope when a task spans multiple crates.
- Tests use `#[tokio::test]` for async and `#[test]` for sync. HTTP mocking via `wiremock` (never stub `reqwest` by hand). Filesystem tests via `tempfile::TempDir`.
- Every `cargo test` / `cargo build` command in this plan is run from `rust/` (`cd rust && cargo test`).
- The engineer runs Windows with `py` (Python launcher) available and PowerShell available. Do not assume `python` or `bash` are on PATH.

---

## Task 1: Create feature branch and Cargo workspace skeleton

**Files:**
- Create: `rust/Cargo.toml` (workspace manifest)
- Create: `rust/.gitignore`
- Create: `rust/rust-toolchain.toml`

- [ ] **Step 1: Create and switch to the `rust-port` branch**

```bash
cd C:/Users/Admin/IdeaProjects/confluence-mcp-server
git checkout -b rust-port
```

- [ ] **Step 2: Create `rust/` directory and workspace Cargo.toml**

Write `rust/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/confluence-core",
    "crates/server",
    "crates/configurator",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt", "macros", "io-std", "net", "time", "sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "gzip"] }
url = "2"
regex = "1"

[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
strip = "symbols"
panic = "abort"
```

- [ ] **Step 3: Create `rust/.gitignore` and `rust/rust-toolchain.toml`**

Write `rust/.gitignore`:

```
target/
**/*.rs.bk
crates/configurator/resources/confluence-mcp-server.exe
dist/
```

Write `rust/rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 4: Verify workspace resolves (no crates yet, should fail gracefully)**

Run: `cd rust && cargo metadata --format-version 1 >NUL 2>&1 && echo OK || echo "expected: workspace member not found"`

Expected: errors because no crates exist yet. This is fine — Task 2 will create the first crate.

- [ ] **Step 5: Commit**

```bash
git add rust/Cargo.toml rust/.gitignore rust/rust-toolchain.toml
git commit -m "rust: add Cargo workspace skeleton"
```

---

## Task 2: `confluence-core` crate — error type

**Files:**
- Create: `rust/crates/confluence-core/Cargo.toml`
- Create: `rust/crates/confluence-core/src/lib.rs`
- Create: `rust/crates/confluence-core/src/error.rs`

- [ ] **Step 1: Create crate manifest**

Write `rust/crates/confluence-core/Cargo.toml`:

```toml
[package]
name = "confluence-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
reqwest = { workspace = true }
url = { workspace = true }
regex = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
wiremock = "0.6"
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Write the failing test for `ConfluenceError::Http`**

Write `rust/crates/confluence-core/src/lib.rs`:

```rust
pub mod error;

pub use error::ConfluenceError;
```

Write `rust/crates/confluence-core/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfluenceError {
    #[error("Confluence API error (HTTP {status}): {message}")]
    Http { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl ConfluenceError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Http { status, .. } => *status,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_formats_status_and_message() {
        let err = ConfluenceError::Http { status: 404, message: "Not found".into() };
        assert_eq!(err.to_string(), "Confluence API error (HTTP 404): Not found");
        assert_eq!(err.status_code(), 404);
    }

    #[test]
    fn non_http_error_status_is_zero() {
        let err = ConfluenceError::Config("no url".into());
        assert_eq!(err.status_code(), 0);
    }
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cd rust && cargo test -p confluence-core`

Expected: `test tests::http_error_formats_status_and_message ... ok`, `test tests::non_http_error_status_is_zero ... ok`, `2 passed`.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/confluence-core
git commit -m "core: add ConfluenceError type"
```

---

## Task 3: `confluence-core` — Config struct + env loading

**Files:**
- Create: `rust/crates/confluence-core/src/config.rs`
- Modify: `rust/crates/confluence-core/src/lib.rs`

- [ ] **Step 1: Write failing tests for `Config::from_env` and `validate`**

Write `rust/crates/confluence-core/src/config.rs`:

```rust
use crate::error::ConfluenceError;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub confluence_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
    pub ssl_verify: bool,
    pub ca_bundle: Option<String>,
    pub timeout: Duration,
    pub rate_limit: u32,
    pub max_content_length: usize,
    pub default_search_limit: u32,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Self {
        fn env(key: &str) -> Option<String> {
            std::env::var(key).ok().filter(|v| !v.is_empty())
        }
        fn env_bool(key: &str, default: bool) -> bool {
            env(key).map(|v| !matches!(v.to_lowercase().as_str(), "false" | "0" | "no")).unwrap_or(default)
        }
        fn env_u32(key: &str, default: u32) -> u32 {
            env(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        }
        fn env_usize(key: &str, default: usize) -> usize {
            env(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        }

        Self {
            confluence_url: env("CONFLUENCE_URL").unwrap_or_default().trim_end_matches('/').to_string(),
            username: env("CONFLUENCE_USERNAME"),
            password: env("CONFLUENCE_PASSWORD"),
            token: env("CONFLUENCE_TOKEN"),
            ssl_verify: env_bool("CONFLUENCE_SSL_VERIFY", true),
            ca_bundle: env("CONFLUENCE_CA_BUNDLE"),
            timeout: Duration::from_secs(env_u32("CONFLUENCE_TIMEOUT", 30) as u64),
            rate_limit: env_u32("CONFLUENCE_RATE_LIMIT", 10),
            max_content_length: env_usize("MAX_CONTENT_LENGTH", 50_000),
            default_search_limit: env_u32("DEFAULT_SEARCH_LIMIT", 10),
            log_level: env("LOG_LEVEL").unwrap_or_else(|| "INFO".into()),
        }
    }

    pub fn validate(&self) -> Result<(), ConfluenceError> {
        if self.confluence_url.is_empty() {
            return Err(ConfluenceError::Config("CONFLUENCE_URL is required".into()));
        }
        if self.token.is_none() && (self.username.is_none() || self.password.is_none()) {
            return Err(ConfluenceError::Config(
                "Either CONFLUENCE_TOKEN or both CONFLUENCE_USERNAME and CONFLUENCE_PASSWORD are required".into(),
            ));
        }
        Ok(())
    }

    pub fn auth_method(&self) -> &'static str {
        if self.token.is_some() { "bearer token" } else { "basic auth" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_env() {
        for key in [
            "CONFLUENCE_URL", "CONFLUENCE_USERNAME", "CONFLUENCE_PASSWORD",
            "CONFLUENCE_TOKEN", "CONFLUENCE_SSL_VERIFY", "CONFLUENCE_TIMEOUT",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn validate_requires_url() {
        clear_env();
        let cfg = Config::from_env();
        assert!(matches!(cfg.validate(), Err(ConfluenceError::Config(_))));
    }

    #[test]
    fn validate_requires_auth() {
        clear_env();
        std::env::set_var("CONFLUENCE_URL", "https://wiki.example.com");
        let cfg = Config::from_env();
        assert!(matches!(cfg.validate(), Err(ConfluenceError::Config(_))));
    }

    #[test]
    fn validate_accepts_token_auth() {
        clear_env();
        std::env::set_var("CONFLUENCE_URL", "https://wiki.example.com/");
        std::env::set_var("CONFLUENCE_TOKEN", "abc123");
        let cfg = Config::from_env();
        assert_eq!(cfg.confluence_url, "https://wiki.example.com");
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.auth_method(), "bearer token");
    }

    #[test]
    fn ssl_verify_defaults_true_and_honors_false() {
        clear_env();
        let cfg = Config::from_env();
        assert!(cfg.ssl_verify);
        std::env::set_var("CONFLUENCE_SSL_VERIFY", "false");
        let cfg = Config::from_env();
        assert!(!cfg.ssl_verify);
    }
}
```

Modify `rust/crates/confluence-core/src/lib.rs`:

```rust
pub mod config;
pub mod error;

pub use config::Config;
pub use error::ConfluenceError;
```

- [ ] **Step 2: Run tests**

Run: `cd rust && cargo test -p confluence-core -- --test-threads=1`

Expected: all 6 tests pass (2 from error.rs + 4 from config.rs). `--test-threads=1` is required because env-var manipulation isn't thread-safe.

- [ ] **Step 3: Commit**

```bash
git add rust/crates/confluence-core/src/config.rs rust/crates/confluence-core/src/lib.rs
git commit -m "core: add Config with env loading and validation"
```

---

## Task 4: `confluence-core` — URL parser

**Files:**
- Create: `rust/crates/confluence-core/src/url_parse.rs`
- Modify: `rust/crates/confluence-core/src/lib.rs`

Port of `_parse_confluence_url` in `server.py:103-172` with full parity. Reference Python logic for the 8 URL formats.

- [ ] **Step 1: Write the failing tests for all 8 URL formats**

Write `rust/crates/confluence-core/src/url_parse.rs`:

```rust
use regex::Regex;
use url::Url;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedUrl {
    pub page_id: Option<String>,
    pub space_key: Option<String>,
    pub title: Option<String>,
}

pub fn parse_confluence_url(input: &str) -> ParsedUrl {
    let input = input.trim();
    let Ok(parsed) = Url::parse(input) else {
        // Accept path-only strings by prepending a dummy scheme
        let Ok(parsed) = Url::parse(&format!("http://dummy{input}")) else {
            return ParsedUrl::default();
        };
        return parse_inner(&parsed);
    };
    parse_inner(&parsed)
}

fn parse_inner(parsed: &Url) -> ParsedUrl {
    let mut out = ParsedUrl::default();

    // Format 1: ?pageId=12345
    if let Some((_, v)) = parsed.query_pairs().find(|(k, _)| k == "pageId") {
        out.page_id = Some(v.into_owned());
        return out;
    }

    // Strip common path prefixes (/wiki, /confluence) before matching
    let mut path = parsed.path().to_string();
    for prefix in ["/wiki", "/confluence"] {
        if path.starts_with(prefix) {
            path = path[prefix.len()..].to_string();
        }
    }

    // Format 2/3: /display/SPACEKEY/Page+Title
    let display_re = Regex::new(r"^/display/([^/]+)/(.+?)(?:\?.*)?$").unwrap();
    if let Some(caps) = display_re.captures(&path) {
        out.space_key = Some(caps[1].to_string());
        out.title = Some(decode_title(&caps[2]));
        return out;
    }

    // Format 4: /spaces/SPACEKEY/pages/12345/Page+Title
    let spaces_re = Regex::new(r"^/spaces/([^/]+)/pages/(\d+)(?:/(.+))?$").unwrap();
    if let Some(caps) = spaces_re.captures(&path) {
        out.space_key = Some(caps[1].to_string());
        out.page_id = Some(caps[2].to_string());
        if let Some(t) = caps.get(3) {
            out.title = Some(decode_title(t.as_str()));
        }
        return out;
    }

    // Format 5: /x/shortlink (tiny URL)
    let tiny_re = Regex::new(r"^/x/([A-Za-z0-9_-]+)").unwrap();
    if let Some(caps) = tiny_re.captures(&path) {
        out.page_id = Some(format!("tinyurl:{}", &caps[1]));
        return out;
    }

    // Format 6: /pages/12345
    let pages_re = Regex::new(r"^/pages/(\d+)").unwrap();
    if let Some(caps) = pages_re.captures(&path) {
        out.page_id = Some(caps[1].to_string());
        return out;
    }

    // Fallback: any /1234+ segment
    let num_re = Regex::new(r"/(\d{4,})").unwrap();
    if let Some(caps) = num_re.captures(&path) {
        out.page_id = Some(caps[1].to_string());
    }

    out
}

fn decode_title(raw: &str) -> String {
    let with_spaces = raw.replace('+', " ");
    urlencoding::decode(&with_spaces).map(|s| s.into_owned()).unwrap_or(with_spaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_1_page_id_query_param() {
        let r = parse_confluence_url("http://wiki/pages/viewpage.action?pageId=12345");
        assert_eq!(r.page_id.as_deref(), Some("12345"));
        assert_eq!(r.space_key, None);
        assert_eq!(r.title, None);
    }

    #[test]
    fn format_2_display_space_title() {
        let r = parse_confluence_url("http://wiki/display/DEV/My+Page");
        assert_eq!(r.space_key.as_deref(), Some("DEV"));
        assert_eq!(r.title.as_deref(), Some("My Page"));
        assert_eq!(r.page_id, None);
    }

    #[test]
    fn format_3_display_with_query() {
        let r = parse_confluence_url("http://wiki/display/DEV/My+Page?src=foo");
        assert_eq!(r.space_key.as_deref(), Some("DEV"));
        assert_eq!(r.title.as_deref(), Some("My Page"));
    }

    #[test]
    fn format_4_spaces_pages() {
        let r = parse_confluence_url("http://wiki/spaces/DEV/pages/999/My+Page");
        assert_eq!(r.space_key.as_deref(), Some("DEV"));
        assert_eq!(r.page_id.as_deref(), Some("999"));
        assert_eq!(r.title.as_deref(), Some("My Page"));
    }

    #[test]
    fn format_5_tiny_url() {
        let r = parse_confluence_url("http://wiki/x/abc-123_DEF");
        assert_eq!(r.page_id.as_deref(), Some("tinyurl:abc-123_DEF"));
    }

    #[test]
    fn format_6_pages_id_only() {
        let r = parse_confluence_url("http://wiki/pages/55555");
        assert_eq!(r.page_id.as_deref(), Some("55555"));
    }

    #[test]
    fn format_7_wiki_context_prefix_stripped() {
        let r = parse_confluence_url("http://wiki/wiki/display/HR/On+Call");
        assert_eq!(r.space_key.as_deref(), Some("HR"));
        assert_eq!(r.title.as_deref(), Some("On Call"));
    }

    #[test]
    fn format_8_confluence_context_prefix_stripped() {
        let r = parse_confluence_url("http://wiki/confluence/display/OPS/Runbook");
        assert_eq!(r.space_key.as_deref(), Some("OPS"));
        assert_eq!(r.title.as_deref(), Some("Runbook"));
    }

    #[test]
    fn fallback_numeric_segment() {
        let r = parse_confluence_url("http://wiki/somewhere/123456/extra");
        assert_eq!(r.page_id.as_deref(), Some("123456"));
    }

    #[test]
    fn unparseable_returns_empty() {
        let r = parse_confluence_url("not a url at all!!!");
        assert_eq!(r, ParsedUrl::default());
    }
}
```

- [ ] **Step 2: Add `urlencoding` dependency**

Modify `rust/crates/confluence-core/Cargo.toml`, add to `[dependencies]`:

```toml
urlencoding = "2"
```

- [ ] **Step 3: Register module in lib.rs**

Modify `rust/crates/confluence-core/src/lib.rs`:

```rust
pub mod config;
pub mod error;
pub mod url_parse;

pub use config::Config;
pub use error::ConfluenceError;
pub use url_parse::{parse_confluence_url, ParsedUrl};
```

- [ ] **Step 4: Run tests**

Run: `cd rust && cargo test -p confluence-core url_parse`

Expected: all 10 URL parser tests pass.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/confluence-core
git commit -m "core: add Confluence URL parser with 8-format parity"
```

---

## Task 5: `confluence-core` — format helpers (HTML strip, truncate)

**Files:**
- Create: `rust/crates/confluence-core/src/format.rs`
- Modify: `rust/crates/confluence-core/src/lib.rs`

Port of `_strip_html` and `_truncate` from `server.py:62-79`.

- [ ] **Step 1: Write failing tests**

Write `rust/crates/confluence-core/src/format.rs`:

```rust
use regex::Regex;

pub fn strip_html(html: &str) -> String {
    let mut text = html.to_string();
    text = Regex::new(r"(?i)<br\s*/?>").unwrap().replace_all(&text, "\n").into_owned();
    text = Regex::new(r"(?i)</(p|div|h[1-6]|li|tr)>").unwrap().replace_all(&text, "\n").into_owned();
    text = Regex::new(r"<[^>]+>").unwrap().replace_all(&text, "").into_owned();
    text = text.replace("&nbsp;", " ")
               .replace("&amp;", "&")
               .replace("&lt;", "<")
               .replace("&gt;", ">");
    text = Regex::new(r"\n{3,}").unwrap().replace_all(&text, "\n\n").into_owned();
    text.trim().to_string()
}

pub fn truncate(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_len).collect();
    let total = text.chars().count();
    format!("{truncated}\n\n... [truncated — {total} chars total]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_removes_tags() {
        let html = "<p>Hello <b>world</b></p>";
        assert_eq!(strip_html(html), "Hello world");
    }

    #[test]
    fn strip_html_handles_br_as_newline() {
        assert_eq!(strip_html("a<br>b<br/>c"), "a\nb\nc");
    }

    #[test]
    fn strip_html_decodes_entities() {
        assert_eq!(strip_html("&amp;&lt;&gt;&nbsp;"), "& <  >");
    }

    #[test]
    fn strip_html_collapses_triple_newlines() {
        assert_eq!(strip_html("a\n\n\n\nb"), "a\n\nb");
    }

    #[test]
    fn truncate_leaves_short_text_alone() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn truncate_appends_marker_when_long() {
        let r = truncate("abcdefghij", 5);
        assert!(r.starts_with("abcde"));
        assert!(r.contains("truncated"));
        assert!(r.contains("10 chars"));
    }
}
```

- [ ] **Step 2: Register in lib.rs**

Modify `rust/crates/confluence-core/src/lib.rs`:

```rust
pub mod config;
pub mod error;
pub mod format;
pub mod url_parse;

pub use config::Config;
pub use error::ConfluenceError;
pub use format::{strip_html, truncate};
pub use url_parse::{parse_confluence_url, ParsedUrl};
```

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p confluence-core format`

Expected: all 6 format tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/confluence-core
git commit -m "core: add HTML strip and truncate helpers"
```

---

## Task 6: `confluence-core` — HTTP client with auth

**Files:**
- Create: `rust/crates/confluence-core/src/client.rs`
- Modify: `rust/crates/confluence-core/src/lib.rs`
- Create: `rust/crates/confluence-core/tests/client_auth.rs`

- [ ] **Step 1: Write the failing integration test for Bearer auth**

Write `rust/crates/confluence-core/tests/client_auth.rs`:

```rust
use confluence_core::{Client, Config};
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(mock_url: String, auth: (Option<&str>, Option<&str>, Option<&str>)) -> Config {
    Config {
        confluence_url: mock_url,
        username: auth.0.map(String::from),
        password: auth.1.map(String::from),
        token: auth.2.map(String::from),
        ssl_verify: true,
        ca_bundle: None,
        timeout: Duration::from_secs(5),
        rate_limit: 100,
        max_content_length: 50_000,
        default_search_limit: 10,
        log_level: "INFO".into(),
    }
}

#[tokio::test]
async fn bearer_token_adds_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/space"))
        .and(header("Authorization", "Bearer secret-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server)
        .await;

    let client = Client::new(cfg(server.uri(), (None, None, Some("secret-token")))).unwrap();
    let result = client.list_spaces(None, 10, "").await.unwrap();
    assert_eq!(result["results"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn basic_auth_encodes_user_and_pass() {
    let server = MockServer::start().await;
    // Basic base64("alice:s3cret") = "YWxpY2U6czNjcmV0"
    Mock::given(method("GET"))
        .and(path("/rest/api/space"))
        .and(header("Authorization", "Basic YWxpY2U6czNjcmV0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server)
        .await;

    let client = Client::new(cfg(server.uri(), (Some("alice"), Some("s3cret"), None))).unwrap();
    client.list_spaces(None, 10, "").await.unwrap();
}

#[tokio::test]
async fn maps_401_to_http_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/space"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&server)
        .await;

    let client = Client::new(cfg(server.uri(), (None, None, Some("bad")))).unwrap();
    let err = client.list_spaces(None, 10, "").await.unwrap_err();
    assert_eq!(err.status_code(), 401);
}
```

- [ ] **Step 2: Implement `Client`**

Write `rust/crates/confluence-core/src/client.rs`:

```rust
use crate::{Config, ConfluenceError};
use reqwest::{header::{HeaderMap, HeaderValue, AUTHORIZATION, ACCEPT}, Method, StatusCode};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    sem: Arc<Semaphore>,
}

impl Client {
    pub fn new(config: Config) -> Result<Self, ConfluenceError> {
        config.validate()?;

        let mut headers = HeaderMap::new();
        if let Some(token) = &config.token {
            let v = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|e| ConfluenceError::Config(format!("invalid token: {e}")))?;
            headers.insert(AUTHORIZATION, v);
        } else if let (Some(u), Some(p)) = (&config.username, &config.password) {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let creds = STANDARD.encode(format!("{u}:{p}"));
            let v = HeaderValue::from_str(&format!("Basic {creds}"))
                .map_err(|e| ConfluenceError::Config(format!("invalid basic auth: {e}")))?;
            headers.insert(AUTHORIZATION, v);
        }
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(!config.ssl_verify)
            .timeout(config.timeout);

        if let Some(bundle_path) = &config.ca_bundle {
            let pem = std::fs::read(bundle_path)
                .map_err(|e| ConfluenceError::Config(format!("cannot read CA bundle at {bundle_path}: {e}")))?;
            let cert = reqwest::Certificate::from_pem(&pem)
                .map_err(|e| ConfluenceError::Config(format!("invalid CA bundle at {bundle_path}: {e}")))?;
            builder = builder.add_root_certificate(cert);
        }

        let http = builder.build()?;

        Ok(Self {
            http,
            base_url: config.confluence_url.clone(),
            sem: Arc::new(Semaphore::new(config.rate_limit as usize)),
        })
    }

    async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ConfluenceError> {
        let _permit = self.sem.acquire().await.unwrap();
        let url = format!("{}{}", self.base_url, path);
        let response = self.http.request(Method::GET, &url).query(query).send().await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response.json().await?);
        }
        let message = response.text().await.unwrap_or_default();
        Err(ConfluenceError::Http {
            status: status.as_u16(),
            message: if message.is_empty() { status.canonical_reason().unwrap_or("").into() } else { message },
        })
    }

    pub async fn list_spaces(&self, space_type: Option<&str>, limit: u32, expand: &str) -> Result<Value, ConfluenceError> {
        let mut q: Vec<(&str, String)> = vec![("limit", limit.to_string())];
        if let Some(t) = space_type { q.push(("type", t.into())); }
        if !expand.is_empty() { q.push(("expand", expand.into())); }
        self.get_json("/rest/api/space", &q).await
    }
}

// Silence unused-status warning for future tasks
#[allow(dead_code)]
fn _retain_status_import(_s: StatusCode) {}
```

- [ ] **Step 3: Add `base64` dependency**

Modify `rust/crates/confluence-core/Cargo.toml`, add:

```toml
base64 = "0.22"
```

- [ ] **Step 4: Register `Client` in lib.rs**

Modify `rust/crates/confluence-core/src/lib.rs`:

```rust
pub mod client;
pub mod config;
pub mod error;
pub mod format;
pub mod url_parse;

pub use client::Client;
pub use config::Config;
pub use error::ConfluenceError;
pub use format::{strip_html, truncate};
pub use url_parse::{parse_confluence_url, ParsedUrl};
```

- [ ] **Step 5: Run tests**

Run: `cd rust && cargo test -p confluence-core --test client_auth`

Expected: 3 tests pass (bearer, basic, 401 mapping).

- [ ] **Step 6: Commit**

```bash
git add rust/crates/confluence-core
git commit -m "core: add HTTP Client with bearer/basic auth"
```

---

## Task 7: `confluence-core` — retry on 429/503

**Files:**
- Modify: `rust/crates/confluence-core/src/client.rs`
- Create: `rust/crates/confluence-core/tests/client_retry.rs`

Port of the retry-with-exponential-backoff behavior from `confluence_client.py`. Max 3 retries on 429/503 with backoff 1s, 2s, 4s.

- [ ] **Step 1: Write the failing retry test**

Write `rust/crates/confluence-core/tests/client_retry.rs`:

```rust
use confluence_core::{Client, Config};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(url: String) -> Config {
    Config {
        confluence_url: url,
        username: None, password: None, token: Some("t".into()),
        ssl_verify: true, ca_bundle: None,
        timeout: Duration::from_secs(30),
        rate_limit: 10, max_content_length: 50_000, default_search_limit: 10,
        log_level: "INFO".into(),
    }
}

#[tokio::test]
async fn retries_on_429_then_succeeds() {
    let server = MockServer::start().await;
    // First call 429, second call 200
    Mock::given(method("GET")).and(path("/rest/api/space"))
        .respond_with(ResponseTemplate::new(429).set_delay(Duration::from_millis(0)))
        .up_to_n_times(1)
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/rest/api/space"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    let start = Instant::now();
    client.list_spaces(None, 10, "").await.unwrap();
    // After 1 retry with 1s backoff, total time ≥ ~1s
    assert!(start.elapsed() >= Duration::from_millis(800));
}

#[tokio::test]
async fn gives_up_after_three_retries() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/rest/api/space"))
        .respond_with(ResponseTemplate::new(503))
        .expect(4)  // initial + 3 retries
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    let err = client.list_spaces(None, 10, "").await.unwrap_err();
    assert_eq!(err.status_code(), 503);
}
```

- [ ] **Step 2: Refactor `get_json` to retry on 429/503**

Replace the `get_json` method in `rust/crates/confluence-core/src/client.rs`:

```rust
async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ConfluenceError> {
    let url = format!("{}{}", self.base_url, path);
    let mut attempt = 0u32;
    let max_retries = 3u32;

    loop {
        let _permit = self.sem.acquire().await.unwrap();
        let response = self.http.request(Method::GET, &url).query(query).send().await?;
        drop(_permit);

        let status = response.status();
        if status.is_success() {
            return Ok(response.json().await?);
        }

        let retryable = matches!(status.as_u16(), 429 | 503);
        if retryable && attempt < max_retries {
            let backoff_ms = 1000u64 * 2u64.pow(attempt);
            tracing::warn!(status = %status, attempt, backoff_ms, "retrying after rate limit / service unavailable");
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            attempt += 1;
            continue;
        }

        let message = response.text().await.unwrap_or_default();
        return Err(ConfluenceError::Http {
            status: status.as_u16(),
            message: if message.is_empty() { status.canonical_reason().unwrap_or("").into() } else { message },
        });
    }
}
```

Add `use std::time::Duration;` near the top of `client.rs` if not already present.

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p confluence-core --test client_retry`

Expected: both retry tests pass. `retries_on_429_then_succeeds` takes ~1 second.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/confluence-core
git commit -m "core: add exponential backoff retry on 429/503"
```

---

## Task 8: `confluence-core` — remaining Client endpoints

**Files:**
- Modify: `rust/crates/confluence-core/src/client.rs`

Add methods used by the 7 MCP tools: `search`, `get_page`, `get_page_by_title`, `get_child`.

- [ ] **Step 1: Add endpoint methods**

Append to the `impl Client` block in `rust/crates/confluence-core/src/client.rs`:

```rust
pub async fn search(&self, cql: &str, limit: u32, expand: &str) -> Result<Value, ConfluenceError> {
    let mut q: Vec<(&str, String)> = vec![("cql", cql.into()), ("limit", limit.to_string())];
    if !expand.is_empty() { q.push(("expand", expand.into())); }
    self.get_json("/rest/api/content/search", &q).await
}

pub async fn get_page(&self, page_id: &str, expand: &str) -> Result<Value, ConfluenceError> {
    let path = format!("/rest/api/content/{page_id}");
    let q: Vec<(&str, String)> = if expand.is_empty() { vec![] } else { vec![("expand", expand.into())] };
    self.get_json(&path, &q).await
}

pub async fn get_page_by_title(&self, space_key: &str, title: &str, expand: &str) -> Result<Value, ConfluenceError> {
    let mut q: Vec<(&str, String)> = vec![
        ("spaceKey", space_key.into()),
        ("title", title.into()),
    ];
    if !expand.is_empty() { q.push(("expand", expand.into())); }
    self.get_json("/rest/api/content", &q).await
}

pub async fn get_child(&self, page_id: &str, child_type: &str, expand: &str, limit: u32) -> Result<Value, ConfluenceError> {
    let path = format!("/rest/api/content/{page_id}/child/{child_type}");
    let mut q: Vec<(&str, String)> = vec![("limit", limit.to_string())];
    if !expand.is_empty() { q.push(("expand", expand.into())); }
    self.get_json(&path, &q).await
}
```

- [ ] **Step 2: Write integration test covering all endpoints**

Create `rust/crates/confluence-core/tests/client_endpoints.rs`:

```rust
use confluence_core::{Client, Config};
use std::time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(url: String) -> Config {
    Config {
        confluence_url: url,
        username: None, password: None, token: Some("t".into()),
        ssl_verify: true, ca_bundle: None,
        timeout: Duration::from_secs(5),
        rate_limit: 10, max_content_length: 50_000, default_search_limit: 10,
        log_level: "INFO".into(),
    }
}

#[tokio::test]
async fn search_sends_cql_and_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/content/search"))
        .and(query_param("cql", "type=page"))
        .and(query_param("limit", "5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": [], "totalSize": 0})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    let result = client.search("type=page", 5, "").await.unwrap();
    assert_eq!(result["totalSize"], 0);
}

#[tokio::test]
async fn get_page_sends_expand() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/content/12345"))
        .and(query_param("expand", "body.storage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "12345", "title": "Test"})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    let page = client.get_page("12345", "body.storage").await.unwrap();
    assert_eq!(page["id"], "12345");
}

#[tokio::test]
async fn get_page_by_title_sends_space_key_and_title() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/content"))
        .and(query_param("spaceKey", "DEV"))
        .and(query_param("title", "My Page"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    client.get_page_by_title("DEV", "My Page", "").await.unwrap();
}

#[tokio::test]
async fn get_child_builds_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/content/99/child/comment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    client.get_child("99", "comment", "", 25).await.unwrap();
}
```

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p confluence-core --test client_endpoints`

Expected: all 4 endpoint tests pass.

- [ ] **Step 4: Run full confluence-core test suite to confirm no regressions**

Run: `cd rust && cargo test -p confluence-core -- --test-threads=1`

Expected: all tests pass (error, config, format, url_parse, client_auth, client_retry, client_endpoints).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/confluence-core
git commit -m "core: add search / get_page / get_page_by_title / get_child endpoints"
```

---

## Task 9: `server` crate — rmcp stdio scaffold

**Files:**
- Create: `rust/crates/server/Cargo.toml`
- Create: `rust/crates/server/src/main.rs`
- Create: `rust/crates/server/src/handler.rs`
- Create: `rust/crates/server/src/format.rs`

- [ ] **Step 1: Write Cargo.toml**

Write `rust/crates/server/Cargo.toml`:

```toml
[package]
name = "server"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "confluence-mcp-server"
path = "src/main.rs"

[dependencies]
confluence-core = { path = "../confluence-core" }
rmcp = { version = "0.8", features = ["server", "macros", "transport-io"] }
serde = { workspace = true }
serde_json = { workspace = true }
schemars = "0.8"
tokio = { workspace = true, features = ["rt-multi-thread", "io-std"] }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = "1"

[dev-dependencies]
wiremock = "0.6"
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Write minimal stdio main**

Write `rust/crates/server/src/main.rs`:

```rust
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
```

- [ ] **Step 3: Write minimal handler with no tools yet**

Write `rust/crates/server/src/handler.rs`:

```rust
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
                version: env!("CARGO_PKG_VERSION").into(),
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
```

- [ ] **Step 4: Write format helpers used by tool handlers**

Write `rust/crates/server/src/format.rs`:

```rust
use serde_json::Value;

pub fn labels_string(page: &Value) -> String {
    let empty = vec![];
    let labels = page.pointer("/metadata/labels/results").and_then(Value::as_array).unwrap_or(&empty);
    labels.iter().filter_map(|l| l["name"].as_str()).collect::<Vec<_>>().join(", ")
}

pub fn ancestors_string(page: &Value) -> String {
    let empty = vec![];
    let ancestors = page.pointer("/ancestors").and_then(Value::as_array).unwrap_or(&empty);
    ancestors.iter().filter_map(|a| a["title"].as_str()).collect::<Vec<_>>().join(" → ")
}

pub fn page_url(page: &Value, fallback_base: &str) -> String {
    let base = page.pointer("/_links/base").and_then(Value::as_str).unwrap_or(fallback_base);
    let webui = page.pointer("/_links/webui").and_then(Value::as_str).unwrap_or("");
    if webui.is_empty() { String::new() } else { format!("{base}{webui}") }
}

pub fn error_response(err: &confluence_core::ConfluenceError) -> String {
    use confluence_core::ConfluenceError::*;
    match err {
        Http { status, message } => format!("❌ Confluence API error (HTTP {status}): {message}"),
        other => format!("❌ {other}"),
    }
}
```

- [ ] **Step 5: Confirm crate compiles with no tools registered**

Run: `cd rust && cargo build -p server`

Expected: build succeeds with warnings about unused imports in `format.rs` (those are used by later tasks). If `schemars` version mismatch with `rmcp`'s internal schemars appears, align the version by removing our own `schemars` dep and relying on `rmcp`'s re-export (`rmcp::schemars`).

- [ ] **Step 6: Commit**

```bash
git add rust/crates/server
git commit -m "server: scaffold rmcp stdio server with no tools"
```

---

## Task 10: `server` tool — `list_spaces`

**Files:**
- Create: `rust/crates/server/src/tools/mod.rs`
- Create: `rust/crates/server/src/tools/list_spaces.rs`
- Modify: `rust/crates/server/src/handler.rs`

Per rmcp pattern: tools are `#[tool(...)]`-annotated methods on the handler struct. For clarity, each tool's formatting logic lives in its own module.

- [ ] **Step 1: Create tools module**

Write `rust/crates/server/src/tools/mod.rs`:

```rust
pub mod list_spaces;
```

- [ ] **Step 2: Write the formatter with tests**

Write `rust/crates/server/src/tools/list_spaces.rs`:

```rust
use serde_json::Value;

pub fn format(response: &Value) -> String {
    let empty = vec![];
    let spaces = response.pointer("/results").and_then(Value::as_array).unwrap_or(&empty);
    if spaces.is_empty() {
        return "No spaces found.".into();
    }
    let mut lines = vec![format!("## Confluence Spaces ({} found)\n", spaces.len())];
    for s in spaces {
        let name = s["name"].as_str().unwrap_or("?");
        let key = s["key"].as_str().unwrap_or("?");
        let stype = s["type"].as_str().unwrap_or("?");
        let desc = s.pointer("/description/plain/value").and_then(Value::as_str).unwrap_or("").trim();
        let desc_preview = if desc.chars().count() > 80 {
            format!("{}…", desc.chars().take(80).collect::<String>())
        } else {
            desc.to_string()
        };
        let mut entry = format!("- **{name}** — key: `{key}` ({stype})");
        if !desc_preview.is_empty() { entry.push_str(&format!("\n  {desc_preview}")); }
        lines.push(entry);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_results_returns_placeholder() {
        let r = format(&json!({"results": []}));
        assert_eq!(r, "No spaces found.");
    }

    #[test]
    fn lists_spaces_with_names_and_keys() {
        let r = format(&json!({
            "results": [
                {"name": "Dev", "key": "DEV", "type": "global"},
                {"name": "HR",  "key": "HR",  "type": "global"}
            ]
        }));
        assert!(r.contains("Dev"));
        assert!(r.contains("DEV"));
        assert!(r.contains("HR"));
    }

    #[test]
    fn truncates_long_descriptions() {
        let long = "x".repeat(100);
        let r = format(&json!({
            "results": [{"name": "A", "key": "A", "type": "global",
                         "description": {"plain": {"value": long}}}]
        }));
        assert!(r.contains("…"));
    }
}
```

- [ ] **Step 3: Register `tools` module in main and add tool handler**

Modify `rust/crates/server/src/main.rs`: add `mod tools;` near `mod handler;`.

Modify `rust/crates/server/src/handler.rs` — inside the `#[tool_router] impl ConfluenceServer` block (before the closing brace), add:

```rust
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
```

And at the top of `handler.rs`, add:

```rust
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSpacesArgs {
    /// Filter by space type — 'global', 'personal', or 'all'. Defaults to 'global'.
    #[serde(rename = "type")]
    pub space_type: Option<String>,
    /// Maximum spaces to return (default 50).
    pub limit: Option<u32>,
}
```

- [ ] **Step 4: Write integration test against the formatter and client**

Create `rust/crates/server/tests/list_spaces.rs`:

```rust
use serde_json::json;

#[test]
fn list_spaces_format_includes_global_and_personal() {
    // Re-export under server crate by making the module public is out of scope;
    // instead test via the shared formatter function exposed below.
    // If server::tools isn't publicly accessible, move the formatter to confluence-core
    // in a follow-up task.
    let input = json!({"results": [
        {"name": "Team", "key": "TEAM", "type": "global"}
    ]});
    // This test lives here as a placeholder; the real unit tests sit in the module
    // next to the formatter in src/tools/list_spaces.rs.
    let _ = input;
}
```

(Note: unit tests in `src/tools/list_spaces.rs` cover the formatter. This file is a stub reserved for later end-to-end MCP tests if needed.)

- [ ] **Step 5: Build and test**

Run: `cd rust && cargo test -p server list_spaces`

Expected: 3 formatter tests pass. `cargo build -p server` succeeds.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/server
git commit -m "server: implement list_spaces tool"
```

---

## Task 11: `server` tool — `search_confluence`

**Files:**
- Create: `rust/crates/server/src/tools/search_confluence.rs`
- Modify: `rust/crates/server/src/tools/mod.rs`
- Modify: `rust/crates/server/src/handler.rs`

- [ ] **Step 1: Write the formatter with tests**

Write `rust/crates/server/src/tools/search_confluence.rs`:

```rust
use serde_json::Value;
use crate::format::{labels_string, page_url};

pub fn format(response: &Value, fallback_base: &str) -> String {
    let empty = vec![];
    let results = response.pointer("/results").and_then(Value::as_array).unwrap_or(&empty);
    if results.is_empty() {
        return "No results found for that query. Try broadening your CQL or check the space key.".into();
    }
    let total = response.pointer("/totalSize").and_then(Value::as_u64).unwrap_or(results.len() as u64);
    let mut lines = vec![format!("Found {total} result(s) — showing {}:\n", results.len())];
    for (i, page) in results.iter().enumerate() {
        let title = page["title"].as_str().unwrap_or("?");
        let id = page["id"].as_str().unwrap_or("?");
        let space_key = page.pointer("/space/key").and_then(Value::as_str).unwrap_or("?");
        let version = page.pointer("/version/number").and_then(Value::as_u64).map(|n| n.to_string()).unwrap_or_else(|| "?".into());
        let labels = labels_string(page);
        let url = page_url(page, fallback_base);

        let mut entry = format!(
            "{}. **{title}**\n   ID: {id} | Space: {space_key} | v{version}",
            i + 1,
        );
        if !labels.is_empty() { entry.push_str(&format!(" | Labels: {labels}")); }
        if !url.is_empty() { entry.push_str(&format!("\n   URL: {url}")); }
        lines.push(entry);
    }
    lines.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_results_returns_placeholder() {
        let r = format(&json!({"results": [], "totalSize": 0}), "http://wiki");
        assert!(r.contains("No results found"));
    }

    #[test]
    fn includes_total_and_numbering() {
        let r = format(&json!({
            "results": [
                {"id": "1", "title": "A", "space": {"key": "S"}, "version": {"number": 1}},
                {"id": "2", "title": "B", "space": {"key": "S"}, "version": {"number": 2}}
            ],
            "totalSize": 2
        }), "http://wiki");
        assert!(r.contains("Found 2 result"));
        assert!(r.contains("1. **A**"));
        assert!(r.contains("2. **B**"));
    }
}
```

- [ ] **Step 2: Register module and tool handler**

Modify `rust/crates/server/src/tools/mod.rs`:

```rust
pub mod list_spaces;
pub mod search_confluence;
```

In `rust/crates/server/src/handler.rs`, add struct and tool handler:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// A CQL query string (e.g. 'type=page AND text~"deployment"').
    pub cql: String,
    /// Maximum results to return (1–50, default 10).
    pub limit: Option<u32>,
}
```

And inside the `#[tool_router] impl` block:

```rust
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
```

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p server search_confluence`

Expected: 2 formatter tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/server
git commit -m "server: implement search_confluence tool"
```

---

## Task 12: `server` tool — `get_page`

**Files:**
- Create: `rust/crates/server/src/tools/get_page.rs`
- Modify: `rust/crates/server/src/tools/mod.rs`
- Modify: `rust/crates/server/src/handler.rs`

- [ ] **Step 1: Write the formatter with tests**

Write `rust/crates/server/src/tools/get_page.rs`:

```rust
use confluence_core::{strip_html, truncate};
use serde_json::Value;
use crate::format::{ancestors_string, labels_string, page_url};

pub fn format(page: &Value, body_format: &str, include_body: bool, fallback_base: &str, max_len: usize) -> String {
    let title = page["title"].as_str().unwrap_or("?");
    let space_name = page.pointer("/space/name").and_then(Value::as_str).unwrap_or("");
    let space_key = page.pointer("/space/key").and_then(Value::as_str).unwrap_or("");
    let version = page.pointer("/version/number").and_then(Value::as_u64).map(|n| n.to_string()).unwrap_or_default();
    let labels = labels_string(page);
    let ancestors = ancestors_string(page);
    let url = page_url(page, fallback_base);

    let mut header = format!("# {title}\n");
    header.push_str(&format!("Space: {space_name} ({space_key}) | Version: {version}\n"));
    if !labels.is_empty() { header.push_str(&format!("Labels: {labels}\n")); }
    if !ancestors.is_empty() { header.push_str(&format!("Path: {ancestors} → {title}\n")); }
    if !url.is_empty() { header.push_str(&format!("URL: {url}\n")); }

    if !include_body {
        return header;
    }

    let raw_body = page.pointer(&format!("/body/{body_format}/value"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let body = if body_format == "view" { strip_html(raw_body) } else { raw_body.to_string() };
    let body = truncate(&body, max_len);
    format!("{header}\n---\n\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn header_includes_title_space_version() {
        let page = json!({
            "title": "My Page", "id": "1",
            "space": {"name": "Dev", "key": "DEV"},
            "version": {"number": 3}
        });
        let r = format(&page, "storage", false, "http://wiki", 50_000);
        assert!(r.contains("# My Page"));
        assert!(r.contains("Space: Dev (DEV)"));
        assert!(r.contains("Version: 3"));
    }

    #[test]
    fn body_included_when_requested() {
        let page = json!({
            "title": "T", "space": {"name": "S", "key": "S"}, "version": {"number": 1},
            "body": {"storage": {"value": "<p>hello</p>"}}
        });
        let r = format(&page, "storage", true, "http://wiki", 50_000);
        assert!(r.contains("<p>hello</p>"));
    }

    #[test]
    fn view_format_strips_html() {
        let page = json!({
            "title": "T", "space": {"name": "S", "key": "S"}, "version": {"number": 1},
            "body": {"view": {"value": "<p>hello</p>"}}
        });
        let r = format(&page, "view", true, "http://wiki", 50_000);
        assert!(r.contains("hello"));
        assert!(!r.contains("<p>"));
    }
}
```

- [ ] **Step 2: Register module and tool handler**

Add `pub mod get_page;` to `rust/crates/server/src/tools/mod.rs`.

In `handler.rs`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPageArgs {
    /// The numeric page ID (e.g. '3965072').
    pub page_id: String,
    /// Body format — 'storage' (raw XHTML) or 'view' (rendered HTML). Default 'storage'.
    pub format: Option<String>,
    /// Set false to fetch only metadata. Default true.
    pub include_body: Option<bool>,
}
```

And inside the `#[tool_router] impl`:

```rust
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
```

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p server get_page`

Expected: 3 formatter tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/server
git commit -m "server: implement get_page tool"
```

---

## Task 13: `server` tool — `get_page_by_title`

**Files:**
- Create: `rust/crates/server/src/tools/get_page_by_title.rs`
- Modify: `rust/crates/server/src/tools/mod.rs`
- Modify: `rust/crates/server/src/handler.rs`

- [ ] **Step 1: Write the formatter with tests**

Write `rust/crates/server/src/tools/get_page_by_title.rs`:

```rust
use confluence_core::truncate;
use serde_json::Value;
use crate::format::{ancestors_string, labels_string, page_url};

pub fn format_not_found(space_key: &str, title: &str) -> String {
    format!(
        "No page titled '{title}' found in space {space_key}.\n\
         Tip: titles are case-sensitive and must be exact. \
         Try search_confluence with: title~\"{title}\" AND space={space_key}"
    )
}

pub fn format_found(page: &Value, space_key: &str, fallback_base: &str, max_len: usize) -> String {
    let title = page["title"].as_str().unwrap_or("?");
    let id = page["id"].as_str().unwrap_or("?");
    let space_name = page.pointer("/space/name").and_then(Value::as_str).unwrap_or("");
    let version = page.pointer("/version/number").and_then(Value::as_u64).map(|n| n.to_string()).unwrap_or_default();
    let labels = labels_string(page);
    let ancestors = ancestors_string(page);
    let url = page_url(page, fallback_base);
    let body = page.pointer("/body/storage/value").and_then(Value::as_str).unwrap_or("");

    let mut header = format!("# {title}\n");
    header.push_str(&format!("ID: {id} | Space: {space_name} ({space_key}) | Version: {version}\n"));
    if !labels.is_empty() { header.push_str(&format!("Labels: {labels}\n")); }
    if !ancestors.is_empty() { header.push_str(&format!("Path: {ancestors} → {title}\n")); }
    if !url.is_empty() { header.push_str(&format!("URL: {url}\n")); }

    format!("{header}\n---\n\n{}", truncate(body, max_len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn not_found_message_mentions_search() {
        let r = format_not_found("DEV", "Runbook");
        assert!(r.contains("No page titled 'Runbook'"));
        assert!(r.contains("search_confluence"));
    }

    #[test]
    fn found_includes_body() {
        let page = json!({
            "title": "R", "id": "1",
            "space": {"name": "Dev", "key": "DEV"},
            "version": {"number": 1},
            "body": {"storage": {"value": "<p>content</p>"}}
        });
        let r = format_found(&page, "DEV", "http://wiki", 50_000);
        assert!(r.contains("# R"));
        assert!(r.contains("<p>content</p>"));
    }
}
```

- [ ] **Step 2: Register module and tool handler**

Add `pub mod get_page_by_title;` to `tools/mod.rs`.

In `handler.rs`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPageByTitleArgs {
    /// The space key (e.g. 'DEV', 'TEAM', 'HR').
    pub space_key: String,
    /// The exact page title to look for.
    pub title: String,
}
```

And inside `#[tool_router] impl`:

```rust
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
```

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p server get_page_by_title`

Expected: 2 formatter tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/server
git commit -m "server: implement get_page_by_title tool"
```

---

## Task 14: `server` tool — `get_page_by_url`

**Files:**
- Create: `rust/crates/server/src/tools/get_page_by_url.rs`
- Modify: `rust/crates/server/src/tools/mod.rs`
- Modify: `rust/crates/server/src/handler.rs`

- [ ] **Step 1: Write handler + formatter with tests**

Write `rust/crates/server/src/tools/get_page_by_url.rs`:

```rust
use confluence_core::{parse_confluence_url, ParsedUrl};

pub enum UrlResolution {
    ById(String),
    BySpaceTitle { space: String, title: String },
    TinyUrl(String),
    Unparseable,
}

pub fn resolve(url: &str) -> UrlResolution {
    let p: ParsedUrl = parse_confluence_url(url);
    if let Some(id) = &p.page_id {
        if id.starts_with("tinyurl:") {
            return UrlResolution::TinyUrl(url.to_string());
        }
        return UrlResolution::ById(id.clone());
    }
    match (p.space_key, p.title) {
        (Some(s), Some(t)) => UrlResolution::BySpaceTitle { space: s, title: t },
        _ => UrlResolution::Unparseable,
    }
}

pub fn format_unparseable(url: &str) -> String {
    format!(
        "❌ Could not parse the Confluence URL: {url}\n\n\
         Supported formats:\n\
           - http://confluence/pages/viewpage.action?pageId=12345\n\
           - http://confluence/display/SPACEKEY/Page+Title\n\
           - http://confluence/spaces/SPACEKEY/pages/12345/Title\n\n\
         Try using get_page(page_id) or get_page_by_title(space_key, title) directly."
    )
}

pub fn format_tiny_url() -> String {
    "❌ Tiny URLs (/x/...) need server-side resolution.\n\
     Try opening the link in a browser first to get the full URL, then paste that instead."
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_page_id() {
        let r = resolve("http://wiki/pages/12345");
        assert!(matches!(r, UrlResolution::ById(id) if id == "12345"));
    }

    #[test]
    fn resolves_space_title() {
        let r = resolve("http://wiki/display/DEV/My+Page");
        assert!(matches!(r, UrlResolution::BySpaceTitle { space, title } if space == "DEV" && title == "My Page"));
    }

    #[test]
    fn resolves_tiny_url() {
        let r = resolve("http://wiki/x/abc");
        assert!(matches!(r, UrlResolution::TinyUrl(_)));
    }

    #[test]
    fn unparseable_url_returns_variant() {
        let r = resolve("not a url");
        assert!(matches!(r, UrlResolution::Unparseable));
    }
}
```

- [ ] **Step 2: Register module and tool handler**

Add `pub mod get_page_by_url;` to `tools/mod.rs`.

In `handler.rs`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPageByUrlArgs {
    /// Any Confluence page URL (full or relative path).
    pub url: String,
    /// Body format — 'storage' (raw XHTML) or 'view' (rendered HTML). Default 'storage'.
    pub format: Option<String>,
}
```

And inside `#[tool_router] impl`:

```rust
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
```

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p server get_page_by_url`

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/server
git commit -m "server: implement get_page_by_url tool"
```

---

## Task 15: `server` tool — `get_comments`

**Files:**
- Create: `rust/crates/server/src/tools/get_comments.rs`
- Modify: `rust/crates/server/src/tools/mod.rs`
- Modify: `rust/crates/server/src/handler.rs`

- [ ] **Step 1: Write the formatter with tests**

Write `rust/crates/server/src/tools/get_comments.rs`:

```rust
use confluence_core::strip_html;
use serde_json::Value;

pub fn format(response: &Value) -> String {
    let empty = vec![];
    let comments = response.pointer("/results").and_then(Value::as_array).unwrap_or(&empty);
    if comments.is_empty() {
        return "No comments on this page.".into();
    }
    let mut lines = vec![format!("## Comments ({})\n", comments.len())];
    for c in comments {
        let author = c.pointer("/version/by/displayName").and_then(Value::as_str).unwrap_or("Unknown");
        let when = c.pointer("/version/when").and_then(Value::as_str).unwrap_or("");
        let location = c.pointer("/extensions/location").and_then(Value::as_str).unwrap_or("footer");
        let raw = c.pointer("/body/view/value").and_then(Value::as_str).unwrap_or("");
        let body = strip_html(raw);
        let mut entry = format!("**{author}** ({location})");
        if !when.is_empty() { entry.push_str(&format!(" — {when}")); }
        entry.push_str(&format!("\n{body}"));
        lines.push(entry);
    }
    lines.join("\n\n---\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_results_returns_placeholder() {
        let r = format(&json!({"results": []}));
        assert_eq!(r, "No comments on this page.");
    }

    #[test]
    fn renders_author_location_body() {
        let r = format(&json!({
            "results": [{
                "version": {"by": {"displayName": "Alice"}, "when": "2026-04-01"},
                "extensions": {"location": "inline"},
                "body": {"view": {"value": "<p>hi</p>"}}
            }]
        }));
        assert!(r.contains("**Alice** (inline)"));
        assert!(r.contains("hi"));
        assert!(r.contains("2026-04-01"));
    }
}
```

- [ ] **Step 2: Register module and tool handler**

Add `pub mod get_comments;` to `tools/mod.rs`.

In `handler.rs`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCommentsArgs {
    /// The numeric page ID.
    pub page_id: String,
    /// Maximum comments to return (default 25).
    pub limit: Option<u32>,
}
```

And inside `#[tool_router] impl`:

```rust
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
```

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p server get_comments`

Expected: 2 formatter tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/server
git commit -m "server: implement get_comments tool"
```

---

## Task 16: `server` tool — `get_attachments`

**Files:**
- Create: `rust/crates/server/src/tools/get_attachments.rs`
- Modify: `rust/crates/server/src/tools/mod.rs`
- Modify: `rust/crates/server/src/handler.rs`

- [ ] **Step 1: Write the formatter with tests**

Write `rust/crates/server/src/tools/get_attachments.rs`:

```rust
use serde_json::Value;

pub fn format(response: &Value, base_url: &str) -> String {
    let empty = vec![];
    let atts = response.pointer("/results").and_then(Value::as_array).unwrap_or(&empty);
    if atts.is_empty() {
        return "No attachments on this page.".into();
    }
    let mut lines = vec![format!("## Attachments ({})\n", atts.len())];
    for a in atts {
        let title = a["title"].as_str().unwrap_or("?");
        let media = a.pointer("/metadata/mediaType").and_then(Value::as_str).unwrap_or("unknown");
        let download = a.pointer("/_links/download").and_then(Value::as_str).unwrap_or("");
        let full_url = if download.is_empty() { "N/A".into() } else { format!("{base_url}{download}") };
        let version = a.pointer("/version/number").and_then(Value::as_u64).map(|n| n.to_string()).unwrap_or_else(|| "?".into());
        lines.push(format!(
            "- **{title}**\n  Type: {media} | Version: {version}\n  Download: {full_url}"
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_results_returns_placeholder() {
        let r = format(&json!({"results": []}), "http://wiki");
        assert_eq!(r, "No attachments on this page.");
    }

    #[test]
    fn renders_title_type_download() {
        let r = format(&json!({
            "results": [{
                "title": "spec.pdf",
                "metadata": {"mediaType": "application/pdf"},
                "_links": {"download": "/download/attachments/1/spec.pdf"},
                "version": {"number": 2}
            }]
        }), "http://wiki");
        assert!(r.contains("spec.pdf"));
        assert!(r.contains("application/pdf"));
        assert!(r.contains("http://wiki/download/attachments/1/spec.pdf"));
    }
}
```

- [ ] **Step 2: Register module and tool handler**

Add `pub mod get_attachments;` to `tools/mod.rs`.

In `handler.rs`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetAttachmentsArgs {
    /// The numeric page ID.
    pub page_id: String,
    /// Maximum attachments to return (default 50).
    pub limit: Option<u32>,
}
```

And inside `#[tool_router] impl`:

```rust
#[tool(description = "List file attachments on a Confluence page with download URLs.")]
async fn get_attachments(
    &self,
    Parameters(args): Parameters<GetAttachmentsArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let limit = args.limit.unwrap_or(50);
    match self.client.get_child(&args.page_id, "attachment", "version", limit).await {
        Ok(data) => Ok(CallToolResult::success(vec![Content::text(crate::tools::get_attachments::format(&data, &self.config.confluence_url))])),
        Err(e)   => Ok(CallToolResult::success(vec![Content::text(crate::format::error_response(&e))])),
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd rust && cargo test -p server get_attachments`

Expected: 2 formatter tests pass.

- [ ] **Step 4: Run full server test suite**

Run: `cd rust && cargo test -p server -- --test-threads=1`

Expected: all formatter tests pass (list_spaces, search_confluence, get_page, get_page_by_title, get_page_by_url, get_comments, get_attachments).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/server
git commit -m "server: implement get_attachments tool"
```

---

## Task 17: Build + measure server binary; establish size baseline

**Files:** (no new files)

- [ ] **Step 1: Build release binary**

Run: `cd rust && cargo build --release -p server`

Expected: `rust/target/release/confluence-mcp-server.exe` exists.

- [ ] **Step 2: Measure unstripped, non-UPX size**

Run (PowerShell): `Get-Item rust/target/release/confluence-mcp-server.exe | Select-Object Length`

Record the size in the next commit message. Expected range: 4–8 MB given the release profile settings.

- [ ] **Step 3: Apply UPX compression**

Run: `./tools/upx-4.2.4-win64/upx.exe --best rust/target/release/confluence-mcp-server.exe`

Expected output ends with "Packed 1 file." Measure the new size.

- [ ] **Step 4: Sanity-check the server binary runs**

Run: `cd rust && cargo run --release -p server` with env `CONFLUENCE_URL=http://nonexistent CONFLUENCE_TOKEN=x` — server should start and hang awaiting stdio (press Ctrl+C to exit). If it crashes immediately, look for panics; otherwise it's working. (MCP protocol handshake is tested later with real Claude Desktop integration in Task 28.)

- [ ] **Step 5: Commit the size measurement in a no-code commit**

```bash
git commit --allow-empty -m "server: baseline release binary size <X MB unstripped / <Y MB UPX>"
```

(Replace `<X>` and `<Y>` with actual measured values. This is a durable record for the spec's size-gate calibration.)

---

## Task 18: `configurator` crate — Tauri 2 scaffold

**Files:**
- Create: `rust/crates/configurator/Cargo.toml`
- Create: `rust/crates/configurator/tauri.conf.json`
- Create: `rust/crates/configurator/build.rs`
- Create: `rust/crates/configurator/src/main.rs`
- Create: `rust/crates/configurator/ui/index.html`
- Create: `rust/crates/configurator/ui/style.css`
- Create: `rust/crates/configurator/ui/app.js`
- Create: `rust/crates/configurator/resources/.gitkeep`

- [ ] **Step 1: Write Cargo.toml**

Write `rust/crates/configurator/Cargo.toml`:

```toml
[package]
name = "configurator"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "ConfluenceMCPSetup"
path = "src/main.rs"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
confluence-core = { path = "../confluence-core" }
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
```

- [ ] **Step 2: Write tauri.conf.json**

Write `rust/crates/configurator/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "ConfluenceMCPSetup",
  "version": "0.1.0",
  "identifier": "com.confluence-mcp.setup",
  "build": {
    "frontendDist": "ui",
    "devUrl": "ui"
  },
  "app": {
    "windows": [
      {
        "title": "Confluence MCP Setup",
        "width": 600,
        "height": 720,
        "resizable": false
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/icon.ico"]
  }
}
```

- [ ] **Step 3: Write build.rs**

Write `rust/crates/configurator/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 4: Write minimal main.rs**

Write `rust/crates/configurator/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("failed to start Tauri app");
}
```

- [ ] **Step 5: Write minimal UI stub**

Write `rust/crates/configurator/ui/index.html`:

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>Confluence MCP Setup</title>
    <link rel="stylesheet" href="style.css" />
  </head>
  <body>
    <main>
      <h1>Confluence MCP Setup</h1>
      <p>Wizard UI will be built in Task 24.</p>
    </main>
    <script src="app.js"></script>
  </body>
</html>
```

Write `rust/crates/configurator/ui/style.css`:

```css
* { box-sizing: border-box; }
body { font-family: -apple-system, "Segoe UI", sans-serif; margin: 0; padding: 24px; background: #fafafa; }
main { max-width: 540px; margin: 0 auto; }
h1 { font-size: 20px; font-weight: 600; margin: 0 0 12px; }
```

Write `rust/crates/configurator/ui/app.js`:

```js
// Wizard frontend — commands wired up in Task 24.
```

Create placeholder for the resources directory:

```bash
touch rust/crates/configurator/resources/.gitkeep
```

- [ ] **Step 6: Build to confirm Tauri toolchain works**

Run: `cd rust && cargo build -p configurator`

Expected: first build downloads Tauri deps (may take 5+ minutes). Produces `rust/target/debug/ConfluenceMCPSetup.exe`.

If the build fails because `icons/icon.ico` is missing, temporarily remove the `icon` line from `tauri.conf.json`. We'll restore it in Task 27.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/configurator
git commit -m "configurator: scaffold Tauri 2 application"
```

---

## Task 19: `configurator` — claude_desktop_config.json read/write module

**Files:**
- Create: `rust/crates/configurator/src/claude_config.rs`
- Create: `rust/crates/configurator/tests/claude_config.rs`

- [ ] **Step 1: Write the failing tests**

Write `rust/crates/configurator/tests/claude_config.rs`:

```rust
use configurator::claude_config::{read_config, write_confluence_entry, remove_confluence_entry, ConfluenceEntry};
use serde_json::json;
use tempfile::TempDir;
use std::fs;

fn tmp(initial: Option<&str>) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("claude_desktop_config.json");
    if let Some(text) = initial {
        fs::write(&path, text).unwrap();
    }
    (dir, path)
}

#[test]
fn reads_existing_confluence_entry() {
    let (_dir, path) = tmp(Some(r#"{
        "mcpServers": {
            "confluence": {
                "command": "C:\\\\app\\\\server.exe",
                "args": [],
                "env": {"CONFLUENCE_URL": "https://wiki.example.com", "CONFLUENCE_TOKEN": "t"}
            }
        }
    }"#));
    let existing = read_config(&path).unwrap();
    assert!(existing.confluence.is_some());
    let c = existing.confluence.unwrap();
    assert_eq!(c.url, "https://wiki.example.com");
    assert_eq!(c.token.as_deref(), Some("t"));
}

#[test]
fn write_preserves_other_mcp_servers() {
    let (_dir, path) = tmp(Some(r#"{
        "mcpServers": {
            "other": {"command": "C:\\\\other.exe", "args": []}
        }
    }"#));
    let entry = ConfluenceEntry {
        command: r"C:\app\server.exe".into(),
        url: "https://wiki".into(),
        username: None,
        password: None,
        token: Some("t".into()),
        ssl_verify: true,
    };
    write_confluence_entry(&path, &entry).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(parsed.pointer("/mcpServers/other").is_some());
    assert_eq!(parsed.pointer("/mcpServers/confluence/env/CONFLUENCE_TOKEN").unwrap(), "t");
}

#[test]
fn malformed_config_is_backed_up_and_replaced() {
    let (_dir, path) = tmp(Some("this is not json"));
    let entry = ConfluenceEntry {
        command: r"C:\server.exe".into(),
        url: "https://wiki".into(),
        username: None, password: None,
        token: Some("t".into()),
        ssl_verify: true,
    };
    write_confluence_entry(&path, &entry).unwrap();

    // A malformed backup file should exist alongside
    let dir = path.parent().unwrap();
    let backups: Vec<_> = fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("malformed"))
        .collect();
    assert_eq!(backups.len(), 1);
}

#[test]
fn remove_deletes_entry() {
    let (_dir, path) = tmp(Some(r#"{"mcpServers": {"confluence": {"command": "x"}, "other": {"command": "y"}}}"#));
    remove_confluence_entry(&path).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(parsed.pointer("/mcpServers/confluence").is_none());
    assert!(parsed.pointer("/mcpServers/other").is_some());
}
```

- [ ] **Step 2: Implement `claude_config`**

Write `rust/crates/configurator/src/claude_config.rs`:

```rust
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ConfluenceEntry {
    pub command: String,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
    pub ssl_verify: bool,
}

#[derive(Debug, Default)]
pub struct ExistingConfig {
    pub path_exists: bool,
    pub confluence: Option<ConfluenceEntry>,
}

/// Default config path for the current platform.
pub fn default_config_path() -> PathBuf {
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        appdata.join("Claude").join("claude_desktop_config.json")
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join(".config").join("Claude").join("claude_desktop_config.json")
    }
}

pub fn read_config(path: &Path) -> std::io::Result<ExistingConfig> {
    let mut out = ExistingConfig::default();
    if !path.is_file() { return Ok(out); }
    out.path_exists = true;

    let raw = fs::read_to_string(path)?;
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(out),
    };
    let confluence = parsed.pointer("/mcpServers/confluence").cloned();
    if let Some(c) = confluence {
        let env = c.pointer("/env").cloned().unwrap_or(Value::Null);
        let cmd = c.pointer("/command").and_then(Value::as_str).unwrap_or("").to_string();
        out.confluence = Some(ConfluenceEntry {
            command: cmd,
            url: env.pointer("/CONFLUENCE_URL").and_then(Value::as_str).unwrap_or("").to_string(),
            username: env.pointer("/CONFLUENCE_USERNAME").and_then(Value::as_str).map(String::from),
            password: env.pointer("/CONFLUENCE_PASSWORD").and_then(Value::as_str).map(String::from),
            token: env.pointer("/CONFLUENCE_TOKEN").and_then(Value::as_str).map(String::from),
            ssl_verify: env.pointer("/CONFLUENCE_SSL_VERIFY")
                .and_then(Value::as_str)
                .map(|v| v != "false")
                .unwrap_or(true),
        });
    }
    Ok(out)
}

pub fn write_confluence_entry(path: &Path, entry: &ConfluenceEntry) -> std::io::Result<()> {
    let mut doc: Value = if path.is_file() {
        match fs::read_to_string(path).ok().as_deref().and_then(|s| serde_json::from_str(s).ok()) {
            Some(v) => v,
            None => {
                // Malformed — back up and start fresh
                let ts = chrono_now();
                let backup = path.with_extension(format!("json.malformed.{ts}"));
                let _ = fs::copy(path, &backup);
                json!({"mcpServers": {}})
            }
        }
    } else {
        json!({"mcpServers": {}})
    };

    // Backup good existing config
    if path.is_file() && doc.pointer("/mcpServers").is_some() {
        let _ = fs::copy(path, path.with_extension("json.backup"));
    }

    let mut env = Map::new();
    env.insert("CONFLUENCE_URL".into(), json!(entry.url.trim_end_matches('/')));
    env.insert("MCP_TRANSPORT".into(), json!("stdio"));
    if let Some(t) = &entry.token {
        env.insert("CONFLUENCE_TOKEN".into(), json!(t));
    } else {
        if let Some(u) = &entry.username { env.insert("CONFLUENCE_USERNAME".into(), json!(u)); }
        if let Some(p) = &entry.password { env.insert("CONFLUENCE_PASSWORD".into(), json!(p)); }
    }
    if !entry.ssl_verify {
        env.insert("CONFLUENCE_SSL_VERIFY".into(), json!("false"));
    }

    let server_entry = json!({
        "command": entry.command,
        "args": [],
        "env": env,
    });

    if doc.get("mcpServers").is_none() {
        doc["mcpServers"] = json!({});
    }
    doc["mcpServers"]["confluence"] = server_entry;

    atomic_write(path, &doc)
}

pub fn remove_confluence_entry(path: &Path) -> std::io::Result<()> {
    if !path.is_file() { return Ok(()); }
    let mut doc: Value = match fs::read_to_string(path).ok().as_deref().and_then(|s| serde_json::from_str(s).ok()) {
        Some(v) => v,
        None => return Ok(()),
    };
    if let Some(servers) = doc.get_mut("mcpServers").and_then(Value::as_object_mut) {
        servers.remove("confluence");
    }
    atomic_write(path, &doc)
}

fn atomic_write(path: &Path, doc: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(doc).unwrap();
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
    }
    fs::rename(&tmp, path)
}

fn chrono_now() -> String {
    // Avoid pulling in a date crate; use seconds-since-epoch as a unique suffix.
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).to_string()
}
```

- [ ] **Step 3: Expose `claude_config` via lib target**

The configurator is a binary crate; to allow integration tests under `tests/` to import `configurator::claude_config`, add a library target.

Modify `rust/crates/configurator/Cargo.toml` — add a `[lib]` section AND keep the existing `[[bin]]`:

```toml
[lib]
name = "configurator"
path = "src/lib.rs"

[[bin]]
name = "ConfluenceMCPSetup"
path = "src/main.rs"
```

Create `rust/crates/configurator/src/lib.rs`:

```rust
pub mod claude_config;
```

Update `rust/crates/configurator/src/main.rs` to `use configurator::claude_config` when needed in later tasks (no change required yet).

Add `tempfile` as a dev-dep in `Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Run tests**

Run: `cd rust && cargo test -p configurator --test claude_config`

Expected: all 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/configurator
git commit -m "configurator: add claude_desktop_config read/write with backup"
```

---

## Task 20: `configurator` — installer module (path resolution + writability probe)

**Files:**
- Create: `rust/crates/configurator/src/installer.rs`
- Modify: `rust/crates/configurator/src/lib.rs`
- Create: `rust/crates/configurator/tests/installer.rs`

- [ ] **Step 1: Write failing tests**

Write `rust/crates/configurator/tests/installer.rs`:

```rust
use configurator::installer::{probe_writable, default_install_dir, resolve_install_dir};
use tempfile::TempDir;

#[test]
fn probe_succeeds_on_writable_dir() {
    let d = TempDir::new().unwrap();
    assert!(probe_writable(d.path()).is_ok());
}

#[test]
fn probe_creates_missing_parent_then_succeeds() {
    let d = TempDir::new().unwrap();
    let nested = d.path().join("a/b/c");
    assert!(probe_writable(&nested).is_ok());
    assert!(nested.is_dir());
}

#[test]
fn default_install_dir_is_under_known_variable() {
    let p = default_install_dir();
    // On Windows, path is under LOCALAPPDATA or USERPROFILE; on Linux/Mac under home.
    let s = p.to_string_lossy();
    assert!(s.ends_with("ConfluenceMCP"), "unexpected path: {s}");
}

#[test]
fn resolve_install_dir_uses_override() {
    let d = TempDir::new().unwrap();
    let got = resolve_install_dir(Some(d.path().to_string_lossy().to_string()));
    assert_eq!(got, d.path());
}
```

- [ ] **Step 2: Implement**

Write `rust/crates/configurator/src/installer.rs`:

```rust
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const SERVER_BINARY_NAME: &str = "confluence-mcp-server.exe";

/// The server binary embedded at compile time.
const EMBEDDED_SERVER: &[u8] = include_bytes!("../resources/confluence-mcp-server.exe");

/// Ordered candidate paths for the default install dir.
fn install_dir_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        if let Some(d) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(d).join("ConfluenceMCP"));
        }
        if let Some(d) = std::env::var_os("USERPROFILE") {
            v.push(PathBuf::from(d).join("ConfluenceMCP"));
        }
    } else if let Some(d) = std::env::var_os("HOME") {
        v.push(PathBuf::from(d).join(".local").join("share").join("ConfluenceMCP"));
    }
    if v.is_empty() {
        v.push(PathBuf::from("ConfluenceMCP"));
    }
    v
}

/// Returns the first writable candidate path. If none are writable, returns the
/// preferred candidate anyway so the UI can display it and the user can Change…
pub fn default_install_dir() -> PathBuf {
    let candidates = install_dir_candidates();
    for c in &candidates {
        if probe_writable(c).is_ok() {
            return c.clone();
        }
    }
    candidates.into_iter().next().expect("at least one candidate")
}

pub fn resolve_install_dir(override_path: Option<String>) -> PathBuf {
    match override_path {
        Some(s) if !s.trim().is_empty() => PathBuf::from(s),
        _ => default_install_dir(),
    }
}

pub fn probe_writable(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let probe = dir.join(".probe");
    {
        let mut f = fs::File::create(&probe)?;
        f.write_all(b"ok")?;
    }
    fs::remove_file(&probe)
}

pub fn extract_server(dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let target = dir.join(SERVER_BINARY_NAME);
    fs::write(&target, EMBEDDED_SERVER)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target, perms)?;
    }
    Ok(target)
}
```

- [ ] **Step 3: Register module**

Modify `rust/crates/configurator/src/lib.rs`:

```rust
pub mod claude_config;
pub mod installer;
```

- [ ] **Step 4: Provide placeholder embedded server for compilation**

The `include_bytes!` macro reads `resources/confluence-mcp-server.exe` at compile time. Until Task 27 wires the real build pipeline, create a stub file so the crate compiles during development:

Run (PowerShell):

```powershell
Copy-Item rust/target/release/confluence-mcp-server.exe rust/crates/configurator/resources/confluence-mcp-server.exe -Force
```

(Requires the server binary from Task 17. If not yet built, run `cargo build --release -p server` first.)

- [ ] **Step 5: Run tests**

Run: `cd rust && cargo test -p configurator --test installer`

Expected: 4 tests pass.

- [ ] **Step 6: Commit (do NOT commit the resources/server.exe binary — it's gitignored)**

```bash
git add rust/crates/configurator/src/installer.rs rust/crates/configurator/src/lib.rs rust/crates/configurator/tests/installer.rs rust/crates/configurator/Cargo.toml
git commit -m "configurator: add installer with writability probe and extract"
```

---

## Task 21: `configurator` — Tauri commands for the wizard

**Files:**
- Create: `rust/crates/configurator/src/commands.rs`
- Modify: `rust/crates/configurator/src/lib.rs`
- Modify: `rust/crates/configurator/src/main.rs`

- [ ] **Step 1: Implement commands**

Write `rust/crates/configurator/src/commands.rs`:

```rust
use crate::claude_config::{default_config_path, read_config, remove_confluence_entry, write_confluence_entry, ConfluenceEntry};
use crate::installer::{extract_server, probe_writable, resolve_install_dir, default_install_dir};
use confluence_core::{Client, Config};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedConfig {
    pub config_exists: bool,
    pub confluence_configured: bool,
    pub url: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub ssl_verify: bool,
    pub install_dir: String,
}

#[tauri::command]
pub async fn load_existing_config() -> Result<LoadedConfig, String> {
    let path = default_config_path();
    let existing = read_config(&path).map_err(|e| e.to_string())?;
    let (url, username, password, token, ssl_verify, install_dir) = match &existing.confluence {
        Some(c) => {
            let dir = PathBuf::from(&c.command).parent().map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| default_install_dir().to_string_lossy().to_string());
            (
                c.url.clone(),
                c.username.clone().unwrap_or_default(),
                c.password.clone().unwrap_or_default(),
                c.token.clone().unwrap_or_default(),
                c.ssl_verify,
                dir,
            )
        }
        None => (
            String::new(), String::new(), String::new(), String::new(), true,
            default_install_dir().to_string_lossy().to_string(),
        ),
    };
    Ok(LoadedConfig {
        config_exists: existing.path_exists,
        confluence_configured: existing.confluence.is_some(),
        url, username, password, token, ssl_verify, install_dir,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionArgs {
    pub url: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub ssl_verify: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn test_connection(args: TestConnectionArgs) -> Result<TestConnectionResult, String> {
    if args.url.trim().is_empty() {
        return Ok(TestConnectionResult { success: false, message: "Please enter the Confluence URL.".into() });
    }
    if args.token.is_empty() && (args.username.is_empty() || args.password.is_empty()) {
        return Ok(TestConnectionResult {
            success: false,
            message: "Please enter either a Personal Access Token or both Username and Password.".into(),
        });
    }

    let cfg = Config {
        confluence_url: args.url.trim_end_matches('/').into(),
        username: (!args.username.is_empty()).then_some(args.username),
        password: (!args.password.is_empty()).then_some(args.password),
        token: (!args.token.is_empty()).then_some(args.token),
        ssl_verify: args.ssl_verify,
        ca_bundle: None,
        timeout: Duration::from_secs(15),
        rate_limit: 5,
        max_content_length: 50_000,
        default_search_limit: 10,
        log_level: "WARN".into(),
    };

    let client = match Client::new(cfg) {
        Ok(c) => c,
        Err(e) => return Ok(TestConnectionResult { success: false, message: e.to_string() }),
    };

    match client.list_spaces(Some("global"), 5, "").await {
        Ok(data) => {
            let count = data.pointer("/results").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            Ok(TestConnectionResult { success: true, message: format!("Connected! Found {count} space(s).") })
        }
        Err(e) => {
            let msg = match e.status_code() {
                401 => "Authentication failed. Please check your username/password or token.".into(),
                403 => "Permission denied. Your account may not have Confluence access.".into(),
                0 => format!("Cannot reach the server. Please check:\n- The URL is correct\n- You are connected to VPN (if required)\n- The server is running\n\nDetails: {e}"),
                code => format!("Error (HTTP {code}): {e}"),
            };
            Ok(TestConnectionResult { success: false, message: msg })
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigArgs {
    pub url: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub ssl_verify: bool,
    pub install_dir: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigResult {
    pub success: bool,
    pub message: String,
    pub config_path: String,
    pub server_path: String,
}

#[tauri::command]
pub async fn save_config(args: SaveConfigArgs) -> Result<SaveConfigResult, String> {
    let dir = resolve_install_dir(Some(args.install_dir.clone()));
    if let Err(e) = probe_writable(&dir) {
        return Ok(SaveConfigResult {
            success: false,
            message: format!("Cannot write to {}: {e}. Pick a different folder.", dir.display()),
            config_path: String::new(), server_path: String::new(),
        });
    }

    let server_path = match extract_server(&dir) {
        Ok(p) => p,
        Err(e) => return Ok(SaveConfigResult {
            success: false,
            message: format!("Failed to extract server binary: {e}. Your antivirus may be blocking this — add an exception or choose a different folder."),
            config_path: String::new(), server_path: String::new(),
        }),
    };

    let entry = ConfluenceEntry {
        command: server_path.to_string_lossy().replace('\\', "/"),
        url: args.url.clone(),
        username: (!args.username.is_empty()).then_some(args.username),
        password: (!args.password.is_empty()).then_some(args.password),
        token: (!args.token.is_empty()).then_some(args.token),
        ssl_verify: args.ssl_verify,
    };
    let config_path = default_config_path();
    if let Err(e) = write_confluence_entry(&config_path, &entry) {
        return Ok(SaveConfigResult {
            success: false,
            message: format!("Cannot write Claude Desktop config: {e}. Try running as Administrator."),
            config_path: config_path.to_string_lossy().into(),
            server_path: server_path.to_string_lossy().into(),
        });
    }

    Ok(SaveConfigResult {
        success: true,
        message: "Configuration saved! Restart Claude Desktop to activate.".into(),
        config_path: config_path.to_string_lossy().into(),
        server_path: server_path.to_string_lossy().into(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn remove_config(install_dir: String) -> Result<RemoveResult, String> {
    let dir = PathBuf::from(&install_dir);
    let config_path = default_config_path();
    if let Err(e) = remove_confluence_entry(&config_path) {
        return Ok(RemoveResult { success: false, message: format!("Failed to update config: {e}") });
    }
    let server_path = dir.join(crate::installer::SERVER_BINARY_NAME);
    let _ = std::fs::remove_file(&server_path);
    let _ = std::fs::remove_dir(&dir); // only removes if empty
    Ok(RemoveResult {
        success: true,
        message: "Confluence MCP removed. Restart Claude Desktop to apply.".into(),
    })
}
```

- [ ] **Step 2: Register commands in main.rs and lib.rs**

Modify `rust/crates/configurator/src/lib.rs`:

```rust
pub mod claude_config;
pub mod commands;
pub mod installer;
```

Modify `rust/crates/configurator/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use configurator::commands;

fn main() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::load_existing_config,
            commands::test_connection,
            commands::save_config,
            commands::remove_config,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Tauri app");
}
```

- [ ] **Step 3: Compile check**

Run: `cd rust && cargo build -p configurator`

Expected: build succeeds. If `Client::new` import errors arise, confirm `confluence-core` re-exports `Client` in `lib.rs` (should from Task 6).

- [ ] **Step 4: Commit**

```bash
git add rust/crates/configurator
git commit -m "configurator: wire Tauri commands for load/test/save/remove"
```

---

## Task 22: `configurator` UI — wizard frontend

**Files:**
- Modify: `rust/crates/configurator/ui/index.html`
- Modify: `rust/crates/configurator/ui/style.css`
- Modify: `rust/crates/configurator/ui/app.js`

Reworked wizard UI — single-page form with collapsing sections, inline validation, native folder-picker via Tauri dialog plugin.

- [ ] **Step 1: Replace `ui/index.html`**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Confluence MCP Setup</title>
    <link rel="stylesheet" href="style.css" />
  </head>
  <body>
    <main>
      <header>
        <h1>Confluence MCP Setup</h1>
        <p class="subtitle">Connect Claude Desktop to your Confluence instance.</p>
      </header>

      <form id="wizard" autocomplete="off">
        <fieldset>
          <legend>Server</legend>
          <label>Confluence URL
            <input type="url" id="url" placeholder="https://wiki.example.com" required />
          </label>
        </fieldset>

        <fieldset>
          <legend>Authentication</legend>
          <div class="tabs">
            <button type="button" class="tab active" data-target="auth-token">Token</button>
            <button type="button" class="tab" data-target="auth-basic">Username & Password</button>
          </div>
          <div id="auth-token" class="panel active">
            <label>Personal Access Token
              <input type="password" id="token" autocomplete="off" />
            </label>
          </div>
          <div id="auth-basic" class="panel">
            <label>Username <input type="text" id="username" autocomplete="off" /></label>
            <label>Password <input type="password" id="password" autocomplete="off" /></label>
          </div>
          <label class="checkbox">
            <input type="checkbox" id="ssl-verify" checked />
            Verify SSL certificate
          </label>
        </fieldset>

        <fieldset>
          <legend>Install location</legend>
          <div class="path-row">
            <input type="text" id="install-dir" readonly />
            <button type="button" id="pick-dir">Change…</button>
          </div>
          <p class="hint" id="path-hint"></p>
        </fieldset>

        <div class="actions">
          <button type="button" id="btn-test">Test Connection</button>
          <button type="button" id="btn-save" disabled>Save</button>
          <button type="button" id="btn-remove" class="link">Remove…</button>
        </div>

        <div id="status" class="status"></div>
      </form>
    </main>
    <script src="app.js"></script>
  </body>
</html>
```

- [ ] **Step 2: Replace `ui/style.css`**

```css
* { box-sizing: border-box; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  background: #fafafa; color: #222; margin: 0; padding: 24px;
}
main { max-width: 540px; margin: 0 auto; }
header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.subtitle { margin: 0 0 24px; color: #666; font-size: 13px; }

fieldset { border: 1px solid #e2e2e2; border-radius: 8px; padding: 14px 16px; margin: 0 0 16px; background: white; }
legend { font-weight: 600; font-size: 13px; padding: 0 6px; color: #444; }
label { display: block; margin: 8px 0; font-size: 13px; color: #333; }
label.checkbox { display: flex; align-items: center; gap: 6px; font-weight: normal; }
input[type="text"], input[type="password"], input[type="url"] {
  display: block; width: 100%; margin-top: 4px; padding: 8px 10px;
  border: 1px solid #d0d0d0; border-radius: 5px; font-size: 13px; background: #fff;
}
input[readonly] { background: #f3f3f3; color: #555; }

.tabs { display: flex; gap: 4px; margin-bottom: 12px; }
.tab { flex: 1; padding: 8px; background: #eee; border: none; border-radius: 5px; cursor: pointer; font-size: 12px; }
.tab.active { background: #2563eb; color: white; }
.panel { display: none; }
.panel.active { display: block; }

.path-row { display: flex; gap: 8px; align-items: center; }
.path-row input { flex: 1; }
.path-row button { padding: 8px 14px; border: 1px solid #d0d0d0; background: white; border-radius: 5px; cursor: pointer; font-size: 12px; }

.actions { display: flex; gap: 8px; margin-top: 16px; align-items: center; }
.actions button { padding: 10px 16px; border: none; border-radius: 6px; cursor: pointer; font-size: 13px; font-weight: 500; }
#btn-test { background: #f0f0f0; color: #222; }
#btn-save { background: #2563eb; color: white; }
#btn-save:disabled { background: #a0b7e6; cursor: not-allowed; }
.link { background: transparent !important; color: #b33 !important; padding: 10px 4px !important; margin-left: auto; font-weight: normal !important; text-decoration: underline; }

.status { margin-top: 14px; padding: 10px; border-radius: 6px; font-size: 13px; display: none; white-space: pre-wrap; }
.status.ok    { display: block; background: #ecfdf5; color: #065f46; border: 1px solid #a7f3d0; }
.status.err   { display: block; background: #fef2f2; color: #7f1d1d; border: 1px solid #fecaca; }
.status.info  { display: block; background: #eff6ff; color: #1e3a8a; border: 1px solid #bfdbfe; }

.hint { margin: 8px 0 0; font-size: 12px; color: #666; }
```

- [ ] **Step 3: Replace `ui/app.js`**

```js
const invoke = window.__TAURI__.core.invoke;
const openDialog = window.__TAURI__.dialog.open;

const $ = (id) => document.getElementById(id);
const statusEl = $("status");

function setStatus(kind, msg) {
  statusEl.className = "status " + kind;
  statusEl.textContent = msg;
}

// Tab switching
document.querySelectorAll(".tab").forEach(tab => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach(t => t.classList.remove("active"));
    document.querySelectorAll(".panel").forEach(p => p.classList.remove("active"));
    tab.classList.add("active");
    $(tab.dataset.target).classList.add("active");
  });
});

async function init() {
  try {
    const cfg = await invoke("load_existing_config");
    $("url").value = cfg.url;
    $("username").value = cfg.username;
    $("password").value = cfg.password;
    $("token").value = cfg.token;
    $("ssl-verify").checked = cfg.sslVerify;
    $("install-dir").value = cfg.installDir;
    if (cfg.token) {
      document.querySelector('[data-target="auth-token"]').click();
    } else if (cfg.username) {
      document.querySelector('[data-target="auth-basic"]').click();
    }
  } catch (e) {
    setStatus("err", "Failed to load existing config: " + e);
  }
}

$("pick-dir").addEventListener("click", async () => {
  const picked = await openDialog({ directory: true, defaultPath: $("install-dir").value });
  if (picked) $("install-dir").value = picked;
});

$("btn-test").addEventListener("click", async () => {
  setStatus("info", "Testing connection…");
  const result = await invoke("test_connection", {
    args: {
      url: $("url").value,
      username: $("username").value,
      password: $("password").value,
      token: $("token").value,
      sslVerify: $("ssl-verify").checked,
    }
  });
  setStatus(result.success ? "ok" : "err", result.message);
  $("btn-save").disabled = !result.success;
});

$("btn-save").addEventListener("click", async () => {
  setStatus("info", "Saving configuration…");
  const result = await invoke("save_config", {
    args: {
      url: $("url").value,
      username: $("username").value,
      password: $("password").value,
      token: $("token").value,
      sslVerify: $("ssl-verify").checked,
      installDir: $("install-dir").value,
    }
  });
  setStatus(result.success ? "ok" : "err", result.message);
});

$("btn-remove").addEventListener("click", async () => {
  if (!confirm("Remove Confluence MCP from Claude Desktop and delete the installed server binary?")) return;
  const result = await invoke("remove_config", { installDir: $("install-dir").value });
  setStatus(result.success ? "ok" : "err", result.message);
});

init();
```

- [ ] **Step 4: Build to confirm UI loads**

Run: `cd rust && cargo build -p configurator`

Expected: builds successfully. Run the debug binary (`cargo run -p configurator`) and verify the wizard window opens, pre-fills the default path, and tab switching works. Do **not** click Save yet — without a real Confluence server, Test Connection will fail, which is expected.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/configurator/ui
git commit -m "configurator: implement wizard UI with tabs and folder picker"
```

---

## Task 23: `configurator` — WebView2 presence check on Windows

**Files:**
- Modify: `rust/crates/configurator/src/main.rs`

Port of `_check_webview2` from `configurator/app.py:78-111`. Tauri's default behavior on missing WebView2 is a cryptic crash; a proactive check gives a clearer message.

- [ ] **Step 1: Add platform-specific check**

Modify `rust/crates/configurator/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use configurator::commands;

#[cfg(windows)]
fn check_webview2_or_warn() -> bool {
    use std::path::PathBuf;

    let candidates = [
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from).map(|p| p.join("Microsoft").join("EdgeWebView").join("Application")),
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from).map(|p| p.join("Microsoft").join("Edge").join("Application")),
        std::env::var_os("ProgramFiles").map(PathBuf::from).map(|p| p.join("Microsoft").join("EdgeWebView").join("Application")),
        std::env::var_os("ProgramFiles").map(PathBuf::from).map(|p| p.join("Microsoft").join("Edge").join("Application")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if let Ok(entries) = std::fs::read_dir(&candidate) {
            for entry in entries.flatten() {
                if entry.path().join("msedgewebview2.exe").is_file() {
                    return true;
                }
            }
        }
    }
    // Fallback: check registry
    use winreg::enums::*;
    use winreg::RegKey;
    for (hive, subkey) in [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}"),
        (HKEY_CURRENT_USER,  r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}"),
        (HKEY_CURRENT_USER,  r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}"),
    ] {
        if RegKey::predef(hive).open_subkey(subkey).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn show_missing_webview2_message() {
    use winapi::um::winuser::{MessageBoxW, MB_ICONWARNING};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let text: Vec<u16> = OsStr::new(
        "Microsoft WebView2 Runtime is required but not installed.\n\n\
         Please download it from:\n\
         https://developer.microsoft.com/en-us/microsoft-edge/webview2/\n\n\
         Install the 'Evergreen Bootstrapper' and try again."
    ).encode_wide().chain(Some(0)).collect();
    let caption: Vec<u16> = OsStr::new("Confluence MCP Setup — Missing Component")
        .encode_wide().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONWARNING);
    }
}

fn main() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();

    #[cfg(windows)]
    {
        if !check_webview2_or_warn() {
            show_missing_webview2_message();
            std::process::exit(1);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::load_existing_config,
            commands::test_connection,
            commands::save_config,
            commands::remove_config,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Tauri app");
}
```

- [ ] **Step 2: Add Windows-only deps**

Modify `rust/crates/configurator/Cargo.toml`:

```toml
[target.'cfg(windows)'.dependencies]
winreg = "0.52"
winapi = { version = "0.3", features = ["winuser"] }
```

- [ ] **Step 3: Build and verify**

Run: `cd rust && cargo build -p configurator`

Expected: builds on Windows. (Non-Windows builds compile but the check is a no-op.)

- [ ] **Step 4: Commit**

```bash
git add rust/crates/configurator
git commit -m "configurator: check for WebView2 runtime on Windows"
```

---

## Task 24: Build pipeline script — `scripts/build.ps1`

**Files:**
- Create: `scripts/build.ps1`
- Modify: `rust/.gitignore`

- [ ] **Step 1: Write the build script**

Write `scripts/build.ps1`:

```powershell
#Requires -Version 5.1
# End-to-end build pipeline for the Rust rewrite.
# Produces dist/ConfluenceMCPSetup.exe

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Rust = Join-Path $Root "rust"
$Dist = Join-Path $Root "dist"
$Upx  = Join-Path $Root "tools/upx-4.2.4-win64/upx.exe"

if (-not (Test-Path $Upx)) {
    Write-Error "UPX not found at $Upx. Run: (download the Python build.py UPX setup first)"
}

Write-Host "== 1/5 Building server crate (release) =="
Push-Location $Rust
cargo build --release -p server
Pop-Location

$ServerBin = Join-Path $Rust "target/release/confluence-mcp-server.exe"
if (-not (Test-Path $ServerBin)) { Write-Error "server binary missing at $ServerBin" }

Write-Host "== 2/5 UPX-compressing server binary =="
& $Upx --best $ServerBin

Write-Host "== 3/5 Copying server binary into configurator resources =="
$Resources = Join-Path $Rust "crates/configurator/resources"
New-Item -ItemType Directory -Force -Path $Resources | Out-Null
Copy-Item $ServerBin (Join-Path $Resources "confluence-mcp-server.exe") -Force

Write-Host "== 4/5 Building configurator crate (release) =="
Push-Location $Rust
cargo build --release -p configurator
Pop-Location

$WizardBin = Join-Path $Rust "target/release/ConfluenceMCPSetup.exe"
if (-not (Test-Path $WizardBin)) { Write-Error "wizard binary missing at $WizardBin" }

Write-Host "== 5/5 UPX-compressing wizard binary =="
& $Upx --best $WizardBin

New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Copy-Item $WizardBin (Join-Path $Dist "ConfluenceMCPSetup.exe") -Force

$finalSize = (Get-Item (Join-Path $Dist "ConfluenceMCPSetup.exe")).Length
Write-Host ""
Write-Host ("Final ConfluenceMCPSetup.exe: {0:N0} bytes ({1:N2} MB)" -f $finalSize, ($finalSize / 1MB))
Write-Host "Output: $Dist\ConfluenceMCPSetup.exe"
```

- [ ] **Step 2: Ensure `rust/.gitignore` covers the embedded binary**

Verify `rust/.gitignore` from Task 1 already contains:

```
crates/configurator/resources/confluence-mcp-server.exe
```

- [ ] **Step 3: Run the pipeline end-to-end**

Run (PowerShell): `powershell -ExecutionPolicy Bypass -File scripts/build.ps1`

Expected: full build succeeds. Final `dist/ConfluenceMCPSetup.exe` exists. Record its size.

- [ ] **Step 4: Size-check**

If `ConfluenceMCPSetup.exe` > 12 MB: the plan's CI gate will fail. Investigate dependency bloat (check `cargo tree -p configurator`). Common culprits: `reqwest` with native-tls enabled, unused Tauri features.

- [ ] **Step 5: Commit**

```bash
git add scripts/build.ps1
git commit -m "build: add end-to-end PowerShell build pipeline"
```

---

## Task 25: CI — GitHub Actions with size gates

**Files:**
- Create: `.github/workflows/rust-ci.yml`

- [ ] **Step 1: Write the workflow**

Write `.github/workflows/rust-ci.yml`:

```yaml
name: Rust CI

on:
  push:
    branches: [master, rust-port]
  pull_request:
    branches: [master]

jobs:
  build:
    runs-on: windows-latest
    defaults:
      run:
        shell: pwsh
        working-directory: rust
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo registry and build
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            rust/target
          key: ${{ runner.os }}-cargo-${{ hashFiles('rust/Cargo.lock') }}

      - name: Fetch UPX
        run: |
          Invoke-WebRequest -Uri "https://github.com/upx/upx/releases/download/v4.2.4/upx-4.2.4-win64.zip" -OutFile "$env:TEMP\upx.zip"
          Expand-Archive -Path "$env:TEMP\upx.zip" -DestinationPath "${{ github.workspace }}/tools" -Force
        working-directory: .

      - name: cargo fmt --check
        run: cargo fmt --all -- --check

      - name: cargo clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: cargo test
        run: cargo test --workspace -- --test-threads=1

      - name: Build distribution
        run: powershell -ExecutionPolicy Bypass -File scripts/build.ps1
        working-directory: ${{ github.workspace }}

      - name: Enforce size gates
        run: |
          $server = (Get-Item "rust/target/release/confluence-mcp-server.exe").Length
          $wizard = (Get-Item "dist/ConfluenceMCPSetup.exe").Length
          Write-Host ("server: {0:N0} bytes ({1:N2} MB)" -f $server, ($server / 1MB))
          Write-Host ("wizard: {0:N0} bytes ({1:N2} MB)" -f $wizard, ($wizard / 1MB))
          if ($server -gt 6MB) { Write-Error "server exceeds 6 MB ceiling"; exit 1 }
          if ($wizard -gt 12MB) { Write-Error "wizard exceeds 12 MB ceiling"; exit 1 }
        working-directory: ${{ github.workspace }}

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ConfluenceMCPSetup
          path: dist/ConfluenceMCPSetup.exe
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/rust-ci.yml
git commit -m "ci: add Rust build workflow with size gates"
```

- [ ] **Step 3: Push and verify CI runs**

```bash
git push -u origin rust-port
```

Expected: the `Rust CI` workflow runs on the push. Check the Actions tab — all steps should pass. Adjust size ceilings downward in the workflow if real numbers undershoot (e.g. if the wizard comes in at 7 MB, lower the ceiling to 9 MB to create a tighter regression guard).

---

## Task 26: Manual smoke test against real Confluence

**Files:** (no file changes)

- [ ] **Step 1: Build the final exe**

Run: `powershell -ExecutionPolicy Bypass -File scripts/build.ps1`

- [ ] **Step 2: Run the wizard**

Run: `./dist/ConfluenceMCPSetup.exe`

Expected flow:
1. Window opens titled "Confluence MCP Setup".
2. If you already had a Confluence entry from the old Python version, fields are pre-filled.
3. Install location shows `%LOCALAPPDATA%\ConfluenceMCP` by default.
4. Enter a real Confluence URL + Personal Access Token.
5. Click **Test Connection** — wait for "Connected! Found N space(s)." or a specific error.
6. If success, **Save** button enables. Click it.
7. "Configuration saved! Restart Claude Desktop to activate." appears.
8. Verify `%APPDATA%\Claude\claude_desktop_config.json` has a `mcpServers.confluence` entry pointing at `%LOCALAPPDATA%\ConfluenceMCP\confluence-mcp-server.exe`.

- [ ] **Step 3: Verify Claude Desktop picks up the server**

Quit Claude Desktop (right-click tray icon → Exit). Reopen it. In a chat, attempt:
> "List my Confluence spaces."

Claude should call the `list_spaces` tool and return the actual spaces from your server. Try:
> "Get Confluence page with ID <some-real-id>."

- [ ] **Step 4: Verify Remove action works**

Re-open the wizard. Click **Remove…**, confirm. The `mcpServers.confluence` entry disappears from `claude_desktop_config.json`; the extracted server binary and its folder are deleted.

- [ ] **Step 5: Commit nothing (this is a manual verification task)**

Write a short note in the next commit message documenting what was tested, or skip the commit and proceed to Task 27.

---

## Task 27: Cutover — delete Python code, promote `rust/` to repo root

**Files:**
- Delete: `server.py`, `main.py`, `config.py`, `confluence_client.py`, `build.py`
- Delete: `configurator/` directory (entire Python configurator)
- Delete: `tests/` directory (Python pytest suite)
- Delete: `requirements.txt`, `pyproject.toml`
- Delete: `build/` directory if present
- Move: contents of `rust/` to repo root
- Modify: `.gitignore`, `CLAUDE.md`, `README.md`

This is a single destructive commit; do it last and only after Task 26 passes.

- [ ] **Step 1: Verify `rust-port` branch is clean and all tests pass**

Run:
```bash
cd rust && cargo test --workspace -- --test-threads=1
cd .. && powershell -ExecutionPolicy Bypass -File scripts/build.ps1
```

Expected: all green. Do NOT proceed until this is true.

- [ ] **Step 2: Delete Python files**

```bash
git rm server.py main.py config.py confluence_client.py build.py requirements.txt pyproject.toml
git rm -r configurator tests
```

If `build/` or `dist/` subdirectories from the old Python build linger (and are not gitignored), delete them too:

```bash
rm -rf build
```

- [ ] **Step 3: Move `rust/` contents to repo root**

Because git treats move-and-delete carefully, use `git mv` for each tracked path:

```bash
git mv rust/Cargo.toml Cargo.toml
git mv rust/rust-toolchain.toml rust-toolchain.toml
git mv rust/crates crates
# Merge rust/.gitignore into root .gitignore
cat rust/.gitignore >> .gitignore
rm rust/.gitignore
rmdir rust
```

- [ ] **Step 4: Update `.gitignore`**

Edit `.gitignore` — remove the Python-specific sections (PyInstaller, Docker, Python dist/build artifacts) and keep:

```
# Secrets
.env
*.pem *.key *.crt

# Rust
target/
**/*.rs.bk
crates/configurator/resources/confluence-mcp-server.exe
dist/

# Build tools
tools/

# IDE / OS
.vscode/ .idea/ *.swp *.swo
.DS_Store Thumbs.db
```

- [ ] **Step 5: Update `CLAUDE.md`**

Replace the entire "Commands" and "Architecture" sections of `CLAUDE.md` with Rust-oriented content. Keep the "Distribution Constraint (CRITICAL)" section unchanged. New content:

```markdown
## Commands

```bash
# Run tests
cd rust && cargo test --workspace -- --test-threads=1

# Build release artifacts (Windows)
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
# Output: dist/ConfluenceMCPSetup.exe

# Run the wizard (debug)
cd rust && cargo run -p configurator

# Run the MCP server (debug, stdio; set env vars first)
cd rust && cargo run -p server
```

## Architecture

Cargo workspace under repo root with three crates:

- **`crates/confluence-core`** — shared library: HTTP client (`rmcp`-independent), `Config` from env vars, URL parser, HTML strip/truncate, error types.
- **`crates/server`** — MCP stdio server binary (`confluence-mcp-server.exe`) built on `rmcp`. Registers 7 Confluence tools (list_spaces, search_confluence, get_page, get_page_by_title, get_page_by_url, get_comments, get_attachments). Launched by Claude Desktop on every boot.
- **`crates/configurator`** — Tauri 2 desktop wizard (`ConfluenceMCPSetup.exe`) that embeds the server binary via `include_bytes!`, extracts it to the user's chosen install directory on Save, and writes the resulting path into `claude_desktop_config.json`.

`scripts/build.ps1` orchestrates the ordered build: server release → strip + UPX → copy into `crates/configurator/resources/` → configurator release → strip + UPX → final artifact in `dist/`.
```

Remove the Python "Testing" and "Environment Variables" subsections or rewrite them to point at the Rust equivalents. The environment-variable list for the MCP server is unchanged and can stay as-is.

- [ ] **Step 6: Update `README.md`**

Replace all Python install/run instructions with:

```markdown
## Install

1. Download `ConfluenceMCPSetup.exe` from the [latest release](../../releases/latest).
2. Double-click — the wizard opens.
3. Enter your Confluence URL and a Personal Access Token (or username/password).
4. Click **Test Connection**, then **Save**.
5. Restart Claude Desktop.

## Build from source

Requires Rust 1.75+ and PowerShell.

```bash
git clone <this repo>
cd confluence-mcp-server
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
# Output: dist/ConfluenceMCPSetup.exe
```
```

- [ ] **Step 7: Run the full test suite from the new root**

Run: `cargo test --workspace -- --test-threads=1` (from repo root, no `cd rust` anymore).

Expected: all tests pass.

- [ ] **Step 8: Run the build pipeline**

Run: `powershell -ExecutionPolicy Bypass -File scripts/build.ps1`

Expected: `dist/ConfluenceMCPSetup.exe` built successfully.

- [ ] **Step 9: Update CI workflow for the new layout**

Modify `.github/workflows/rust-ci.yml`:
- Remove `working-directory: rust` from the defaults and each step that had it.
- Update `actions/cache` key and paths to remove the `rust/` prefix.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "cutover: replace Python implementation with Rust rewrite"
```

- [ ] **Step 11: Push and open PR to master**

```bash
git push
gh pr create --title "Rust rewrite: ~27 MB → ~6-9 MB distribution" --body "$(cat <<'EOF'
## Summary

Replaces the Python + PyInstaller implementation with a Rust workspace:
- `crates/confluence-core` — shared HTTP client, URL parser, config.
- `crates/server` — MCP stdio server built on `rmcp`.
- `crates/configurator` — Tauri 2 wizard embedding the server binary.

Distribution drops from ~27 MB to ~6–9 MB for the Windows exe.

See `docs/superpowers/specs/2026-04-17-rust-rewrite-size-reduction-design.md` for the design and `docs/superpowers/plans/2026-04-17-rust-rewrite-size-reduction.md` for the execution plan.

## Test plan

- [ ] `cargo test --workspace -- --test-threads=1` passes
- [ ] `scripts/build.ps1` produces `dist/ConfluenceMCPSetup.exe`
- [ ] CI size-gates pass (server < 6 MB, wizard < 12 MB)
- [ ] Manual smoke test: wizard opens, Test Connection works, Save writes `claude_desktop_config.json`, Claude Desktop launches the new server and all 7 tools return expected results
EOF
)"
```

---
