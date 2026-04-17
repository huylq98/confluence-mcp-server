# v0.2 — Wizard Clarity + Monitor Depth — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship v0.2 of Confluence Connect: Setup-tab clarity fixes (PAT deep-link, URL validation, post-save guidance, copy polish), a Monitor tab with today-snapshot + 7-day token-usage chart + recent-errors drawer + test-live button + rule-based analyzer sidebar with "Copy diagnostics for Claude".

**Architecture:** Introduce a tiny data plane — the MCP server appends `history.jsonl` + `errors.jsonl` to its install directory on every tool call; the configurator reads those files (no IPC). Add two new Rust modules in the configurator (`stats.rs` for bucketing, `analyzer.rs` for heuristic rules) behind new Tauri commands. UI changes are additive — existing `index.html` / `app.js` / `style.css` get extended, not rewritten.

**Tech Stack:** Rust 1.75+, Tauri 2, rmcp 0.8, tokio, vanilla HTML/CSS/JS, inline SVG for charts (no chart library), `wiremock` + `tempfile` for tests.

---

## Design Reference

This plan implements `docs/superpowers/specs/2026-04-17-v02-v04-roadmap-design.md` (v0.2 scope only). v0.3 (GitHub Pages landing + auto-updater) and v0.4 (polish) have their own plans created later.

## Data Plane — Schema (Implementation Contract)

**`history.jsonl`** (written by server, read by configurator):

```json
{"ts":1744928400,"tool":"get_page","args":{"page_id":"3965072"},"out_chars":18432,"tokens_est":4608,"status":"ok"}
```

**`errors.jsonl`** (written by server, read by configurator):

```json
{"ts":1744928412,"tool":"get_page","status":"403","message":"Page 3965072 restricted"}
```

Fields:
- `ts` — unix seconds (integer)
- `tool` — tool name, e.g., `"get_page"`, `"search_confluence"`
- `args` — minimal identifying arguments per tool (see table below). Empty `{}` for tools without a per-call key
- `out_chars` — character count of the formatted tool output returned to Claude
- `tokens_est` — `out_chars / 4` (rough approximation, no tokenizer)
- `status` — `"ok"` on success, HTTP status code as string on HTTP errors (`"403"`, `"500"`), or `"error"` for client-side failures
- `message` (errors file only) — first 300 chars of the error message

Per-tool `args` shape:

| Tool | `args` |
|---|---|
| `get_page` | `{"page_id": "..."}` |
| `get_page_by_url` | `{"url": "..."}` |
| `get_page_by_title` | `{"space_key": "...", "title": "..."}` |
| `get_comments` | `{"page_id": "..."}` |
| `get_attachments` | `{"page_id": "..."}` |
| `list_spaces` | `{}` |
| `search_confluence` | `{}` (CQL is user-sensitive; don't record it) |

Both files live in the same directory as `confluence-mcp-server.exe`. Server resolves it with `std::env::current_exe()?.parent()`. Configurator resolves it the same way it already resolves the install dir in `commands.rs::load_existing_config` (parent of `c.command`).

Truncation: server checks file line count on startup and once every 100 writes; when over limit, reads all, keeps last N, rewrites via temp-file + rename.

## Files

| Path | Action | Responsibility |
|---|---|---|
| `crates/server/src/recorder.rs` | **Create** | Append + truncate helpers for `history.jsonl` / `errors.jsonl`; owns all file I/O for the data plane |
| `crates/server/src/main.rs` | Modify | Add `mod recorder;` |
| `crates/server/src/handler.rs` | Modify | Hold `Arc<Recorder>`; each tool call records at end |
| `crates/configurator/src/stats.rs` | **Create** | Parse `history.jsonl`/`errors.jsonl`; bucket by day; Today summary |
| `crates/configurator/src/analyzer.rs` | **Create** | Pure-function heuristic rules over a parsed history slice |
| `crates/configurator/src/lib.rs` | Modify | Add `pub mod stats; pub mod analyzer;` |
| `crates/configurator/src/commands.rs` | Modify | Add Tauri commands: `get_stats`, `get_recommendations`, `test_live_connection`, `copy_diagnostics`, `open_claude_log`; extend error-chain formatter for proxy/CA hints |
| `crates/configurator/src/main.rs` | Modify | Register new Tauri commands in `invoke_handler!` |
| `crates/configurator/ui/index.html` | Modify | Copy fix; URL badge; PAT deep-link; post-save panel; Monitor widgets |
| `crates/configurator/ui/app.js` | Modify | Input handlers, chart renderer, analyzer rendering, Tauri calls |
| `crates/configurator/ui/style.css` | Modify | Styles for new widgets |
| `crates/configurator/Cargo.toml` | Modify | Add `arboard` (clipboard for Copy diagnostics) and `opener` (open Claude log folder) deps — Tauri has clipboard/shell plugins too; we'll use the Rust crates directly to keep plugin surface minimal |

---

## Task 1: Server-side `Recorder` module (foundation)

**Files:**
- Create: `crates/server/src/recorder.rs`
- Modify: `crates/server/Cargo.toml` (dev-dep `tempfile`)
- Test: inline `#[cfg(test)] mod tests` in `recorder.rs`

- [ ] **Step 1: Add `tempfile` dev-dependency to server**

Edit `crates/server/Cargo.toml`, add under `[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 2: Create `recorder.rs` with types and append logic (no truncation yet)**

Create `crates/server/src/recorder.rs`:

```rust
//! Append-only recorder for `history.jsonl` and `errors.jsonl`.
//!
//! Writes one JSON line per tool call to the server's install directory.
//! The configurator reads these files; there is no IPC — just a shared dir.

use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub const HISTORY_FILE: &str = "history.jsonl";
pub const ERRORS_FILE: &str = "errors.jsonl";
pub const MAX_HISTORY_LINES: usize = 1000;
pub const MAX_ERROR_LINES: usize = 20;
const TRUNCATE_EVERY_N_WRITES: u32 = 100;

#[derive(Debug, Serialize)]
pub struct HistoryEntry<'a> {
    pub ts: i64,
    pub tool: &'a str,
    pub args: serde_json::Value,
    pub out_chars: usize,
    pub tokens_est: usize,
    pub status: &'a str,
}

#[derive(Debug, Serialize)]
pub struct ErrorEntry<'a> {
    pub ts: i64,
    pub tool: &'a str,
    pub status: &'a str,
    pub message: &'a str,
}

pub struct Recorder {
    dir: PathBuf,
    write_counter: AtomicU32,
}

impl Recorder {
    /// Create a recorder rooted at `dir`. The directory must exist.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, write_counter: AtomicU32::new(0) }
    }

    /// Resolve install dir from the current executable's path.
    /// Returns `None` if the path cannot be determined.
    pub fn from_current_exe() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?.to_path_buf();
        Some(Self::new(dir))
    }

    pub fn record_history(&self, entry: &HistoryEntry<'_>) {
        let line = match serde_json::to_string(entry) {
            Ok(s) => s,
            Err(_) => return,
        };
        let path = self.dir.join(HISTORY_FILE);
        append_line(&path, &line);
        self.maybe_truncate(&path, MAX_HISTORY_LINES);
    }

    pub fn record_error(&self, entry: &ErrorEntry<'_>) {
        let line = match serde_json::to_string(entry) {
            Ok(s) => s,
            Err(_) => return,
        };
        let path = self.dir.join(ERRORS_FILE);
        append_line(&path, &line);
        self.maybe_truncate(&path, MAX_ERROR_LINES);
    }

    fn maybe_truncate(&self, path: &Path, max: usize) {
        let n = self.write_counter.fetch_add(1, Ordering::Relaxed);
        if n % TRUNCATE_EVERY_N_WRITES != 0 {
            return;
        }
        truncate_to_last_n_lines(path, max);
    }
}

fn append_line(path: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

fn truncate_to_last_n_lines(path: &Path, max: usize) {
    let Ok(file) = std::fs::File::open(path) else { return };
    let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
    if lines.len() <= max {
        return;
    }
    let keep = &lines[lines.len() - max..];
    let tmp = path.with_extension("jsonl.tmp");
    let Ok(mut f) = std::fs::File::create(&tmp) else { return };
    for l in keep {
        if writeln!(f, "{l}").is_err() {
            return;
        }
    }
    let _ = std::fs::rename(&tmp, path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ts() -> i64 { 1700000000 }

    #[test]
    fn append_creates_file_with_one_line() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recorder::new(dir.path().to_path_buf());
        rec.record_history(&HistoryEntry {
            ts: ts(), tool: "list_spaces", args: json!({}),
            out_chars: 42, tokens_est: 10, status: "ok",
        });
        let contents = std::fs::read_to_string(dir.path().join(HISTORY_FILE)).unwrap();
        assert_eq!(contents.lines().count(), 1);
        let parsed: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(parsed["tool"], "list_spaces");
        assert_eq!(parsed["out_chars"], 42);
    }

    #[test]
    fn multiple_appends_accumulate() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recorder::new(dir.path().to_path_buf());
        for _ in 0..5 {
            rec.record_history(&HistoryEntry {
                ts: ts(), tool: "get_page", args: json!({"page_id":"1"}),
                out_chars: 10, tokens_est: 2, status: "ok",
            });
        }
        let contents = std::fs::read_to_string(dir.path().join(HISTORY_FILE)).unwrap();
        assert_eq!(contents.lines().count(), 5);
    }

    #[test]
    fn errors_go_to_separate_file() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recorder::new(dir.path().to_path_buf());
        rec.record_error(&ErrorEntry {
            ts: ts(), tool: "get_page", status: "403", message: "denied",
        });
        assert!(!dir.path().join(HISTORY_FILE).exists());
        assert!(dir.path().join(ERRORS_FILE).exists());
    }
}
```

- [ ] **Step 3: Register the module**

Edit `crates/server/src/main.rs`, add alongside existing `mod` lines:

```rust
mod format;
mod handler;
mod recorder;
mod tools;
```

- [ ] **Step 4: Run tests and verify they pass**

Run: `cargo test -p server recorder -- --test-threads=1`

Expected: 3 tests pass (`append_creates_file_with_one_line`, `multiple_appends_accumulate`, `errors_go_to_separate_file`).

- [ ] **Step 5: Add truncation test and verify truncation works**

Add this test to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn truncate_keeps_last_n_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(HISTORY_FILE);
    // Seed with 15 lines.
    for i in 0..15 {
        append_line(&path, &format!(r#"{{"i":{i}}}"#));
    }
    truncate_to_last_n_lines(&path, 10);
    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 10);
    // Should have kept i=5..15.
    assert!(lines[0].contains(r#""i":5"#));
    assert!(lines[9].contains(r#""i":14"#));
}
```

Run: `cargo test -p server recorder::tests::truncate_keeps_last_n_lines -- --test-threads=1`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/recorder.rs crates/server/src/main.rs crates/server/Cargo.toml Cargo.lock
git commit -m "feat(server): add JSONL recorder with truncation

Appends per-call history to history.jsonl and per-error rows to
errors.jsonl in the install dir. Truncates to last 1000/20 lines
every 100 writes. Configurator reads these; no IPC."
```

---

## Task 2: Wire `Recorder` into every tool handler

**Files:**
- Modify: `crates/server/src/handler.rs`

- [ ] **Step 1: Import recorder types and hold an Arc**

At the top of `crates/server/src/handler.rs`, add:

```rust
use crate::recorder::{ErrorEntry, HistoryEntry, Recorder};
use std::time::{SystemTime, UNIX_EPOCH};
```

In the `ConfluenceServer` struct, add a new field:

```rust
#[derive(Clone)]
pub struct ConfluenceServer {
    pub(crate) client: Arc<Client>,
    pub(crate) config: Arc<Config>,
    recorder: Arc<Option<Recorder>>,
    tool_router: ToolRouter<ConfluenceServer>,
}
```

In `from_env`, initialize the recorder right before constructing `Self`:

```rust
let recorder = Arc::new(Recorder::from_current_exe());
Ok(Self {
    client: Arc::new(client),
    config: Arc::new(config),
    recorder,
    tool_router: Self::tool_router(),
})
```

- [ ] **Step 2: Add a small helper for recording**

Inside the `impl ConfluenceServer { ... }` block (near the top, after `from_env`/`confluence_url`), add:

```rust
fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn record(
    &self,
    tool: &'static str,
    args: serde_json::Value,
    text: &str,
    status: &str,
) {
    let Some(rec) = self.recorder.as_ref() else { return };
    rec.record_history(&HistoryEntry {
        ts: Self::now_ts(),
        tool,
        args,
        out_chars: text.chars().count(),
        tokens_est: text.chars().count() / 4,
        status,
    });
}

fn record_error(&self, tool: &'static str, status: &str, message: &str) {
    let Some(rec) = self.recorder.as_ref() else { return };
    let snippet: String = message.chars().take(300).collect();
    rec.record_error(&ErrorEntry {
        ts: Self::now_ts(),
        tool,
        status,
        message: &snippet,
    });
}
```

- [ ] **Step 3: Wrap `list_spaces`**

Replace the current body of `async fn list_spaces(...)` with:

```rust
async fn list_spaces(
    &self,
    Parameters(args): Parameters<ListSpacesArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let space_type = match args.space_type.as_deref() {
        Some("all") | None => None,
        Some(other) => Some(other),
    };
    let limit = args.limit.unwrap_or(50);
    let (text, status) = match self.client.list_spaces(space_type, limit, "description.plain").await {
        Ok(data) => (crate::tools::list_spaces::format(&data), "ok".to_string()),
        Err(e) => {
            let msg = crate::format::error_response(&e);
            let code = e.status_code();
            let status = if code > 0 { code.to_string() } else { "error".into() };
            self.record_error("list_spaces", &status, &msg);
            (msg, status)
        }
    };
    self.record("list_spaces", serde_json::json!({}), &text, &status);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}
```

- [ ] **Step 4: Wrap the other six tools the same way**

Replace each of the following with the same pattern — compute `(text, status)` first, then call `self.record(...)` once and return `CallToolResult::success`.

**`search_confluence`** — args recorded as `{}` (CQL is user-sensitive):

```rust
async fn search_confluence(
    &self,
    Parameters(args): Parameters<SearchArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let limit = args.limit.unwrap_or(10).clamp(1, 50);
    let (text, status) = match self.client.search(&args.cql, limit, "space,version,metadata.labels").await {
        Ok(data) => (crate::tools::search_confluence::format(&data, &self.config.confluence_url), "ok".to_string()),
        Err(e) => {
            let msg = crate::format::error_response(&e);
            let code = e.status_code();
            let status = if code > 0 { code.to_string() } else { "error".into() };
            self.record_error("search_confluence", &status, &msg);
            (msg, status)
        }
    };
    self.record("search_confluence", serde_json::json!({}), &text, &status);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}
```

**`get_page`** — args: `page_id`:

```rust
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
            crate::tools::get_page::format(&page, body_format, include_body, &self.config.confluence_url, self.config.max_content_length),
            "ok".to_string(),
        ),
        Err(e) => {
            let msg = crate::format::error_response(&e);
            let code = e.status_code();
            let status = if code > 0 { code.to_string() } else { "error".into() };
            self.record_error("get_page", &status, &msg);
            (msg, status)
        }
    };
    self.record("get_page", serde_json::json!({"page_id": page_id}), &text, &status);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}
```

**`get_page_by_title`** — args: `space_key` + `title`:

```rust
async fn get_page_by_title(
    &self,
    Parameters(args): Parameters<GetPageByTitleArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let expand = "body.storage,version,space,metadata.labels,ancestors";
    let args_json = serde_json::json!({"space_key": args.space_key, "title": args.title});
    let (text, status) = match self.client.get_page_by_title(&args.space_key, &args.title, expand).await {
        Ok(data) => {
            let empty = vec![];
            let results = data.pointer("/results").and_then(|v| v.as_array()).unwrap_or(&empty);
            let text = if results.is_empty() {
                crate::tools::get_page_by_title::format_not_found(&args.space_key, &args.title)
            } else {
                crate::tools::get_page_by_title::format_found(&results[0], &args.space_key, &self.config.confluence_url, self.config.max_content_length)
            };
            (text, "ok".to_string())
        }
        Err(e) => {
            let msg = crate::format::error_response(&e);
            let code = e.status_code();
            let status = if code > 0 { code.to_string() } else { "error".into() };
            self.record_error("get_page_by_title", &status, &msg);
            (msg, status)
        }
    };
    self.record("get_page_by_title", args_json, &text, &status);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}
```

**`get_page_by_url`** — args: `url` (not the parsed resolution — record what the user passed):

```rust
async fn get_page_by_url(
    &self,
    Parameters(args): Parameters<GetPageByUrlArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    use crate::tools::get_page_by_url::{resolve, UrlResolution, format_unparseable, format_tiny_url};

    let body_format = args.format.as_deref().unwrap_or("storage");
    let expand = format!("body.{body_format},version,space,metadata.labels,ancestors");
    let args_json = serde_json::json!({"url": args.url});

    let (text, status) = match resolve(&args.url) {
        UrlResolution::Unparseable => (format_unparseable(&args.url), "ok".to_string()),
        UrlResolution::TinyUrl(_)  => (format_tiny_url(), "ok".to_string()),
        UrlResolution::ById(id) => match self.client.get_page(&id, &expand).await {
            Ok(page) => (crate::tools::get_page::format(&page, body_format, true, &self.config.confluence_url, self.config.max_content_length), "ok".to_string()),
            Err(e) => {
                let msg = crate::format::error_response(&e);
                let code = e.status_code();
                let status = if code > 0 { code.to_string() } else { "error".into() };
                self.record_error("get_page_by_url", &status, &msg);
                (msg, status)
            }
        },
        UrlResolution::BySpaceTitle { space, title } => match self.client.get_page_by_title(&space, &title, &expand).await {
            Ok(data) => {
                let empty = vec![];
                let results = data.pointer("/results").and_then(|v| v.as_array()).unwrap_or(&empty);
                let text = if results.is_empty() {
                    format!(
                        "No page titled '{title}' found in space {space}.\nTip: Try search_confluence with: title~\"{title}\" AND space={space}"
                    )
                } else {
                    crate::tools::get_page::format(&results[0], body_format, true, &self.config.confluence_url, self.config.max_content_length)
                };
                (text, "ok".to_string())
            }
            Err(e) => {
                let msg = crate::format::error_response(&e);
                let code = e.status_code();
                let status = if code > 0 { code.to_string() } else { "error".into() };
                self.record_error("get_page_by_url", &status, &msg);
                (msg, status)
            }
        },
    };
    self.record("get_page_by_url", args_json, &text, &status);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}
```

**`get_comments`** — args: `page_id`:

```rust
async fn get_comments(
    &self,
    Parameters(args): Parameters<GetCommentsArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let limit = args.limit.unwrap_or(25);
    let page_id = args.page_id.clone();
    let (text, status) = match self.client.get_child(&page_id, "comment", "body.view,version,extensions.inlineProperties", limit).await {
        Ok(data) => (crate::tools::get_comments::format(&data), "ok".to_string()),
        Err(e) => {
            let msg = crate::format::error_response(&e);
            let code = e.status_code();
            let status = if code > 0 { code.to_string() } else { "error".into() };
            self.record_error("get_comments", &status, &msg);
            (msg, status)
        }
    };
    self.record("get_comments", serde_json::json!({"page_id": page_id}), &text, &status);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}
```

**`get_attachments`** — args: `page_id`:

```rust
async fn get_attachments(
    &self,
    Parameters(args): Parameters<GetAttachmentsArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let limit = args.limit.unwrap_or(50);
    let page_id = args.page_id.clone();
    let (text, status) = match self.client.get_child(&page_id, "attachment", "version", limit).await {
        Ok(data) => (crate::tools::get_attachments::format(&data, &self.config.confluence_url), "ok".to_string()),
        Err(e) => {
            let msg = crate::format::error_response(&e);
            let code = e.status_code();
            let status = if code > 0 { code.to_string() } else { "error".into() };
            self.record_error("get_attachments", &status, &msg);
            (msg, status)
        }
    };
    self.record("get_attachments", serde_json::json!({"page_id": page_id}), &text, &status);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}
```

- [ ] **Step 5: Build and smoke-test the server**

Run: `cargo build -p server`

Expected: build succeeds, no warnings about unused imports.

Run: `cargo test -p server -- --test-threads=1`

Expected: all existing tests + the 4 recorder tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/handler.rs
git commit -m "feat(server): record every tool call to history.jsonl

Each handler now computes (text, status), calls record() once, and
writes an extra errors.jsonl line on non-ok status. Args are recorded
per-tool with a minimal identifying shape; CQL and search queries are
deliberately omitted from args to avoid persisting sensitive terms."
```

---

## Task 3: Configurator `stats.rs` — read & bucket JSONL

**Files:**
- Create: `crates/configurator/src/stats.rs`
- Modify: `crates/configurator/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `stats.rs`

- [ ] **Step 1: Create `stats.rs` with types + parser**

Create `crates/configurator/src/stats.rs`:

```rust
//! Reads `history.jsonl` and `errors.jsonl` from the install dir and produces
//! UI-ready summaries. Pure functions — no Tauri, no side effects beyond file
//! reads.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const HISTORY_FILE: &str = "history.jsonl";
pub const ERRORS_FILE: &str = "errors.jsonl";

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryRow {
    pub ts: i64,
    pub tool: String,
    #[serde(default)]
    pub args: serde_json::Value,
    pub out_chars: usize,
    pub tokens_est: usize,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorRow {
    pub ts: i64,
    pub tool: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DayBucket {
    /// YYYY-MM-DD in the user's local timezone.
    pub date: String,
    pub calls: usize,
    pub tokens: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub today_calls: usize,
    pub today_tokens: usize,
    pub today_errors: usize,
    pub last_call_ts: Option<i64>,
    /// Seven entries, oldest first, covering the last 7 calendar days
    /// including today. Missing days get zero counts.
    pub seven_day_tokens: Vec<DayBucket>,
    pub recent_errors: Vec<ErrorRow>,
}

pub fn read_history(dir: &Path) -> Vec<HistoryRow> {
    read_jsonl(dir.join(HISTORY_FILE).as_path())
}

pub fn read_errors(dir: &Path) -> Vec<ErrorRow> {
    read_jsonl(dir.join(ERRORS_FILE).as_path())
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    let Ok(contents) = std::fs::read_to_string(path) else { return Vec::new() };
    contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<T>(l).ok())
        .collect()
}
```

- [ ] **Step 2: Add the summary function**

Append to `stats.rs`:

```rust
/// Build a 7-day UI summary. `now_ts` is passed in so tests are deterministic.
pub fn summarize(history: &[HistoryRow], errors: &[ErrorRow], now_ts: i64) -> StatsSummary {
    use chrono::{DateTime, Local, TimeZone, Utc};

    let now = Utc.timestamp_opt(now_ts, 0).single().unwrap_or_else(Utc::now);
    let today_local = now.with_timezone(&Local).date_naive();

    let mut today_calls = 0usize;
    let mut today_tokens = 0usize;
    let mut last_call_ts: Option<i64> = None;
    let mut daily: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for row in history {
        let ts = Utc.timestamp_opt(row.ts, 0).single().unwrap_or_else(Utc::now);
        let local_date = ts.with_timezone(&Local).date_naive();
        let key = local_date.format("%Y-%m-%d").to_string();
        let entry = daily.entry(key).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += row.tokens_est;
        if local_date == today_local {
            today_calls += 1;
            today_tokens += row.tokens_est;
        }
        last_call_ts = Some(last_call_ts.map_or(row.ts, |cur| cur.max(row.ts)));
    }

    let today_errors = errors.iter().filter(|e| {
        let ts = Utc.timestamp_opt(e.ts, 0).single().unwrap_or_else(Utc::now);
        ts.with_timezone(&Local).date_naive() == today_local
    }).count();

    let mut seven_day_tokens = Vec::with_capacity(7);
    for i in (0..7).rev() {
        let d = today_local - chrono::Duration::days(i);
        let key = d.format("%Y-%m-%d").to_string();
        let (calls, tokens) = daily.get(&key).copied().unwrap_or((0, 0));
        seven_day_tokens.push(DayBucket { date: key, calls, tokens });
    }

    let mut recent_errors = errors.to_vec();
    recent_errors.sort_by(|a, b| b.ts.cmp(&a.ts));
    recent_errors.truncate(20);

    StatsSummary {
        today_calls, today_tokens, today_errors,
        last_call_ts,
        seven_day_tokens,
        recent_errors,
    }
}
```

- [ ] **Step 3: Add `chrono` dependency**

Edit `crates/configurator/Cargo.toml`, add to `[dependencies]`:

```toml
chrono = { version = "0.4", default-features = false, features = ["clock", "serde"] }
```

- [ ] **Step 4: Register the module**

Edit `crates/configurator/src/lib.rs`:

```rust
pub mod claude_config;
pub mod commands;
pub mod debug_log;
pub mod installer;
pub mod stats;
```

- [ ] **Step 5: Add tests**

Append to `stats.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn row(ts: i64, tool: &str, tokens: usize, status: &str) -> HistoryRow {
        HistoryRow {
            ts, tool: tool.into(), args: serde_json::json!({}),
            out_chars: tokens * 4, tokens_est: tokens, status: status.into(),
        }
    }

    #[test]
    fn empty_inputs_produce_seven_zero_days() {
        let s = summarize(&[], &[], 1_700_000_000);
        assert_eq!(s.today_calls, 0);
        assert_eq!(s.today_tokens, 0);
        assert_eq!(s.today_errors, 0);
        assert_eq!(s.seven_day_tokens.len(), 7);
        assert!(s.seven_day_tokens.iter().all(|d| d.tokens == 0));
    }

    #[test]
    fn today_counters_aggregate_correctly() {
        // Two calls on the same day.
        let now = 1_700_000_000;
        let history = vec![
            row(now - 10, "get_page", 100, "ok"),
            row(now - 20, "get_page", 200, "ok"),
        ];
        let s = summarize(&history, &[], now);
        assert_eq!(s.today_calls, 2);
        assert_eq!(s.today_tokens, 300);
    }

    #[test]
    fn errors_counted_for_today_only() {
        let now = 1_700_000_000;
        let day = 86_400i64;
        let errors = vec![
            ErrorRow { ts: now, tool: "get_page".into(), status: "403".into(), message: "today".into() },
            ErrorRow { ts: now - 2 * day, tool: "get_page".into(), status: "403".into(), message: "two days ago".into() },
        ];
        let s = summarize(&[], &errors, now);
        assert_eq!(s.today_errors, 1);
    }

    #[test]
    fn recent_errors_sorted_newest_first_truncated_to_20() {
        let base = 1_700_000_000;
        let errors: Vec<ErrorRow> = (0..30).map(|i| ErrorRow {
            ts: base - i,
            tool: "get_page".into(),
            status: "403".into(),
            message: format!("err{i}"),
        }).collect();
        let s = summarize(&[], &errors, base);
        assert_eq!(s.recent_errors.len(), 20);
        assert_eq!(s.recent_errors[0].message, "err0");
        assert_eq!(s.recent_errors[19].message, "err19");
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(HISTORY_FILE);
        std::fs::write(
            &path,
            b"{\"ts\":1,\"tool\":\"x\",\"args\":{},\"out_chars\":1,\"tokens_est\":1,\"status\":\"ok\"}\nnot json\n{\"ts\":2,\"tool\":\"y\",\"args\":{},\"out_chars\":2,\"tokens_est\":2,\"status\":\"ok\"}\n",
        ).unwrap();
        let rows = read_history(tmp.path());
        assert_eq!(rows.len(), 2);
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p configurator stats -- --test-threads=1`

Expected: 5 tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/configurator/src/stats.rs crates/configurator/src/lib.rs crates/configurator/Cargo.toml Cargo.lock
git commit -m "feat(configurator): add stats module for history/errors JSONL

Pure-function parser + 7-day summarizer (chrono for timezone-aware
day bucketing). Handles empty/malformed input by skipping lines."
```

---

## Task 4: Configurator `analyzer.rs` — heuristic rules

**Files:**
- Create: `crates/configurator/src/analyzer.rs`
- Modify: `crates/configurator/src/lib.rs`
- Test: inline tests

- [ ] **Step 1: Create `analyzer.rs` with rule engine**

Create `crates/configurator/src/analyzer.rs`:

```rust
//! Heuristic rules over a parsed history slice. Pure functions; no I/O.

use crate::stats::HistoryRow;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tip {
    pub id: &'static str,
    pub title: String,
    pub detail: String,
}

const SEVEN_DAYS_SECS: i64 = 7 * 86_400;
const OVERSIZED_TOKENS: usize = 20_000;
const REPEAT_FETCH_THRESHOLD: usize = 5;
const FREQUENT_403_THRESHOLD: usize = 3;
const HIGH_ERROR_RATE: f64 = 0.10;

pub fn analyze(history: &[HistoryRow], now_ts: i64) -> Vec<Tip> {
    let cutoff = now_ts - SEVEN_DAYS_SECS;
    let recent: Vec<&HistoryRow> = history.iter().filter(|r| r.ts >= cutoff).collect();
    let mut tips = Vec::new();

    if let Some(t) = repeated_page_fetch(&recent) { tips.push(t); }
    if let Some(t) = oversized_output(&recent) { tips.push(t); }
    if let Some(t) = high_error_rate(&recent) { tips.push(t); }
    if let Some(t) = frequent_403s(&recent) { tips.push(t); }
    tips
}

fn repeated_page_fetch(rows: &[&HistoryRow]) -> Option<Tip> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in rows {
        if r.tool != "get_page" { continue; }
        if let Some(id) = r.args.get("page_id").and_then(|v| v.as_str()) {
            *counts.entry(id.to_string()).or_insert(0) += 1;
        }
    }
    let (page_id, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    if n < REPEAT_FETCH_THRESHOLD { return None; }
    Some(Tip {
        id: "repeated_page_fetch",
        title: format!("You've fetched page {page_id} {n} times this week."),
        detail: "Pin it in your Claude prompt or use `include_body=false` for previews.".into(),
    })
}

fn oversized_output(rows: &[&HistoryRow]) -> Option<Tip> {
    let n = rows.iter().filter(|r| r.tokens_est > OVERSIZED_TOKENS).count();
    if n == 0 { return None; }
    Some(Tip {
        id: "oversized_output",
        title: format!("{n} tool call(s) returned over 20k tokens."),
        detail: "Narrow your CQL (e.g. add `space=...`) or call `get_page` with `include_body=false` first.".into(),
    })
}

fn high_error_rate(rows: &[&HistoryRow]) -> Option<Tip> {
    if rows.is_empty() { return None; }
    let errors = rows.iter().filter(|r| r.status != "ok").count();
    let rate = errors as f64 / rows.len() as f64;
    if rate < HIGH_ERROR_RATE { return None; }
    Some(Tip {
        id: "high_error_rate",
        title: format!("{}% of calls failed this week.", (rate * 100.0).round() as u32),
        detail: "Check the Recent errors list below — the same endpoint may keep failing.".into(),
    })
}

fn frequent_403s(rows: &[&HistoryRow]) -> Option<Tip> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in rows {
        if r.status != "403" { continue; }
        if let Some(id) = r.args.get("page_id").and_then(|v| v.as_str()) {
            *counts.entry(id.to_string()).or_insert(0) += 1;
        }
    }
    let (page_id, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    if n < FREQUENT_403_THRESHOLD { return None; }
    Some(Tip {
        id: "frequent_403s",
        title: format!("Page {page_id} returned 403 {n} times."),
        detail: "That page is restricted — ask your Confluence admin for access or use a different page.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(ts: i64, tool: &str, status: &str, tokens: usize, args: serde_json::Value) -> HistoryRow {
        HistoryRow { ts, tool: tool.into(), args, out_chars: tokens * 4, tokens_est: tokens, status: status.into() }
    }

    #[test]
    fn no_tips_when_empty() {
        assert!(analyze(&[], 1_700_000_000).is_empty());
    }

    #[test]
    fn repeated_page_fetch_fires_at_threshold() {
        let now = 1_700_000_000;
        let rows: Vec<HistoryRow> = (0..5).map(|i| row(now - i, "get_page", "ok", 100, json!({"page_id":"42"}))).collect();
        let tips = analyze(&rows, now);
        assert!(tips.iter().any(|t| t.id == "repeated_page_fetch"));
    }

    #[test]
    fn repeated_page_fetch_does_not_fire_below_threshold() {
        let now = 1_700_000_000;
        let rows: Vec<HistoryRow> = (0..4).map(|i| row(now - i, "get_page", "ok", 100, json!({"page_id":"42"}))).collect();
        let tips = analyze(&rows, now);
        assert!(!tips.iter().any(|t| t.id == "repeated_page_fetch"));
    }

    #[test]
    fn oversized_output_fires_on_large_call() {
        let now = 1_700_000_000;
        let rows = vec![row(now, "search_confluence", "ok", 25_000, json!({}))];
        let tips = analyze(&rows, now);
        assert!(tips.iter().any(|t| t.id == "oversized_output"));
    }

    #[test]
    fn high_error_rate_fires_above_ten_percent() {
        let now = 1_700_000_000;
        // 2 errors out of 10 calls = 20%.
        let mut rows = Vec::new();
        for i in 0..8 { rows.push(row(now - i, "get_page", "ok", 50, json!({"page_id":"1"}))); }
        for i in 0..2 { rows.push(row(now - 20 - i, "get_page", "500", 50, json!({"page_id":"1"}))); }
        let tips = analyze(&rows, now);
        assert!(tips.iter().any(|t| t.id == "high_error_rate"));
    }

    #[test]
    fn frequent_403s_fires_on_same_page() {
        let now = 1_700_000_000;
        let rows: Vec<HistoryRow> = (0..3).map(|i| row(now - i, "get_page", "403", 50, json!({"page_id":"99"}))).collect();
        let tips = analyze(&rows, now);
        assert!(tips.iter().any(|t| t.id == "frequent_403s"));
    }

    #[test]
    fn old_entries_ignored() {
        let now = 1_700_000_000;
        let eight_days = 8 * 86_400;
        // Five fetches of the same page, but 8 days ago — should NOT fire.
        let rows: Vec<HistoryRow> = (0..5).map(|i| row(now - eight_days - i, "get_page", "ok", 100, json!({"page_id":"42"}))).collect();
        let tips = analyze(&rows, now);
        assert!(!tips.iter().any(|t| t.id == "repeated_page_fetch"));
    }
}
```

- [ ] **Step 2: Register the module**

Edit `crates/configurator/src/lib.rs`:

```rust
pub mod analyzer;
pub mod claude_config;
pub mod commands;
pub mod debug_log;
pub mod installer;
pub mod stats;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p configurator analyzer -- --test-threads=1`

Expected: 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/configurator/src/analyzer.rs crates/configurator/src/lib.rs
git commit -m "feat(configurator): add analyzer with 4 heuristic rules

Pure functions over parsed HistoryRow slices: repeated_page_fetch,
oversized_output, high_error_rate, frequent_403s. Each rule returns
Option<Tip>; none fire on empty input. Fully unit-tested."
```

---

## Task 5: New Tauri commands (bridge UI ↔ stats/analyzer)

**Files:**
- Modify: `crates/configurator/src/commands.rs`
- Modify: `crates/configurator/src/main.rs`
- Modify: `crates/configurator/Cargo.toml` (add `arboard`, `opener`)

- [ ] **Step 1: Add `arboard` + `opener` dependencies**

Edit `crates/configurator/Cargo.toml`, add to `[dependencies]`:

```toml
arboard = { version = "3", default-features = false }
opener = "0.7"
```

`arboard` is for clipboard access (Copy diagnostics). `opener` is for opening a folder in Explorer (Claude Desktop log shortcut).

- [ ] **Step 2: Add `get_stats` command**

At the bottom of `crates/configurator/src/commands.rs` (before the `#[cfg(test)]` block), add:

```rust
use crate::stats::{read_errors, read_history, summarize, StatsSummary};

fn install_dir_from_config() -> Option<PathBuf> {
    let existing = read_config(&default_config_path()).ok()?;
    let cmd = existing.confluence?.command;
    PathBuf::from(cmd).parent().map(|p| p.to_path_buf())
}

#[tauri::command]
pub async fn get_stats() -> Result<StatsSummary, String> {
    let dir = install_dir_from_config().ok_or_else(|| "Not configured yet.".to_string())?;
    let history = read_history(&dir);
    let errors = read_errors(&dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(summarize(&history, &errors, now))
}
```

- [ ] **Step 3: Add `get_recommendations` command**

Append to `commands.rs`:

```rust
use crate::analyzer::{analyze, Tip};

#[tauri::command]
pub async fn get_recommendations() -> Result<Vec<Tip>, String> {
    let dir = install_dir_from_config().ok_or_else(|| "Not configured yet.".to_string())?;
    let history = read_history(&dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(analyze(&history, now))
}
```

- [ ] **Step 4: Add `test_live_connection` command**

Append to `commands.rs`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTestResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: u64,
    pub space_count: usize,
}

#[tauri::command]
pub async fn test_live_connection() -> Result<LiveTestResult, String> {
    let existing = read_config(&default_config_path()).map_err(|e| e.to_string())?;
    let entry = existing.confluence.ok_or_else(|| "Not configured yet.".to_string())?;

    let cfg = Config {
        confluence_url: entry.url.trim_end_matches('/').into(),
        username: entry.username,
        password: entry.password,
        token: entry.token,
        ssl_verify: entry.ssl_verify,
        ca_bundle: None,
        proxy_url: entry.proxy_url,
        timeout: Duration::from_secs(10),
        rate_limit: 5,
        max_content_length: 50_000,
        default_search_limit: 10,
        log_level: "WARN".into(),
    };

    let client = Client::new(cfg).map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    match client.list_spaces(Some("global"), 5, "").await {
        Ok(data) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let count = data.pointer("/results").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            Ok(LiveTestResult {
                success: true,
                message: format!("OK · {count} space(s) · {latency_ms} ms"),
                latency_ms,
                space_count: count,
            })
        }
        Err(e) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            Ok(LiveTestResult {
                success: false,
                message: format_error_chain(&e),
                latency_ms,
                space_count: 0,
            })
        }
    }
}
```

- [ ] **Step 5: Add `copy_diagnostics` command**

Append to `commands.rs`:

```rust
#[tauri::command]
pub async fn copy_diagnostics() -> Result<String, String> {
    let dir = install_dir_from_config().ok_or_else(|| "Not configured yet.".to_string())?;
    let history = read_history(&dir);
    let errors = read_errors(&dir);

    let mut md = String::new();
    md.push_str("# Confluence Connect — diagnostics\n\n");
    md.push_str(&format!("## Recent tool calls ({} rows, newest last)\n\n", history.len().min(100)));
    md.push_str("| ts | tool | args | tokens | status |\n|---|---|---|---|---|\n");
    for r in history.iter().rev().take(100).rev() {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            r.ts, r.tool, r.args, r.tokens_est, r.status,
        ));
    }
    md.push_str("\n## Recent errors\n\n");
    for e in errors.iter().take(20) {
        md.push_str(&format!("- `{}` · **{}** · {}: {}\n", e.ts, e.tool, e.status, e.message));
    }
    md.push_str(
        "\n---\nPlease analyze usage patterns and suggest ways to reduce token usage, \
         avoid repeated fetches, or narrow my CQL queries. Any error patterns I should fix?\n",
    );

    arboard::Clipboard::new()
        .and_then(|mut cb| cb.set_text(md.clone()))
        .map_err(|e| format!("Clipboard error: {e}"))?;
    Ok(format!("Copied {} chars to clipboard — paste into Claude Desktop.", md.len()))
}
```

- [ ] **Step 6: Add `open_claude_log` command**

Append to `commands.rs`:

```rust
#[tauri::command]
pub async fn open_claude_log() -> Result<(), String> {
    // Windows: %APPDATA%\Claude\logs\
    let appdata = std::env::var_os("APPDATA")
        .ok_or_else(|| "APPDATA env var not set".to_string())?;
    let logs = PathBuf::from(appdata).join("Claude").join("logs");
    if !logs.exists() {
        return Err(format!("Log folder not found at {}", logs.display()));
    }
    opener::open(&logs).map_err(|e| format!("Failed to open folder: {e}"))
}
```

- [ ] **Step 7: Register the five new commands**

Edit `crates/configurator/src/main.rs`, extend the `invoke_handler!` list:

```rust
.invoke_handler(tauri::generate_handler![
    commands::load_existing_config,
    commands::test_connection,
    commands::save_config,
    commands::remove_config,
    commands::server_status,
    commands::stop_server,
    commands::get_stats,
    commands::get_recommendations,
    commands::test_live_connection,
    commands::copy_diagnostics,
    commands::open_claude_log,
    debug_log::get_debug_log,
    debug_log::clear_debug_log,
])
```

- [ ] **Step 8: Build and run the existing test suite**

Run: `cargo test -p configurator -- --test-threads=1`

Expected: all existing tests + 5 stats tests + 7 analyzer tests pass. No new tests here — the commands are thin glue; integration is covered by Task 9/10's manual smoke test.

- [ ] **Step 9: Commit**

```bash
git add crates/configurator/src/commands.rs crates/configurator/src/main.rs crates/configurator/Cargo.toml Cargo.lock
git commit -m "feat(configurator): add Tauri commands for stats/analyzer/live-test

get_stats, get_recommendations, test_live_connection, copy_diagnostics,
open_claude_log. Uses arboard for clipboard and opener for folder-open."
```

---

## Task 6: Wizard clarity — copy fix + URL validation badge + PAT deep-link

**Files:**
- Modify: `crates/configurator/ui/index.html`
- Modify: `crates/configurator/ui/app.js`
- Modify: `crates/configurator/ui/style.css`

UI tests are manual (consistent with the existing approach). The task still has verification steps that run the wizard.

- [ ] **Step 1: Fix the "wiki" copy in the hero**

Edit `crates/configurator/ui/index.html`, replace:

```html
<h1>
  Connect Claude Desktop<br />
  <em>to your wiki.</em>
</h1>
```

with:

```html
<h1>
  Connect Claude Desktop<br />
  <em>to Confluence.</em>
</h1>
```

- [ ] **Step 2: Add a URL validation badge and PAT deep-link next to the inputs**

In `index.html`, replace the `<fieldset class="group">` for "Server" with:

```html
<fieldset class="group">
  <legend>Server</legend>
  <label class="field">
    <span class="field-label">Confluence URL</span>
    <div class="input-wrap">
      <input
        type="url"
        id="url"
        placeholder="https://confluence.example.com"
        spellcheck="false"
        autocapitalize="off"
        required
      />
      <span class="url-badge" id="url-badge" data-state="empty"></span>
    </div>
  </label>
</fieldset>
```

And inside the `<div id="auth-token" class="seg-panel active">`, replace the `<label class="field">` with:

```html
<label class="field">
  <span class="field-label">
    Personal access token
    <a id="pat-link" class="field-link" href="#" target="_blank" aria-disabled="true">Get your token →</a>
  </span>
  <input type="password" id="token" autocomplete="off" spellcheck="false" />
  <span class="field-hint">
    Confluence → <kbd>Profile → Settings → Personal Access Tokens</kbd>
  </span>
</label>
```

- [ ] **Step 3: Add styles for the badge and link**

Append to `crates/configurator/ui/style.css`:

```css
.input-wrap { position: relative; }
.url-badge {
  position: absolute;
  right: 10px; top: 50%;
  transform: translateY(-50%);
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 10px;
  font-family: "IBM Plex Mono", monospace;
  pointer-events: none;
  opacity: 0;
  transition: opacity .2s;
}
.url-badge[data-state="ok"]    { opacity: 1; background: #dcfce7; color: #14532d; }
.url-badge[data-state="warn"]  { opacity: 1; background: #fef3c7; color: #78350f; }
.url-badge[data-state="bad"]   { opacity: 1; background: #fee2e2; color: #7f1d1d; }
.url-badge[data-state="empty"] { opacity: 0; }

.field-link {
  float: right;
  font-size: 11px;
  color: var(--accent, #555);
  text-decoration: underline;
  cursor: pointer;
}
.field-link[aria-disabled="true"] {
  opacity: 0.35;
  pointer-events: none;
}
```

- [ ] **Step 4: Wire up the URL badge and PAT link in `app.js`**

In `app.js`, find where the URL input is referenced (near the top event wiring). After the `switchView`/segment setup, add:

```javascript
/* ── URL validation badge + PAT deep-link ────────────────────────────── */
const urlInput = $("url");
const urlBadge = $("url-badge");
const patLink = $("pat-link");

function updateUrlBadge() {
  const v = urlInput.value.trim();
  if (!v) { urlBadge.dataset.state = "empty"; urlBadge.textContent = ""; return; }
  try {
    const u = new URL(v);
    if (u.protocol === "https:") { urlBadge.dataset.state = "ok";   urlBadge.textContent = "HTTPS ✓"; }
    else if (u.protocol === "http:") { urlBadge.dataset.state = "warn"; urlBadge.textContent = "HTTP ⚠"; }
    else { urlBadge.dataset.state = "bad"; urlBadge.textContent = "✗"; }
  } catch {
    urlBadge.dataset.state = "bad";
    urlBadge.textContent = "✗";
  }
}

function updatePatLink() {
  const v = urlInput.value.trim();
  try {
    const u = new URL(v);
    const base = `${u.protocol}//${u.host}`;
    patLink.href = `${base}/plugins/personalaccesstokens/usertokens.action`;
    patLink.setAttribute("aria-disabled", "false");
  } catch {
    patLink.href = "#";
    patLink.setAttribute("aria-disabled", "true");
  }
}

urlInput.addEventListener("input", () => { updateUrlBadge(); updatePatLink(); });
updateUrlBadge();
updatePatLink();
```

- [ ] **Step 5: Build and launch the wizard**

Run: `cargo run -p configurator`

Verify manually in the opened window:

1. Hero copy now reads "Connect Claude Desktop / to Confluence."
2. URL field: empty → no badge; type `http://x.com` → amber "HTTP ⚠"; type `https://x.com` → green "HTTPS ✓"; type `not a url` → red "✗".
3. "Get your token →" link next to the PAT field: grayed out with empty URL; once URL is valid, link becomes active and `patLink.href` ends in `/plugins/personalaccesstokens/usertokens.action`. Right-click → Copy link to verify the URL — or click it; a browser tab should open to that path.

Close the wizard.

- [ ] **Step 6: Commit**

```bash
git add crates/configurator/ui/index.html crates/configurator/ui/app.js crates/configurator/ui/style.css
git commit -m "feat(wizard): URL validation badge, PAT deep-link, copy fix

'wiki' -> 'Confluence' in the hero. Live HTTPS/HTTP/invalid badge on
the URL field. Contextual 'Get your token -> ' link that opens the
user's own Confluence PAT settings page, derived from the URL field."
```

---

## Task 7: Wizard clarity — post-save "You're set!" panel

**Files:**
- Modify: `crates/configurator/ui/index.html`
- Modify: `crates/configurator/ui/app.js`
- Modify: `crates/configurator/ui/style.css`

- [ ] **Step 1: Add the panel markup**

In `index.html`, add a new `<section>` right after the `</form>` of the Setup view (inside `<section id="view-setup">`):

```html
<section id="youre-set" class="youre-set" hidden>
  <h2 class="youre-set-title">You're set.</h2>
  <p class="youre-set-subtitle">Three steps left:</p>
  <ol class="youre-set-steps">
    <li>Fully quit Claude Desktop from the system tray icon.</li>
    <li>Reopen Claude Desktop.</li>
    <li>
      Try asking Claude:
      <ul class="youre-set-examples">
        <li><code>Find pages about onboarding</code></li>
        <li><code>Look up this page for me: &lt;paste a Confluence URL&gt;</code></li>
        <li><code>List the Confluence spaces I can access</code></li>
      </ul>
    </li>
  </ol>
  <button type="button" id="btn-to-monitor" class="btn btn-primary">
    <span class="btn-label">Continue to Monitor</span>
    <span class="btn-arrow">→</span>
  </button>
</section>
```

- [ ] **Step 2: Style the panel**

Append to `style.css`:

```css
.youre-set {
  padding: 24px 4px 12px;
  animation: fadein .3s ease;
}
.youre-set-title {
  font-family: "Fraunces", serif;
  font-size: 28px;
  margin: 0 0 6px;
}
.youre-set-subtitle { opacity: 0.7; margin: 0 0 14px; }
.youre-set-steps { line-height: 1.7; }
.youre-set-examples {
  margin-top: 6px;
  padding-left: 18px;
  font-family: "IBM Plex Mono", monospace;
  font-size: 12px;
  opacity: 0.85;
}
.youre-set-examples li { margin: 3px 0; }
@keyframes fadein { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: none; } }
```

- [ ] **Step 3: Show the panel after a successful Save**

In `app.js`, find the save handler (search for `save_config` or `btn-save`). After a successful save, instead of (or before) auto-switching to Monitor, reveal the panel:

```javascript
// After the existing "saved successfully" branch:
$("wizard").hidden = true;
$("youre-set").hidden = false;
$("btn-to-monitor").addEventListener("click", () => {
  $("youre-set").hidden = true;
  enableMonitorTab(true);
  switchView("monitor");
}, { once: true });
```

(If the existing code already calls `switchView("monitor")` on save, remove that call — the user should read the panel before advancing.)

- [ ] **Step 4: Run wizard and smoke-test**

Run: `cargo run -p configurator`

Manual check: fill in a valid URL + token, click Test → Save. The wizard form should hide; "You're set." panel appears with three numbered steps. Click **Continue to Monitor** — Monitor tab opens.

- [ ] **Step 5: Commit**

```bash
git add crates/configurator/ui/index.html crates/configurator/ui/app.js crates/configurator/ui/style.css
git commit -m "feat(wizard): post-save \"You're set!\" panel with example prompts

Replaces the silent jump-to-monitor with an explicit three-step
checklist and three copyable example prompts. User clicks Continue to
advance."
```

---

## Task 8: Smarter proxy/network error hints

**Files:**
- Modify: `crates/configurator/src/commands.rs`

- [ ] **Step 1: Extend `format_error_chain` with hint detection**

In `crates/configurator/src/commands.rs`, find `fn format_error_chain`. Replace it with:

```rust
fn format_error_chain(err: &ConfluenceError) -> String {
    let mut parts = vec![err.to_string()];
    let mut src: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(err);
    while let Some(e) = src {
        let msg = e.to_string();
        if !parts.last().is_some_and(|last| last.contains(&msg)) {
            parts.push(msg);
        }
        src = e.source();
    }
    let base = parts.join(" → ");
    let lower = base.to_lowercase();

    let mut hints: Vec<&str> = Vec::new();
    if lower.contains("timed out") || lower.contains("timeout") {
        hints.push("Connection timed out — check the URL and that you're on VPN if the server is internal.");
    }
    if lower.contains("invalid") && lower.contains("certificate") {
        hints.push("CA bundle invalid — re-check the path, and confirm the file is PEM-encoded.");
    }
    if lower.contains("dns error") || lower.contains("failed to lookup") {
        hints.push("DNS lookup failed — the hostname isn't resolving. Check spelling and DNS/VPN.");
    }
    if hints.is_empty() {
        base
    } else {
        format!("{base}\n\nHint:\n- {}", hints.join("\n- "))
    }
}
```

- [ ] **Step 2: Add a unit test**

At the bottom of `commands.rs` inside the existing `#[cfg(test)] mod tests` block, add:

```rust
#[test]
fn format_error_chain_adds_timeout_hint() {
    let err = ConfluenceError::Http { status: 0, message: "request timed out after 15s".into() };
    let out = format_error_chain(&err);
    assert!(out.contains("Connection timed out"), "got: {out}");
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p configurator commands::tests -- --test-threads=1`

Expected: 4 existing + 1 new = 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/configurator/src/commands.rs
git commit -m "feat(configurator): add hint lines to error chain output

Detects timeout, bad CA bundle, DNS lookup failures in the error
source chain and appends a short 'Hint: ...' line. Existing 403 +
private-IP hints keep their current treatment in test_connection."
```

---

## Task 9: Monitor — Today tile + token usage chart + recent errors drawer

**Files:**
- Modify: `crates/configurator/ui/index.html`
- Modify: `crates/configurator/ui/app.js`
- Modify: `crates/configurator/ui/style.css`

- [ ] **Step 1: Add Today tile + chart + drawer markup**

In `index.html`, inside `<section id="view-monitor">`, between the existing `<div class="monitor-card">` and `<div id="monitor-status">`, insert:

```html
<!-- Today tile -->
<div class="mini-card">
  <div class="mini-card-label">Today</div>
  <div class="mini-card-grid">
    <div><div class="mini-card-num" id="today-calls">0</div><div class="mini-card-sub">tool calls</div></div>
    <div><div class="mini-card-num" id="today-tokens">0</div><div class="mini-card-sub">tokens</div></div>
    <div><div class="mini-card-num error" id="today-errors">0</div><div class="mini-card-sub">errors</div></div>
  </div>
</div>

<!-- Token usage chart -->
<div class="mini-card">
  <div class="mini-card-head">
    <span class="mini-card-label">Token usage · last 7 days</span>
    <span class="mini-card-total" id="tokens-total">0</span>
  </div>
  <svg id="chart-tokens" viewBox="0 0 300 90" preserveAspectRatio="none"></svg>
  <div class="chart-axis" id="chart-axis"></div>
  <div class="mini-card-note">Confluence content returned to Claude, estimated from character count (chars ÷ 4).</div>
</div>

<!-- Recent errors drawer -->
<details class="mini-card" id="errors-drawer">
  <summary><span id="errors-summary">Recent errors (0)</span></summary>
  <ul id="errors-list" class="errors-list"></ul>
</details>
```

- [ ] **Step 2: Style the new blocks**

Append to `style.css`:

```css
.mini-card {
  background: rgba(30, 41, 59, 0.13);
  border: 1px solid rgba(51, 51, 68, 0.4);
  border-radius: 6px;
  padding: 10px 12px;
  margin-bottom: 10px;
}
.mini-card-head {
  display: flex; justify-content: space-between; align-items: baseline;
  margin-bottom: 8px;
}
.mini-card-label {
  font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; opacity: 0.55;
}
.mini-card-total { font-size: 11px; opacity: 0.55; }
.mini-card-grid {
  display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; font-size: 13px;
}
.mini-card-num { font-size: 18px; font-weight: 600; }
.mini-card-num.error { color: #ef4444; }
.mini-card-sub { opacity: 0.6; font-size: 11px; }
.mini-card-note { font-size: 10px; opacity: 0.5; margin-top: 8px; }

#chart-tokens { width: 100%; height: 90px; display: block; }
.chart-axis {
  display: flex; justify-content: space-between;
  font-size: 10px; opacity: 0.55; padding: 0 6px; margin-top: 4px;
}

.errors-list { list-style: none; padding: 0; margin: 8px 0 0; font-family: "IBM Plex Mono", monospace; font-size: 11px; }
.errors-list li { padding: 3px 0; opacity: 0.85; }
```

- [ ] **Step 3: Render function in `app.js`**

Append to `app.js`:

```javascript
/* ── Monitor: stats rendering ───────────────────────────────────────── */
async function refreshMonitorStats() {
  let s;
  try {
    s = await invoke("get_stats");
  } catch (_) { return; } // not configured yet, silent no-op
  $("today-calls").textContent   = s.todayCalls;
  $("today-tokens").textContent  = formatTokens(s.todayTokens);
  $("today-errors").textContent  = s.todayErrors;

  renderTokenChart(s.sevenDayTokens);

  const errList = $("errors-list");
  errList.innerHTML = "";
  for (const e of s.recentErrors) {
    const li = document.createElement("li");
    const date = new Date(e.ts * 1000);
    li.textContent = `${date.toLocaleTimeString([], {hour: '2-digit', minute:'2-digit'})} · ${e.tool} · ${e.status} · ${e.message}`;
    errList.appendChild(li);
  }
  $("errors-summary").textContent = `Recent errors (${s.recentErrors.length})`;
}

function formatTokens(n) {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return (n / 1000).toFixed(n < 10000 ? 1 : 0) + "k";
  return (n / 1_000_000).toFixed(1) + "M";
}

function renderTokenChart(days) {
  const svg = $("chart-tokens");
  const axis = $("chart-axis");
  while (svg.firstChild) svg.removeChild(svg.firstChild);
  axis.innerHTML = "";

  const max = Math.max(1, ...days.map(d => d.tokens));
  const total = days.reduce((a, d) => a + d.tokens, 0);
  $("tokens-total").textContent = formatTokens(total);

  const W = 300, H = 90, pad = 12;
  const slot = (W - 2 * pad) / days.length;
  const barW = Math.max(2, slot * 0.7);
  days.forEach((d, i) => {
    const h = (d.tokens / max) * (H - 20);
    const x = pad + i * slot + (slot - barW) / 2;
    const y = H - 12 - h;
    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", barW);
    rect.setAttribute("height", Math.max(1, h));
    rect.setAttribute("rx", 2);
    rect.setAttribute("fill", "#60a5fa");
    if (d.tokens === 0) rect.setAttribute("opacity", "0.2");
    svg.appendChild(rect);

    const dow = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const dayLabel = document.createElement("span");
    const dt = new Date(d.date + "T00:00:00");
    dayLabel.textContent = dow[dt.getDay()];
    axis.appendChild(dayLabel);
  });

  // baseline
  const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
  line.setAttribute("x1", 0); line.setAttribute("x2", W);
  line.setAttribute("y1", H - 12); line.setAttribute("y2", H - 12);
  line.setAttribute("stroke", "rgba(100,100,120,0.4)");
  line.setAttribute("stroke-width", "1");
  svg.appendChild(line);
}
```

- [ ] **Step 4: Call `refreshMonitorStats` on view entry**

In `app.js`, find the existing `startStatusPolling` (it's called from `switchView("monitor")`). Add a call to `refreshMonitorStats` right after it:

```javascript
if (name === "monitor") {
  startStatusPolling();
  refreshMonitorStats();
} else {
  stopStatusPolling();
}
```

And extend the polling tick (search for where `server_status` is invoked on an interval) to also call `refreshMonitorStats()` every N ticks — e.g., every other tick to avoid thrashing:

```javascript
// inside the setInterval tick handler:
if (tickCounter % 2 === 0) {
  refreshMonitorStats();
}
tickCounter += 1;
```

If there's no `tickCounter` variable in the file, declare it near the top: `let tickCounter = 0;`.

- [ ] **Step 5: Manual smoke**

Run: `cargo run -p configurator`

Save a valid config, then on the Monitor tab:

1. Today tile shows 0s initially (expected — no tool calls yet).
2. Chart shows empty baselines for each of the 7 days.
3. Recent errors drawer shows "Recent errors (0)".

Trigger a tool call via Claude Desktop (or manually by running `cargo run -p server` with env vars set and sending a stdio MCP request). Return to Monitor — Today tile increments; chart bar for today appears.

- [ ] **Step 6: Commit**

```bash
git add crates/configurator/ui/index.html crates/configurator/ui/app.js crates/configurator/ui/style.css
git commit -m "feat(monitor): Today tile, 7-day token chart, recent errors drawer

Reads get_stats on view entry and every other polling tick. Renders
tokens-per-day as an inline SVG column chart (no chart library).
Error list from recent_errors summary; max 20 items."
```

---

## Task 10: Monitor — Test live + analyzer sidebar + Copy diagnostics + Open log

**Files:**
- Modify: `crates/configurator/ui/index.html`
- Modify: `crates/configurator/ui/app.js`
- Modify: `crates/configurator/ui/style.css`

- [ ] **Step 1: Markup — Test live button, analyzer sidebar, Copy diagnostics, Open log link**

In `index.html`, modify the `<div class="monitor-card-head">` block to include a Test live button on the right:

```html
<div class="monitor-card-head">
  <span class="status-dot status-dot-lg" id="status-dot" data-state="unknown"></span>
  <div class="monitor-card-text">
    <span class="monitor-card-status" id="status-text">checking…</span>
    <span class="monitor-card-meta" id="status-meta"></span>
  </div>
  <button type="button" id="btn-test-live" class="btn btn-secondary btn-inline">
    <span class="btn-label">Test live</span>
    <span class="btn-arrow">→</span>
  </button>
</div>
```

Right after the recent-errors drawer (from Task 9), insert:

```html
<!-- Analyzer sidebar -->
<div class="mini-card" id="analyzer-card">
  <div class="mini-card-head">
    <span class="mini-card-label">For you this week</span>
    <button type="button" id="btn-copy-diag" class="ghost">Copy diagnostics for Claude</button>
  </div>
  <ul id="analyzer-list" class="analyzer-list"></ul>
  <div class="mini-card-note" id="analyzer-empty" hidden>
    Not enough data yet — tips appear after a few tool calls.
  </div>
</div>
```

And before the danger zone block, add the log shortcut:

```html
<a class="log-link" id="open-claude-log">Open Claude Desktop log →</a>
```

- [ ] **Step 2: Styles**

Append to `style.css`:

```css
.btn-inline {
  margin-left: auto;
  font-size: 11px;
  padding: 4px 10px;
}

.analyzer-list { list-style: none; padding: 0; margin: 8px 0 0; font-size: 12.5px; }
.analyzer-list li {
  padding: 8px 0;
  border-top: 1px solid rgba(51, 51, 68, 0.3);
}
.analyzer-list li:first-child { border-top: none; }
.analyzer-list .tip-title { font-weight: 500; }
.analyzer-list .tip-detail { opacity: 0.7; font-size: 11.5px; margin-top: 2px; }

.log-link {
  font-size: 11px;
  opacity: 0.6;
  cursor: pointer;
  text-decoration: underline;
  display: inline-block;
  margin: 8px 0;
}
```

- [ ] **Step 3: Wire up all four behaviors**

Append to `app.js`:

```javascript
/* ── Monitor: Test live ─────────────────────────────────────────────── */
$("btn-test-live").addEventListener("click", async () => {
  const btn = $("btn-test-live");
  btn.disabled = true;
  setStatus("info", "Testing live connection…");
  try {
    const r = await invoke("test_live_connection");
    if (r.success) setStatus("success", r.message);
    else setStatus("error", r.message);
  } catch (e) {
    setStatus("error", String(e));
  } finally {
    btn.disabled = false;
  }
});

/* ── Monitor: analyzer sidebar ──────────────────────────────────────── */
async function refreshAnalyzer() {
  let tips = [];
  try { tips = await invoke("get_recommendations"); } catch (_) { return; }
  const list = $("analyzer-list");
  const empty = $("analyzer-empty");
  list.innerHTML = "";
  if (tips.length === 0) {
    empty.hidden = false;
    return;
  }
  empty.hidden = true;
  for (const t of tips) {
    const li = document.createElement("li");
    const title = document.createElement("div"); title.className = "tip-title"; title.textContent = t.title;
    const detail = document.createElement("div"); detail.className = "tip-detail"; detail.textContent = t.detail;
    li.appendChild(title);
    li.appendChild(detail);
    list.appendChild(li);
  }
}

/* ── Monitor: Copy diagnostics ──────────────────────────────────────── */
$("btn-copy-diag").addEventListener("click", async () => {
  try {
    const msg = await invoke("copy_diagnostics");
    setStatus("success", msg);
  } catch (e) {
    setStatus("error", String(e));
  }
});

/* ── Monitor: Open Claude log ───────────────────────────────────────── */
$("open-claude-log").addEventListener("click", async (ev) => {
  ev.preventDefault();
  try { await invoke("open_claude_log"); } catch (e) { setStatus("error", String(e)); }
});
```

- [ ] **Step 4: Include analyzer in the Monitor refresh**

Update the monitor-entry block to also refresh the analyzer:

```javascript
if (name === "monitor") {
  startStatusPolling();
  refreshMonitorStats();
  refreshAnalyzer();
}
```

And inside the polling interval tick:

```javascript
if (tickCounter % 2 === 0) {
  refreshMonitorStats();
  refreshAnalyzer();
}
```

- [ ] **Step 5: Manual smoke**

Run: `cargo run -p configurator`

On the Monitor tab:

1. Click **Test live →** — status strip shows "OK · N space(s) · X ms" (or a clear error).
2. "For you this week" section shows "Not enough data yet …" initially. After triggering a few tool calls via Claude Desktop, it populates.
3. Click **Copy diagnostics for Claude** — clipboard contains a markdown document; paste it into Claude Desktop in a new chat and verify the prompt reads coherently.
4. Click **Open Claude Desktop log →** — Explorer opens to `%APPDATA%\Claude\logs\`.

- [ ] **Step 6: Final full-workspace test + size check**

Run: `cargo test --workspace -- --test-threads=1`

Expected: all tests pass (existing + recorder + stats + analyzer + commands).

Run: `powershell -ExecutionPolicy Bypass -File scripts/build.ps1`

Expected: `dist/ConfluenceConnect.exe` produced. Record the file size in the commit message below (target: stays under ~4 MB — we've added `chrono`, `arboard`, `opener`, `semver` is NOT added here; it's v0.3's).

- [ ] **Step 7: Commit**

```bash
git add crates/configurator/ui/index.html crates/configurator/ui/app.js crates/configurator/ui/style.css
git commit -m "feat(monitor): test-live button, analyzer sidebar, copy diagnostics, log shortcut

Completes v0.2. The Monitor tab now surfaces:
- One-shot Test live → check against the running config
- Rule-based tips pulled from get_recommendations
- Copy diagnostics → places a markdown prompt on the clipboard ready
  to paste into Claude Desktop
- Open Claude Desktop log → opens %APPDATA%\\Claude\\logs\\ in Explorer"
```

- [ ] **Step 8: Tag v0.2**

```bash
git tag v0.2.0 -m "v0.2.0 — wizard clarity + monitor depth"
```

Don't push the tag yet — that's for when the build+smoke results are recorded and you're ready to cut a release. For v0.2 there's no landing page / updater yet, so the release stays on GitHub's Releases page for the time being.

---

## Self-Review Results

**Spec coverage (design doc § v0.2 Design):**
- Wizard clarity: PAT deep-link (Task 6 ✓), URL validation badge (Task 6 ✓), post-save panel (Task 7 ✓), smarter error hints (Task 8 ✓), copy fix (Task 6 ✓)
- Monitor depth: Test live (Task 10 ✓), Today tile (Task 9 ✓), token column chart (Task 9 ✓), recent errors drawer (Task 9 ✓), rule-based analyzer sidebar (Task 10 ✓), Copy diagnostics (Task 10 ✓), Open Claude log (Task 10 ✓)
- Data plane: `history.jsonl` + `errors.jsonl` schema (spec table matches Task 1's `HistoryEntry`/`ErrorEntry` struct fields), truncation to 1000/20 (Task 1 ✓), written by server in `handler.rs` (Task 2 ✓), read by configurator in `stats.rs`/`analyzer.rs` (Tasks 3-4 ✓)
- Code organization: `stats.rs`, `analyzer.rs` (both new modules ✓); `commands.rs` extended thinly (Task 5 ✓); server gets `recorder.rs` (Task 1 ✓)
- Testing: recorder (Task 1), stats (Task 3), analyzer (Task 4), error-chain hint (Task 8). UI is manual by design — consistent with existing codebase practice.

**Placeholder scan:** no "TBD", "TODO", "implement later", "similar to". Every step has exact code or exact commands.

**Type consistency:** `HistoryEntry` in `recorder.rs` uses `tool: &'a str`, `out_chars: usize`, `tokens_est: usize`, `status: &'a str`. `HistoryRow` in `stats.rs` uses owned `String`s + `usize` + `String`. Serde round-trips because field names match (`ts`, `tool`, `args`, `out_chars`, `tokens_est`, `status`). `Tip` struct identical in `analyzer.rs` definition and `commands.rs` return type. `StatsSummary` is `#[serde(rename_all = "camelCase")]` so the JS side sees `todayCalls` / `sevenDayTokens` — matching the JS code in Task 9.

**Scope check:** Ten tasks, each self-contained, each ending in a commit. Independent enough for subagent-driven execution with review between tasks.

One real assumption worth flagging to the executor: Task 2's handler rewrite replaces all seven existing tool handler bodies. Verify that running `cargo test -p server -- --test-threads=1` after Step 5 passes before committing; if any test names reference details of the old handler shape (unlikely — they test `tools::*::format` directly), update them inline.
