# Confluence Connect

[![SafeSkill 93/100](https://img.shields.io/badge/SafeSkill-93%2F100_Verified%20Safe-brightgreen)](https://safeskill.dev/scan/huylq98-confluence-mcp-server)
A small desktop app (plus a Model Context Protocol server) that connects Claude Desktop to a self-hosted **Confluence Server / Data Center** instance. Setup takes about two minutes; the wizard monitors the server and lets you stop or remove it at any time.

## Install

1. Download `ConfluenceConnect.exe` from the [latest release](https://github.com/huylq98/confluence-mcp-server/releases/latest).
2. Double-click — the wizard opens on the **Setup** tab.
3. Enter your Confluence URL and a Personal Access Token (or username/password).
4. Click **Test connection**, then **Save & finish**.
5. **Fully quit Claude Desktop** (tray icon → Exit) and reopen it. The MCP server is now wired in.

No Python, no terminal, no config files to edit. The exe bundles everything needed. If Windows SmartScreen warns about an unknown publisher, click **More info → Run anyway** (the build is unsigned).

## Monitor & uninstall

Re-run `ConfluenceConnect.exe` any time — the **Monitor** tab opens by default when you already have a configuration. It shows:

- Live **running / not running** status (the MCP server is a child process that Claude Desktop spawns on startup — if Claude Desktop isn't open, the server isn't running either).
- PID and memory usage, refreshed every 3 s.
- **Edit credentials** — jumps back to the Setup tab with your current values pre-filled.
- **Stop process** — kills the running instance (Claude Desktop will relaunch it).
- **Turn off & remove** — unregisters the MCP server from Claude Desktop and deletes the installed binary. Fully quit and relaunch Claude Desktop afterwards.

---

## Tools Provided

| Tool | Description |
|---|---|
| `list_spaces` | List all accessible Confluence spaces |
| `search_confluence` | CQL-powered full-text search across all spaces |
| `get_page` | Fetch full page content by numeric ID |
| `get_page_by_url` | Parse a Confluence URL and fetch the page |
| `get_page_by_title` | Find a page by exact title within a space |
| `get_comments` | Retrieve page comments (inline + footer) |
| `get_attachments` | List file attachments with download URLs |

---

## Build from Source

Requires Rust 1.75+ and PowerShell (Windows).

```bash
git clone <this repo>
cd confluence-mcp-server
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
# Output: dist/ConfluenceConnect.exe
```

---

## Authentication

The wizard writes your credentials into `claude_desktop_config.json` as environment variables on the MCP server entry — you don't edit anything by hand. This section documents what ends up in that file (useful for debugging or scripting).

### Personal Access Token (recommended for Data Center 7.9+)

```env
CONFLUENCE_TOKEN=your_pat_here
```

Generate at: **Profile → Settings → Personal Access Tokens**. PATs can be individually revoked, don't expose the user's main password, and can be set to expire.

### Basic Auth (username + password)

```env
CONFLUENCE_USERNAME=jsmith
CONFLUENCE_PASSWORD=secretpassword
```

Works with all Confluence Server versions. When `CONFLUENCE_TOKEN` is set, it takes priority over username/password.

---

## CQL Quick Reference

The `search_confluence` tool accepts CQL (Confluence Query Language) strings:

```sql
-- Full-text search
type=page AND text~"deployment guide"

-- Search within a space
type=page AND space=DEV AND text~"API docs"

-- By label
type=page AND label="architecture"

-- Recently modified
type=page AND lastmodified > now("-7d") ORDER BY lastmodified DESC

-- By author
creator=jsmith AND type=page

-- Fuzzy title search
title~"release notes" AND space IN (DEV, OPS)
```

---

## SSL / Self-Signed Certificates

The wizard has a **Verify SSL certificate** checkbox — uncheck it for self-signed certs. If you need to pin a specific CA bundle, edit `claude_desktop_config.json` and add:

```env
CONFLUENCE_CA_BUNDLE=/path/to/your-company-ca.crt
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `HTTP 401: Authentication failed` | Wrong credentials | Check `CONFLUENCE_USERNAME` / `CONFLUENCE_PASSWORD` or `CONFLUENCE_TOKEN` |
| `HTTP 403: Permission denied` | User lacks access | Ensure the account has read permissions on the target spaces |
| `HTTP 404: Not found` | Page doesn't exist or is restricted | Verify the page ID and user permissions |
| `Cannot connect` | Wrong URL or network issue | Check `CONFLUENCE_URL`, ensure the server is reachable |
| `SSL certificate verify failed` | Self-signed cert | Set `CONFLUENCE_SSL_VERIFY=false` or provide `CONFLUENCE_CA_BUNDLE` |

---

## License

[MIT](LICENSE)
