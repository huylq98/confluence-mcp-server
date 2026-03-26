"""
Configuration loader for the Atlassian Enterprise MCP Server.
Supports Confluence, Jira, and Bitbucket with both Cloud and Data Center deployments.

Environment variable priority for each service:
  1. Service-specific vars  (CONFLUENCE_URL, JIRA_URL, BITBUCKET_URL)
  2. Global fallback vars   (ATLASSIAN_URL, ATLASSIAN_USERNAME, ATLASSIAN_TOKEN)
  3. Empty string           (service disabled)
"""

import os
import sys
from dataclasses import dataclass
from pathlib import Path

from dotenv import load_dotenv


def _find_env_file() -> Path | None:
    """Search for .env file in current dir, parent dirs, and script dir."""
    if getattr(sys, "frozen", False):
        return None
    candidates = [
        Path.cwd() / ".env",
        Path(__file__).parent / ".env",
        Path(__file__).parent.parent / ".env",
    ]
    for p in candidates:
        if p.is_file():
            return p
    return None


def _parse_bool(value: str) -> bool:
    return value.strip().lower() not in ("false", "0", "no")


def _ssl_verify(ssl_raw: str, ca_bundle: str) -> bool | str:
    if ca_bundle:
        return ca_bundle
    return _parse_bool(ssl_raw)


@dataclass(frozen=True)
class ServiceConfig:
    """Configuration for a single Atlassian service."""

    url: str
    token: str | None
    username: str | None
    password: str | None
    ssl_verify: bool | str
    timeout: int
    rate_limit: int
    enabled: bool

    @property
    def auth_method(self) -> str:
        if self.token:
            return "token"
        if self.username and self.password:
            return "basic"
        return "none"


@dataclass(frozen=True)
class AtlassianConfig:
    """Immutable top-level configuration for the Atlassian MCP Server."""

    deployment_type: str  # "cloud" or "datacenter"

    confluence: ServiceConfig
    jira: ServiceConfig
    bitbucket: ServiceConfig

    # MCP server
    transport: str
    host: str
    port: int

    # Content
    max_content_length: int
    default_search_limit: int

    # Logging
    log_level: str

    def validate(self) -> list[str]:
        """Return validation errors (empty = valid)."""
        errors = []

        if self.deployment_type not in ("cloud", "datacenter"):
            errors.append(
                f"ATLASSIAN_DEPLOYMENT must be 'cloud' or 'datacenter', "
                f"got '{self.deployment_type}'"
            )

        if self.transport not in ("stdio", "http"):
            errors.append(
                f"MCP_TRANSPORT must be 'stdio' or 'http', got '{self.transport}'"
            )

        # At least one service must be configured
        active = [s for s in (self.confluence, self.jira, self.bitbucket) if s.enabled]
        if not active:
            errors.append(
                "No services are configured. Set at least one of: "
                "CONFLUENCE_URL, JIRA_URL, BITBUCKET_URL (or ATLASSIAN_URL)"
            )

        # Validate each enabled service has credentials
        for name, svc in [("Confluence", self.confluence), ("Jira", self.jira), ("Bitbucket", self.bitbucket)]:
            if not svc.enabled:
                continue
            if not svc.token and not (svc.username and svc.password):
                errors.append(
                    f"{name}: Either *_TOKEN or both *_USERNAME and *_PASSWORD must be set"
                )

        return errors


# ---------------------------------------------------------------------------
# Backward-compatible alias — existing code that imports `Config` still works
# ---------------------------------------------------------------------------
Config = AtlassianConfig


def _build_service_config(
    *,
    url: str,
    token: str | None,
    username: str | None,
    password: str | None,
    ssl_raw: str,
    ca_bundle: str,
    timeout: int,
    rate_limit: int,
) -> ServiceConfig:
    return ServiceConfig(
        url=url.rstrip("/"),
        token=token or None,
        username=username or None,
        password=password or None,
        ssl_verify=_ssl_verify(ssl_raw, ca_bundle),
        timeout=timeout,
        rate_limit=rate_limit,
        enabled=bool(url),
    )


def load_config() -> AtlassianConfig:
    """Load and validate configuration from environment / .env file."""
    env_path = _find_env_file()
    if env_path:
        load_dotenv(env_path, override=False)

    # Global Atlassian fallbacks (useful for Cloud where one credential set covers all services)
    global_url = os.getenv("ATLASSIAN_URL", "").rstrip("/")
    global_token = os.getenv("ATLASSIAN_TOKEN", "")
    global_username = os.getenv("ATLASSIAN_USERNAME", "")
    global_password = os.getenv("ATLASSIAN_PASSWORD", "")
    global_ssl_raw = os.getenv("ATLASSIAN_SSL_VERIFY", "true")
    global_ca_bundle = os.getenv("ATLASSIAN_CA_BUNDLE", "").strip()
    global_timeout = int(os.getenv("ATLASSIAN_TIMEOUT", "30"))
    global_rate_limit = int(os.getenv("ATLASSIAN_RATE_LIMIT", "5"))

    deployment_type = os.getenv("ATLASSIAN_DEPLOYMENT", "datacenter").lower()

    # ── Confluence ──────────────────────────────────────────────────────────
    # Cloud Confluence uses /wiki prefix; datacenter does not
    raw_conf_url = os.getenv("CONFLUENCE_URL", global_url)
    confluence = _build_service_config(
        url=raw_conf_url,
        token=os.getenv("CONFLUENCE_TOKEN", global_token),
        username=os.getenv("CONFLUENCE_USERNAME", global_username),
        password=os.getenv("CONFLUENCE_PASSWORD", global_password),
        ssl_raw=os.getenv("CONFLUENCE_SSL_VERIFY", global_ssl_raw),
        ca_bundle=os.getenv("CONFLUENCE_CA_BUNDLE", global_ca_bundle),
        timeout=int(os.getenv("CONFLUENCE_TIMEOUT", str(global_timeout))),
        rate_limit=int(os.getenv("CONFLUENCE_RATE_LIMIT", str(global_rate_limit))),
    )

    # ── Jira ────────────────────────────────────────────────────────────────
    jira = _build_service_config(
        url=os.getenv("JIRA_URL", global_url),
        token=os.getenv("JIRA_TOKEN", global_token),
        username=os.getenv("JIRA_USERNAME", global_username),
        password=os.getenv("JIRA_PASSWORD", global_password),
        ssl_raw=os.getenv("JIRA_SSL_VERIFY", global_ssl_raw),
        ca_bundle=os.getenv("JIRA_CA_BUNDLE", global_ca_bundle),
        timeout=int(os.getenv("JIRA_TIMEOUT", str(global_timeout))),
        rate_limit=int(os.getenv("JIRA_RATE_LIMIT", str(global_rate_limit))),
    )

    # ── Bitbucket ───────────────────────────────────────────────────────────
    bitbucket = _build_service_config(
        url=os.getenv("BITBUCKET_URL", global_url),
        token=os.getenv("BITBUCKET_TOKEN", global_token),
        username=os.getenv("BITBUCKET_USERNAME", global_username),
        password=os.getenv("BITBUCKET_PASSWORD", global_password),
        ssl_raw=os.getenv("BITBUCKET_SSL_VERIFY", global_ssl_raw),
        ca_bundle=os.getenv("BITBUCKET_CA_BUNDLE", global_ca_bundle),
        timeout=int(os.getenv("BITBUCKET_TIMEOUT", str(global_timeout))),
        rate_limit=int(os.getenv("BITBUCKET_RATE_LIMIT", str(global_rate_limit))),
    )

    config = AtlassianConfig(
        deployment_type=deployment_type,
        confluence=confluence,
        jira=jira,
        bitbucket=bitbucket,
        transport=os.getenv("MCP_TRANSPORT", "http").lower(),
        host=os.getenv("MCP_HOST", "0.0.0.0"),
        port=int(os.getenv("MCP_PORT", "8000")),
        max_content_length=int(os.getenv("MAX_CONTENT_LENGTH", "50000")),
        default_search_limit=int(os.getenv("DEFAULT_SEARCH_LIMIT", "10")),
        log_level=os.getenv("LOG_LEVEL", "INFO").upper(),
    )

    errors = config.validate()
    if errors:
        print("Configuration errors:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        sys.exit(1)

    return config
