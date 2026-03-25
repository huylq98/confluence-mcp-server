# Confluence Server MCP

An [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) server that connects **Claude AI** to your self-hosted **Confluence Server / Data Center** instance.

## Features

| Tool | Description |
|---|---|
| `search_confluence` | CQL-powered full-text search across all spaces |
| `get_page` | Fetch full page content by numeric ID |
| `get_page_by_title` | Find a page by exact title within a space |
| `list_spaces` | List all accessible Confluence spaces |
| `get_comments` | Retrieve page comments (inline + footer) |
| `get_attachments` | List file attachments with download URLs |

## Prerequisites

- Python 3.11+
- A Confluence Server (self-hosted) or Data Center instance
- Confluence credentials: username/password **or** a Personal Access Token (PAT)

---

## Quick Start

### 1. Clone & configure

```bash
git clone <your-repo-url>
cd confluence-mcp-server

# Create your secret .env file from the template
cp .env.example .env
```

Edit `.env` with your Confluence details:

```env
CONFLUENCE_URL=https://confluence.yourcompany.com
CONFLUENCE_USERNAME=your_username
CONFLUENCE_PASSWORD=your_password
```

> **Security note:** The `.env` file is git-ignored and never committed. For production, use Docker secrets, Vault, or your cloud's secret manager instead.

### 2. Install dependencies

```bash
python -m venv .venv
source .venv/bin/activate     # Windows: .venv\Scripts\activate
pip install -r requirements.txt
```

### 3. Run

```bash
# Test locally with stdio (works with Claude Desktop)
MCP_TRANSPORT=stdio python server.py

# Run as HTTP server (for remote deployment / Claude.ai teams)
python server.py
# → Listening on http://0.0.0.0:8000
```

---

## Deployment Options

### Option A: Claude Desktop (local, per-user)

Each team member runs the server locally. Edit your Claude Desktop config:

**macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
**Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "confluence": {
      "command": "python",
      "args": ["/path/to/confluence-mcp-server/server.py"],
      "env": {
        "CONFLUENCE_URL": "https://confluence.yourcompany.com",
        "CONFLUENCE_USERNAME": "your_username",
        "CONFLUENCE_PASSWORD": "your_password",
        "MCP_TRANSPORT": "stdio"
      }
    }
  }
}
```

Or with `uv` (no venv needed):

```json
{
  "mcpServers": {
    "confluence": {
      "command": "uv",
      "args": ["--directory", "/path/to/confluence-mcp-server", "run", "server.py"],
      "env": {
        "CONFLUENCE_URL": "https://confluence.yourcompany.com",
        "CONFLUENCE_USERNAME": "your_username",
        "CONFLUENCE_PASSWORD": "your_password",
        "MCP_TRANSPORT": "stdio"
      }
    }
  }
}
```

### Option B: Claude Code

```bash
# Add via CLI (HTTP remote server)
claude mcp add --transport http confluence http://your-server:8000/mcp

# Or import from Claude Desktop config
claude mcp import-from-claude-desktop
```

### Option C: Remote server for Claude.ai teams (recommended)

Deploy once, share with your whole team via Claude.ai integrations.

#### Docker Compose (simplest)

```bash
cp .env.example .env
# Edit .env with your credentials
docker compose up -d
```

#### Docker (manual)

```bash
docker build -t confluence-mcp .

docker run -d \
  --name confluence-mcp \
  --restart unless-stopped \
  --env-file .env \
  -p 8000:8000 \
  confluence-mcp
```

#### Cloud deployment examples

**AWS (ECS/Fargate):** Use the Dockerfile, pass secrets via AWS Secrets Manager.

**Azure (Container Apps):** Deploy the container, use Azure Key Vault for credentials.

**GCP (Cloud Run):** Deploy the container, use Secret Manager for credentials.

**Cloudflare Workers:** Adapt the server to use Cloudflare's MCP hosting (provides OAuth out of the box).

#### Connect to Claude.ai

1. Go to [claude.ai/settings/connectors](https://claude.ai/settings/connectors)
2. Click **Add Integration**
3. Enter your server URL: `https://your-server.company.com/mcp`
4. On Team/Enterprise plans, the admin adds it once and it's available to all members

---

## Authentication

### Basic Auth (username + password)

```env
CONFLUENCE_USERNAME=jsmith
CONFLUENCE_PASSWORD=secretpassword
```

Works with all Confluence Server versions. The password is the user's actual login password.

### Personal Access Token (recommended for Data Center 7.9+)

```env
CONFLUENCE_TOKEN=your_pat_here
```

Generate at: **Profile → Settings → Personal Access Tokens**

PATs are preferred because they can be individually revoked, don't expose the user's main password, and can be set to expire.

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

## Security Best Practices

1. **Never commit `.env`** — it's git-ignored by default
2. **Use PATs over passwords** when your Confluence version supports them
3. **Use Docker secrets or Vault** in production instead of `.env` files:
   ```bash
   # Docker secrets example
   echo "your_password" | docker secret create confluence_password -
   ```
4. **Run behind a reverse proxy** (nginx/Caddy) with TLS when exposing to Claude.ai
5. **Restrict network access** — the MCP server only needs to reach your Confluence instance
6. **Use a service account** with read-only permissions rather than a personal account
7. **Set token expiry** on PATs so they auto-rotate

---

## Project Structure

```
confluence-mcp-server/
├── server.py              # MCP server with all 6 tools
├── confluence_client.py   # Async Confluence REST API client
├── config.py              # .env loader and validator
├── requirements.txt       # Python dependencies
├── .env.example           # Template — copy to .env
├── .gitignore             # Protects .env from commits
├── Dockerfile             # Container build (multi-stage)
├── docker-compose.yml     # One-command deployment
└── README.md              # This file
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `HTTP 401: Authentication failed` | Wrong credentials | Check `CONFLUENCE_USERNAME` / `CONFLUENCE_PASSWORD` or `CONFLUENCE_TOKEN` in `.env` |
| `HTTP 403: Permission denied` | User lacks access | Ensure the service account has read permissions on the target spaces |
| `HTTP 404: Not found` | Page doesn't exist or is restricted | Verify the page ID and user permissions |
| `Cannot connect` | Wrong URL or network issue | Check `CONFLUENCE_URL`, ensure the server is reachable from where the MCP server runs |
| `SSL certificate verify failed` | Self-signed cert | Set `CONFLUENCE_SSL_VERIFY=false` or provide `CONFLUENCE_CA_BUNDLE` |
| Responses are empty | Body not expanded | This is handled automatically — if you're customizing, ensure `expand=body.storage` is in the request |

---

## License

MIT
