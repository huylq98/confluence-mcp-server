# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

An MCP (Model Context Protocol) server that connects Claude to **Confluence Server / Data Center**. Includes a PyInstaller-bundled desktop configurator GUI for non-technical users.

## Commands

```bash
# Install dependencies
pip install -r requirements.txt

# Or install as editable package
pip install -e ".[dev]"

# Run MCP server (HTTP mode, default)
python server.py

# Run MCP server (stdio mode for Claude Desktop)
MCP_TRANSPORT=stdio python server.py

# Run desktop configurator GUI
python main.py

# Run all tests
pytest

# Run a single test file
pytest tests/test_confluence_client.py

# Build desktop exe
pip install pyinstaller
python build.py
# Output: dist/ConfluenceMCPSetup.exe
```

## Architecture

**Entry points:**
- `server.py` — MCP server. Creates a `FastMCP` instance, registers 7 Confluence tools, and runs the transport (stdio or HTTP).
- `main.py` — Desktop entry point. No args = GUI wizard; `--serve` = MCP server.

**Configuration:** `config.py` loads env vars (with `.env` support via `python-dotenv`). Validates that Confluence URL and credentials are set. Exits on errors.

**API client:** `confluence_client.py` — Async HTTP client for the Confluence REST API. Handles auth (Bearer token or Basic), rate limiting via semaphore + minimum interval, retry with exponential backoff on 429/503, and error handling.

**Desktop configurator** (`configurator/`): pywebview-based GUI that writes Claude Desktop's `claude_desktop_config.json`. Has its own `requirements.txt`.

## Testing

Tests use `pytest` with `pytest-asyncio` (auto mode) and `respx` for mocking httpx requests. Test files are in `tests/`.

## Environment Variables

Confluence: `CONFLUENCE_URL`, `CONFLUENCE_TOKEN` or `CONFLUENCE_USERNAME`/`CONFLUENCE_PASSWORD`, `CONFLUENCE_SSL_VERIFY`, `CONFLUENCE_CA_BUNDLE`, `CONFLUENCE_TIMEOUT`, `CONFLUENCE_RATE_LIMIT`.

Server: `MCP_TRANSPORT` (`stdio`|`http`), `MCP_PORT` (default 8000).

Content: `MAX_CONTENT_LENGTH` (default 50000), `DEFAULT_SEARCH_LIMIT` (default 10).

## Important Notes

- Never log to stdout in stdio mode — all logging goes to stderr.
- Tool functions return formatted markdown strings, not raw JSON — they are designed for LLM consumption.
