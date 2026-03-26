"""
Configuration loader for Confluence MCP Server.
Reads from .env file and validates required settings.
"""

import os
import sys
from dataclasses import dataclass
from pathlib import Path

from dotenv import load_dotenv


def _find_env_file() -> Path | None:
    """Search for .env file in current dir, parent dirs, and script dir."""
    # In frozen (PyInstaller) mode, env vars come from Claude Desktop — skip .env
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


@dataclass(frozen=True)
class Config:
    """Immutable server configuration."""

    # Confluence
    confluence_url: str
    username: str | None
    password: str | None
    token: str | None
    ssl_verify: bool | str  # bool or path to CA bundle
    timeout: int
    rate_limit: int

    # MCP Server
    transport: str
    host: str
    port: int

    # Content
    max_content_length: int
    default_search_limit: int

    # Logging
    log_level: str

    @property
    def auth_method(self) -> str:
        if self.token:
            return "token"
        if self.username and self.password:
            return "basic"
        return "none"

    def validate(self) -> list[str]:
        """Return list of validation errors (empty = valid)."""
        errors = []
        if not self.confluence_url:
            errors.append("CONFLUENCE_URL is required")
        if not self.token and not (self.username and self.password):
            errors.append(
                "Either CONFLUENCE_TOKEN or both CONFLUENCE_USERNAME "
                "and CONFLUENCE_PASSWORD must be set"
            )
        if self.transport not in ("stdio", "http"):
            errors.append(f"MCP_TRANSPORT must be 'stdio' or 'http', got '{self.transport}'")
        return errors


def load_config() -> Config:
    """Load configuration from environment / .env file."""
    env_path = _find_env_file()
    if env_path:
        load_dotenv(env_path, override=False)

    ssl_verify_raw = os.getenv("CONFLUENCE_SSL_VERIFY", "true").strip()
    ca_bundle = os.getenv("CONFLUENCE_CA_BUNDLE", "").strip()
    if ca_bundle:
        ssl_verify: bool | str = ca_bundle
    else:
        ssl_verify = ssl_verify_raw.lower() not in ("false", "0", "no")

    config = Config(
        confluence_url=os.getenv("CONFLUENCE_URL", "").rstrip("/"),
        username=os.getenv("CONFLUENCE_USERNAME"),
        password=os.getenv("CONFLUENCE_PASSWORD"),
        token=os.getenv("CONFLUENCE_TOKEN"),
        ssl_verify=ssl_verify,
        timeout=int(os.getenv("CONFLUENCE_TIMEOUT", "30")),
        rate_limit=int(os.getenv("CONFLUENCE_RATE_LIMIT", "5")),
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
