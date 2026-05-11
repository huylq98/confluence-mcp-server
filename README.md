# Confluence Connect

[![SafeSkill 93/100](https://img.shields.io/badge/SafeSkill-93%2F100_Verified%20Safe-brightgreen)](https://safeskill.dev/scan/huylq98-confluence-mcp-server)

**Ask Claude about your Confluence pages.** A free Windows / macOS app that connects Claude Desktop to your company's **Confluence Server** or **Data Center** in about two minutes. No Python, no terminal, no config files to edit — the app bundles everything it needs.

[**→ Download & instructions (website)**](https://huylq98.github.io/confluence-mcp-server/) &nbsp;·&nbsp; [Latest release](https://github.com/huylq98/confluence-mcp-server/releases/latest) &nbsp;·&nbsp; [What you can ask Claude](#what-you-can-ask-claude)

---

## Install in two minutes

You don't need to be a developer. The app is a single file that sets everything up for you.

1. **Download** `ConfluenceConnect.exe` (Windows) or `ConfluenceConnect.pkg` (macOS) — get it from the [download page](https://huylq98.github.io/confluence-mcp-server/) or the [latest release](https://github.com/huylq98/confluence-mcp-server/releases/latest).
2. **Double-click** the file to open the setup wizard.
3. **Type your Confluence URL** and either a Personal Access Token *or* your username and password.
4. Click **Test connection**, then **Save & finish**.
5. **Fully quit Claude Desktop** (tray icon → Exit) and reopen it.

That's it. Claude Desktop can now read your Confluence.

> **If Windows warns about an "unknown publisher":** click **More info → Run anyway**. The app is open-source and free — we don't have a paid code-signing certificate.
> **If macOS blocks the installer:** open **System Settings → Privacy & Security** and click **Open Anyway**.

## What you can ask Claude

Once it's connected, try things like:

- *"Find the on-call rotation page in Confluence and tell me who's on this week."*
- *"Look up the deployment runbook for the payments service and summarize the rollback steps."*
- *"What does the page titled 'Release process' in the ENG space say?"*
- *"Are there any pages tagged `architecture` updated in the last 7 days?"*
- *"Read this page for me: <paste a Confluence URL>"*

Claude picks the right tool automatically — you just ask in plain English.

## Manage or uninstall later

Run `ConfluenceConnect.exe` again any time. The **Monitor** tab opens by default and shows:

- Whether Claude's Confluence connector is **running** or **not running** — and its memory usage, refreshed every 3 seconds.
- **Edit credentials** — change your URL or token without re-doing the wizard.
- **Stop process** — stop the connector (Claude Desktop will restart it on its next launch).
- **Turn off & remove** — fully uninstall and remove from Claude Desktop's config.

> The connector only runs while Claude Desktop is open. Claude Desktop starts it on launch and stops it on exit — you don't need to manage it yourself.

---

## Tools Claude gets

The connector adds seven tools to Claude Desktop:

| Tool | What it does |
|---|---|
| `list_spaces` | Lists every Confluence space you have access to |
| `search_confluence` | Full-text or CQL search across all spaces |
| `get_page` | Fetches a page by its numeric ID |
| `get_page_by_url` | Paste any Confluence URL — Claude reads the page |
| `get_page_by_title` | Finds a page by exact title within a space |
| `get_comments` | Reads inline and footer comments on a page |
| `get_attachments` | Lists file attachments with download URLs |

---

# For developers

Everything below is for people building, debugging, or customizing the connector. Regular users don't need any of this.

## Build from source

Requires Rust 1.75+ and PowerShell (Windows).

```bash
git clone git@github.com:huylq98/confluence-mcp-server.git
cd confluence-mcp-server
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
# Output: dist/ConfluenceConnect.exe (~7.8 MB unsigned)
```

Run tests: `cargo test --workspace -- --test-threads=1`.

The repo is a Cargo workspace with three crates:

- `crates/confluence-core` — shared HTTP client (rate limit + retry), config, URL parser, HTML helpers.
- `crates/server` — MCP stdio server (`confluence-mcp-server.exe`) built on the `rmcp` crate. Registers the seven tools above.
- `crates/configurator` — Tauri 2 desktop wizard (`ConfluenceConnect.exe`). Embeds the server binary via `include_bytes!`, extracts it to `%LOCALAPPDATA%\ConfluenceConnect\` on Save, and writes the path into `claude_desktop_config.json`.

## Authentication

The wizard writes credentials into `claude_desktop_config.json` as environment variables on the MCP server entry. You never edit by hand — this is documented for debugging or scripting only.

### Personal Access Token (recommended for Data Center 7.9+)

```env
CONFLUENCE_TOKEN=your_pat_here
```

Generate at **Profile → Settings → Personal Access Tokens**. PATs can be individually revoked, don't expose the user's main password, and can be set to expire.

### Basic Auth (username + password)

```env
CONFLUENCE_USERNAME=jsmith
CONFLUENCE_PASSWORD=secretpassword
```

Works with all Confluence Server versions. When `CONFLUENCE_TOKEN` is set, it takes priority over username/password.

## CQL Quick Reference

`search_confluence` accepts CQL (Confluence Query Language) strings:

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

## SSL / self-signed certificates

The wizard has a **Verify SSL certificate** checkbox — uncheck it for self-signed certs. To pin a specific CA bundle, edit `claude_desktop_config.json` and add:

```env
CONFLUENCE_CA_BUNDLE=/path/to/your-company-ca.crt
```

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
