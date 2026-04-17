# Confluence MCP Server

An MCP (Model Context Protocol) server that connects Claude Desktop to a self-hosted **Confluence Server / Data Center** instance, with a Tauri 2 desktop wizard for non-technical setup.

## Install

1. Download `ConfluenceMCPSetup.exe` from the [latest release](../../releases/latest) (~2.8 MB).
2. Double-click — the wizard opens.
3. Enter your Confluence URL and a Personal Access Token (or username/password).
4. Click **Test Connection**, then **Save**.
5. Restart Claude Desktop.

No Python, no terminal, no config files to edit. The exe bundles everything needed.

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
# Output: dist/ConfluenceMCPSetup.exe
```

---

## Authentication

### Personal Access Token (recommended for Data Center 7.9+)

```env
CONFLUENCE_TOKEN=your_pat_here
```

Generate at: **Profile → Settings → Personal Access Tokens**

PATs are preferred because they can be individually revoked, don't expose the user's main password, and can be set to expire.

### Basic Auth (username + password)

```env
CONFLUENCE_USERNAME=jsmith
CONFLUENCE_PASSWORD=secretpassword
```

Works with all Confluence Server versions.

> When `CONFLUENCE_TOKEN` is set, it takes priority over username/password.

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

For Confluence instances behind self-signed certificates:

```env
# Disable SSL verification (development only!)
CONFLUENCE_SSL_VERIFY=false

# OR provide a custom CA bundle (production)
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
