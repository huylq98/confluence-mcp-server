# Enterprise MCP Platform — Design Document

## Vision

A platform that connects Claude to enterprise tools (Atlassian, GitHub, Slack, etc.) with
auth, access control, and audit logging built in. Teams self-serve connectors. IT gets visibility.

---

## Repositories (3 repos, start simple)

```
mcp-platform          # SDK + gateway config + control plane + dashboard
mcp-connectors        # All first-party connectors (this repo evolves into this)
mcp-connectors-*      # Community/third-party connectors (their own repos, published to PyPI)
```

Start with `mcp-connectors` (current repo). Build `mcp-platform` when the connector SDK stabilises.

---

## Architecture

```
Claude
  │  MCP/HTTP
  ▼
toolhive              # Auth (OIDC/API key), RBAC, audit, container isolation
  │  MCP/HTTP
  ▼
mcp-proxy             # Aggregates all connector servers into one endpoint
  │  MCP/HTTP (per connector)
  ▼
┌──────────┐ ┌────────┐ ┌───────┐ ┌───────────┐
│atlassian │ │ github │ │ slack │ │    ...    │   ← our FastMCP connectors
└──────────┘ └────────┘ └───────┘ └───────────┘
  │  REST APIs
  ▼
External services (Jira, Confluence, GitHub, Slack…)


Control Plane + Dashboard   # wraps toolhive config, connector registry, usage analytics
```

---

## Community Tools (don't build these)

| Tool | Purpose | Repo |
|---|---|---|
| `stacklok/toolhive` | Auth, RBAC, audit, container isolation | github.com/stacklok/toolhive |
| `tbxark/mcp-proxy` | Aggregates multiple MCP servers | github.com/tbxark/mcp-proxy |
| `jlowin/fastmcp` | Python MCP framework for connectors | github.com/jlowin/fastmcp |

---

## What We Build

### 1. Connector SDK (`packages/sdk`)

A small Python package that defines the contract every connector must implement.

```python
@dataclass
class ConnectorMetadata:
    id: str           # "atlassian", "github", "slack"
    name: str
    version: str      # semver
    category: str     # "devtools" | "communication" | "crm" | "itsm" | "cloud"
    description: str
    capabilities: list[str]   # ["read", "write"]

class ConnectorBase(ABC):
    metadata: ConnectorMetadata

    @abstractmethod
    def register_tools(self, mcp: FastMCP, config: dict) -> list[str]:
        """Register tools, return list of tool names."""

    @abstractmethod
    async def health_check(self) -> dict:
        """Return {"status": "ok"|"degraded"|"down", "message": str}"""
```

Every connector implements this. The gateway discovers connectors by Python entry point.

### 2. Connectors (`mcp-connectors` repo)

Each connector is an independently versioned Python package.

```
connectors/
├── atlassian/      # done — Confluence, Jira, Bitbucket
├── github/         # next
├── slack/
├── servicenow/
├── salesforce/
└── _template/      # scaffold for new connectors
```

**Priority order:** Atlassian → GitHub → Slack → ServiceNow → Microsoft Teams → Salesforce → AWS

Each connector:
- Implements `ConnectorBase`
- Runs as a standalone FastMCP HTTP server
- Has its own `pyproject.toml` and version
- Is published to PyPI (internal or public)
- Has unit tests

### 3. Control Plane + Dashboard (`mcp-platform` repo)

Simple web app for:
- **Connectors** — list installed connectors, health status, enable/disable
- **Teams** — create teams, add members
- **Access** — assign which teams can use which connectors
- **Audit log** — searchable table of all tool calls
- **API keys** — generate keys per team

Tech: FastAPI backend, simple frontend (HTMX or Next.js — decide later).

---

## RBAC Model (simple)

Three levels only to start:

| Role | Access |
|---|---|
| `viewer` | Read-only tools across all connectors |
| `contributor` | Read + write tools across all connectors |
| `admin` | Full access + manage team members |

Per-connector granularity comes later.

Config is a YAML file that toolhive reads:

```yaml
teams:
  - id: engineering
    members: [alice, bob]
    connectors: [atlassian, github]
    role: contributor

  - id: finance
    members: [carol]
    connectors: [atlassian]
    role: viewer
```

---

## Audit Log (simple)

Every tool call produces one record:

```json
{
  "timestamp": "2026-03-26T10:30:00Z",
  "user":      "alice@company.com",
  "team":      "engineering",
  "connector": "atlassian",
  "tool":      "jira_create_issue",
  "status":    "success",
  "duration_ms": 342
}
```

Stored in PostgreSQL. Exported via OpenTelemetry (toolhive handles this).

---

## Connector Structure (standard layout)

```
connectors/github/
├── pyproject.toml
├── github/
│   ├── __init__.py       # exports GithubConnector
│   ├── connector.py      # implements ConnectorBase
│   ├── client.py         # HTTP client (extends BaseHttpClient from SDK)
│   └── tools.py          # register_tools() implementation
├── tests/
│   ├── test_client.py
│   └── test_tools.py
└── README.md
```

---

## Deployment (simple first)

```yaml
# docker-compose.yml
services:
  toolhive:       # auth + RBAC + audit
  mcp-proxy:      # aggregator
  atlassian-mcp:  # connector
  github-mcp:     # connector
  control-plane:  # our API
  dashboard:      # our UI
  postgres:       # audit log + RBAC config
```

Kubernetes + Helm comes later.

---

## Phased Plan

### Phase 1 — Connector Foundation (now)
- [ ] Define `ConnectorBase` and `ConnectorMetadata` in SDK
- [ ] Refactor Atlassian connector to implement `ConnectorBase`
- [ ] Add `_template` connector scaffold
- [ ] Build GitHub connector (proves the SDK contract works)
- [ ] Each connector runs as standalone FastMCP HTTP server
- [ ] `docker-compose.yml` with `mcp-proxy` aggregating both

### Phase 2 — Gateway
- [ ] Integrate `toolhive` for auth (API keys first, OIDC later)
- [ ] RBAC via simple YAML config (teams → connectors → role)
- [ ] Audit log to PostgreSQL via toolhive OpenTelemetry
- [ ] Add Slack and ServiceNow connectors

### Phase 3 — Control Plane
- [ ] FastAPI control plane: teams, access, audit query
- [ ] Simple dashboard UI (connector health, audit log table, team management)
- [ ] API key management per team
- [ ] Connector registry (list available connectors, install/enable)

### Phase 4 — Marketplace
- [ ] Self-service connector onboarding
- [ ] Third-party connector publishing (PyPI + entry point)
- [ ] Usage analytics per team/connector
- [ ] OIDC/SSO integration

---

## What's Not In Scope (yet)

- Per-tool RBAC (connector-level is enough for now)
- Connector versioning UI
- Chargeback / billing per team
- Multi-region deployment
- Secrets management (Vault, AWS SM) — `.env` files first
