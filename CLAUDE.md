# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

An MCP (Model Context Protocol) server that connects Claude to **Confluence Server / Data Center**. Includes a Tauri 2 desktop configurator wizard that writes the Claude Desktop config and extracts the MCP server binary to the user's chosen install directory.

## Distribution Constraint (CRITICAL)

The shipped distribution **must run standalone** on a tester/end-user machine that has **nothing installed** — no Python, no Rust, no Node, no runtimes, no dev tools. The single `ConfluenceConnect.exe` embeds everything it needs.

Implications:
- The MCP server binary is embedded inside the wizard via `include_bytes!` and extracted on Save.
- Once extracted, Claude Desktop launches the standalone server exe on every boot.
- Do not rely on PATH, system libs beyond what Windows ships with, or user installations.

## Commands

```bash
# Run tests (all crates)
cargo test --workspace -- --test-threads=1

# Build release distribution (Windows, PowerShell)
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
# From git-bash/MSYS where cargo is not on PATH, use the wrapper instead:
./scripts/package.sh
# Output: dist/ConfluenceConnect.exe (~7.6 MB unsigned, no UPX)

# Run the wizard in debug mode
cargo run -p configurator

# Run the MCP server in debug mode (set CONFLUENCE_URL + CONFLUENCE_TOKEN first)
cargo run -p server
```

## Architecture

Cargo workspace at repo root with three crates:

- **`crates/confluence-core`** — shared library: HTTP client with rate limiting + retry/backoff, `Config` loaded from env vars, URL parser (supports 8 formats), HTML strip/truncate helpers, error types.
- **`crates/server`** — MCP stdio server binary (`confluence-mcp-server.exe`) built on the official `rmcp` crate. Registers 7 Confluence tools: `list_spaces`, `search_confluence`, `get_page`, `get_page_by_title`, `get_page_by_url`, `get_comments`, `get_attachments`. Launched by Claude Desktop on every boot.
- **`crates/configurator`** — Tauri 2 desktop wizard (`ConfluenceConnect.exe`). Embeds the server binary via `include_bytes!`, extracts it to `%LOCALAPPDATA%\ConfluenceConnect\` (or user-chosen path) on Save, and writes the resulting path into Claude Desktop's `claude_desktop_config.json`.

`scripts/build.ps1` orchestrates the ordered build: server release → UPX → copy into configurator resources → configurator release → UPX → final artifact in `dist/`.

## Testing

Rust tests use `cargo test` with `wiremock` for HTTP mocking (parallel to Python's `respx`) and `tempfile` for filesystem isolation. Run with `--test-threads=1` because `confluence-core::config` tests mutate process env vars.

## Environment Variables

Confluence: `CONFLUENCE_URL`, `CONFLUENCE_TOKEN` or `CONFLUENCE_USERNAME`/`CONFLUENCE_PASSWORD`, `CONFLUENCE_SSL_VERIFY`, `CONFLUENCE_CA_BUNDLE`, `CONFLUENCE_TIMEOUT`, `CONFLUENCE_RATE_LIMIT`.

Content: `MAX_CONTENT_LENGTH` (default 50000), `DEFAULT_SEARCH_LIMIT` (default 10).

## Important Notes

- Never log to stdout in stdio mode — all logging goes to stderr (`tracing_subscriber::fmt().with_writer(std::io::stderr)`).
- Tool functions return formatted markdown strings, not raw JSON — designed for LLM consumption.
- Schemars version: pin via rmcp's bundled schemars 1.x (don't add a standalone dependency).
