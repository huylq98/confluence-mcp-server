"""
Atlassian Enterprise MCP Server
================================
A unified MCP (Model Context Protocol) server that connects Claude to Atlassian tools:
  - Confluence — search pages, read content, list spaces, comments, attachments
  - Jira        — search issues, create/update issues, manage transitions, comments
  - Bitbucket   — list repos/branches/commits, manage pull requests

Supports both Atlassian Cloud and self-hosted Data Center / Server deployments.

Usage:
  # stdio mode (Claude Desktop / Claude Code)
  python server.py

  # HTTP mode (remote deployment for Claude.ai teams)
  MCP_TRANSPORT=http python server.py
"""

import logging
import sys

from mcp.server.fastmcp import FastMCP

from config import load_config

# ── Bootstrap ───────────────────────────────────────────────────

config = load_config()

logging.basicConfig(
    level=getattr(logging, config.log_level, logging.INFO),
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    stream=sys.stderr,  # CRITICAL: never log to stdout in stdio mode
)
logger = logging.getLogger("atlassian-mcp")

mcp = FastMCP(
    "atlassian-server",
    instructions=(
        "Atlassian Enterprise integration. Tools available for:\n"
        "- Confluence: search wiki pages, read content, list spaces, fetch comments and attachments\n"
        "- Jira: search and manage issues, projects, transitions, and comments\n"
        "- Bitbucket: list repositories, branches, commits, and manage pull requests\n\n"
        "When a user pastes a Confluence URL, use confluence_get_page_by_url. "
        "When searching Jira, use JQL syntax with jira_search_issues. "
        "Use tool names prefixed with the service (confluence_*, jira_*, bitbucket_*)."
    ),
)

# ── Register service tools (conditional on configuration) ────────

if config.confluence.enabled:
    from atlassian.confluence.client import ConfluenceClient
    from atlassian.confluence.tools import register_tools as register_confluence

    confluence_client = ConfluenceClient(config.confluence, config.deployment_type)
    register_confluence(mcp, confluence_client, config)
    logger.info("Confluence tools registered (URL: %s)", config.confluence.url)
else:
    logger.info("Confluence not configured — skipping")

if config.jira.enabled:
    from atlassian.jira.client import JiraClient
    from atlassian.jira.tools import register_tools as register_jira

    jira_client = JiraClient(config.jira, config.deployment_type)
    register_jira(mcp, jira_client, config)
    logger.info("Jira tools registered (URL: %s)", config.jira.url)
else:
    logger.info("Jira not configured — skipping")

if config.bitbucket.enabled:
    from atlassian.bitbucket.client import BitbucketClient
    from atlassian.bitbucket.tools import register_tools as register_bitbucket

    bitbucket_client = BitbucketClient(config.bitbucket, config.deployment_type)
    register_bitbucket(mcp, bitbucket_client, config)
    logger.info("Bitbucket tools registered (URL: %s)", config.bitbucket.url)
else:
    logger.info("Bitbucket not configured — skipping")


# ── Entrypoint ──────────────────────────────────────────────────

def main():
    enabled = []
    if config.confluence.enabled:
        enabled.append("Confluence")
    if config.jira.enabled:
        enabled.append("Jira")
    if config.bitbucket.enabled:
        enabled.append("Bitbucket")

    logger.info("Starting Atlassian Enterprise MCP Server")
    logger.info("  Services:  %s", ", ".join(enabled) or "none")
    logger.info("  Deployment:%s", config.deployment_type)
    logger.info("  Transport: %s", config.transport)

    if config.transport == "http":
        logger.info("  Listening: %s:%s", config.host, config.port)
        mcp.run(transport="streamable-http", host=config.host, port=config.port)
    else:
        logger.info("  Running in stdio mode (Claude Desktop / Claude Code)")
        mcp.run(transport="stdio")


if __name__ == "__main__":
    main()
