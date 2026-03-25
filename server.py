"""
Confluence Server MCP Server
=============================
An MCP (Model Context Protocol) server that connects Claude to your
self-hosted Confluence Server / Data Center instance.

Tools provided:
  - search_confluence   — CQL-powered search across all spaces
  - get_page            — Fetch full page content by ID
  - get_page_by_url     — Fetch page content by pasting any Confluence URL
  - get_page_by_title   — Find a page by exact title in a space
  - list_spaces         — List all accessible Confluence spaces
  - get_comments        — Get comments on a page
  - get_attachments     — List file attachments on a page

Usage:
  # stdio mode (Claude Desktop / Claude Code)
  python server.py

  # HTTP mode (remote deployment for Claude.ai teams)
  MCP_TRANSPORT=http python server.py
"""

import logging
import re
import sys
from typing import Literal
from urllib.parse import urlparse, parse_qs, unquote

from mcp.server.fastmcp import FastMCP

from config import Config, load_config
from confluence_client import ConfluenceClient, ConfluenceError

# ── Bootstrap ───────────────────────────────────────────────────

config: Config = load_config()

logging.basicConfig(
    level=getattr(logging, config.log_level, logging.INFO),
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    stream=sys.stderr,  # CRITICAL: never log to stdout in stdio mode
)
logger = logging.getLogger("confluence-mcp")

mcp = FastMCP(
    "confluence-server",
    instructions=(
        "Confluence Server integration. Use these tools to search wiki pages, "
        "read page content, list spaces, and fetch comments/attachments from "
        "a self-hosted Confluence instance. "
        "When a user pastes a Confluence URL, always use get_page_by_url to "
        "fetch the page content directly from the link."
    ),
)

client = ConfluenceClient(config)


# ── Helpers ─────────────────────────────────────────────────────

def _strip_html(html: str) -> str:
    """Lightweight HTML tag stripper for readable output."""
    text = re.sub(r"<br\s*/?>", "\n", html)
    text = re.sub(r"</(p|div|h[1-6]|li|tr)>", "\n", text)
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"&nbsp;", " ", text)
    text = re.sub(r"&amp;", "&", text)
    text = re.sub(r"&lt;", "<", text)
    text = re.sub(r"&gt;", ">", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


def _truncate(text: str, max_len: int | None = None) -> str:
    limit = max_len or config.max_content_length
    if len(text) <= limit:
        return text
    return text[:limit] + f"\n\n... [truncated — {len(text)} chars total]"


def _format_labels(page: dict) -> str:
    labels = (
        page.get("metadata", {})
        .get("labels", {})
        .get("results", [])
    )
    return ", ".join(lb["name"] for lb in labels) if labels else ""


def _page_url(page: dict) -> str:
    links = page.get("_links", {})
    base = links.get("base", config.confluence_url)
    webui = links.get("webui", "")
    return f"{base}{webui}" if webui else ""


def _error_response(err: ConfluenceError) -> str:
    """Format a ConfluenceError into a user-friendly message."""
    return f"❌ Confluence API error (HTTP {err.status_code}): {err.message}"


def _parse_confluence_url(url: str) -> dict[str, str | None]:
    """
    Parse any Confluence URL and extract page_id, space_key, and title.

    Supported URL formats:
      1. /pages/viewpage.action?pageId=12345
      2. /display/SPACEKEY/Page+Title
      3. /display/SPACEKEY/Page+Title?src=...
      4. /spaces/SPACEKEY/pages/12345/Page+Title   (newer Data Center)
      5. /x/shortlink                               (tiny URLs)
      6. /pages/12345                               (direct ID)
      7. /wiki/display/SPACEKEY/Page+Title          (with /wiki context)
      8. /confluence/display/SPACEKEY/Page+Title     (with /confluence context)
    """
    parsed = urlparse(url.strip())
    path = parsed.path
    query = parse_qs(parsed.query)

    result: dict[str, str | None] = {
        "page_id": None,
        "space_key": None,
        "title": None,
    }

    # Format 1: ?pageId=12345
    if "pageId" in query:
        result["page_id"] = query["pageId"][0]
        return result

    # Strip common context prefixes (/wiki, /confluence, etc.)
    for prefix in ["/wiki", "/confluence"]:
        if path.startswith(prefix):
            path = path[len(prefix):]

    # Format 2/3: /display/SPACEKEY/Page+Title
    display_match = re.match(r"/display/([^/]+)/(.+?)(?:\?.*)?$", path)
    if display_match:
        result["space_key"] = display_match.group(1)
        result["title"] = unquote(display_match.group(2).replace("+", " "))
        return result

    # Format 4: /spaces/SPACEKEY/pages/12345/Page+Title
    spaces_match = re.match(r"/spaces/([^/]+)/pages/(\d+)(?:/(.+))?", path)
    if spaces_match:
        result["space_key"] = spaces_match.group(1)
        result["page_id"] = spaces_match.group(2)
        if spaces_match.group(3):
            result["title"] = unquote(spaces_match.group(3).replace("+", " "))
        return result

    # Format 5: /x/shortlink (tiny URL — need to resolve via API)
    tiny_match = re.match(r"/x/([A-Za-z0-9_-]+)", path)
    if tiny_match:
        # Tiny links can't be resolved locally; return marker
        result["page_id"] = f"tinyurl:{tiny_match.group(1)}"
        return result

    # Format 6: /pages/12345
    pages_match = re.match(r"/pages/(\d+)", path)
    if pages_match:
        result["page_id"] = pages_match.group(1)
        return result

    # Fallback: if the path contains a numeric segment, try that as page ID
    num_match = re.search(r"/(\d{4,})", path)
    if num_match:
        result["page_id"] = num_match.group(1)
        return result

    return result


# ── MCP Tools ───────────────────────────────────────────────────

@mcp.tool()
async def get_page_by_url(
    url: str,
    format: Literal["storage", "view"] = "storage",
) -> str:
    """Retrieve a Confluence page by its full URL.

    Use this when a user pastes a Confluence link. Supports all URL formats:
    /pages/viewpage.action?pageId=123, /display/SPACE/Title, /spaces/SPACE/pages/123/Title, etc.

    Args:
        url: Any Confluence page URL (full or relative path).
        format: Body format — 'storage' (raw XHTML) or 'view' (rendered HTML).
    """
    parsed = _parse_confluence_url(url)

    page_id = parsed.get("page_id")
    space_key = parsed.get("space_key")
    title = parsed.get("title")

    # If we couldn't parse anything useful
    if not page_id and not (space_key and title):
        return (
            f"❌ Could not parse the Confluence URL: {url}\n\n"
            f"Supported formats:\n"
            f"  - http://confluence/pages/viewpage.action?pageId=12345\n"
            f"  - http://confluence/display/SPACEKEY/Page+Title\n"
            f"  - http://confluence/spaces/SPACEKEY/pages/12345/Title\n\n"
            f"Try using get_page(page_id) or get_page_by_title(space_key, title) directly."
        )

    # If we have a tiny URL, we can't resolve it locally — try fetching via ID
    if page_id and page_id.startswith("tinyurl:"):
        return (
            f"❌ Tiny URLs (/x/...) need server-side resolution.\n"
            f"Try opening the link in a browser first to get the full URL, "
            f"then paste that instead."
        )

    expand = f"body.{format},version,space,metadata.labels,ancestors"

    try:
        if page_id:
            page = await client.get_page(page_id, expand=expand)
        else:
            # Lookup by space + title
            data = await client.get_page_by_title(
                space_key=space_key, title=title, expand=expand
            )
            results = data.get("results", [])
            if not results:
                return (
                    f"No page titled '{title}' found in space {space_key}.\n"
                    f"Tip: Try search_confluence with: title~\"{title}\" AND space={space_key}"
                )
            page = results[0]
    except ConfluenceError as e:
        return _error_response(e)

    # Format output (same as get_page)
    space_name = page.get("space", {}).get("name", "")
    sk = page.get("space", {}).get("key", "")
    version = page.get("version", {}).get("number", "")
    labels = _format_labels(page)
    ancestors = " → ".join(a["title"] for a in page.get("ancestors", []))
    page_url = _page_url(page)

    header = f"# {page['title']}\n"
    header += f"ID: {page['id']} | Space: {space_name} ({sk}) | Version: {version}\n"
    if labels:
        header += f"Labels: {labels}\n"
    if ancestors:
        header += f"Path: {ancestors} → {page['title']}\n"
    if page_url:
        header += f"URL: {page_url}\n"

    raw_body = page.get("body", {}).get(format, {}).get("value", "")
    body = _strip_html(raw_body) if format == "view" else raw_body
    body = _truncate(body)

    return f"{header}\n---\n\n{body}"

@mcp.tool()
async def search_confluence(
    cql: str,
    limit: int = 10,
) -> str:
    """Search Confluence pages using CQL (Confluence Query Language).

    Use this to find pages across all spaces by keyword, label, author, or date.

    Args:
        cql: A CQL query string. Examples:
             - 'type=page AND text~"deployment guide"'
             - 'type=page AND space=DEV AND text~"API"'
             - 'type=page AND label="architecture"'
             - 'type=page AND lastmodified > now("-7d") ORDER BY lastmodified DESC'
             - 'title~"release notes" AND space IN (DEV, OPS)'
             - 'creator=jsmith AND type=page'
        limit: Maximum results to return (1–50, default 10).
    """
    try:
        data = await client.search(
            cql=cql,
            limit=min(max(limit, 1), 50),
            expand="space,version,metadata.labels",
        )
    except ConfluenceError as e:
        return _error_response(e)

    results = data.get("results", [])
    if not results:
        return "No results found for that query. Try broadening your CQL or check the space key."

    total = data.get("totalSize", len(results))
    lines = [f"Found {total} result(s) — showing {len(results)}:\n"]

    for i, page in enumerate(results, 1):
        space_key = page.get("space", {}).get("key", "?")
        version = page.get("version", {}).get("number", "?")
        labels = _format_labels(page)
        url = _page_url(page)

        entry = (
            f"{i}. **{page['title']}**\n"
            f"   ID: {page['id']} | Space: {space_key} | v{version}"
        )
        if labels:
            entry += f" | Labels: {labels}"
        if url:
            entry += f"\n   URL: {url}"
        lines.append(entry)

    return "\n\n".join(lines)


@mcp.tool()
async def get_page(
    page_id: str,
    format: Literal["storage", "view"] = "storage",
    include_body: bool = True,
) -> str:
    """Retrieve a Confluence page's full content by its numeric ID.

    Args:
        page_id: The numeric page ID (e.g. '3965072').
        format: Body format — 'storage' (raw XHTML, good for parsing)
                or 'view' (rendered HTML, more readable).
        include_body: Set False to fetch only metadata (title, space, labels).
    """
    expand_parts = ["version", "space", "metadata.labels", "ancestors"]
    if include_body:
        expand_parts.append(f"body.{format}")

    try:
        page = await client.get_page(page_id, expand=",".join(expand_parts))
    except ConfluenceError as e:
        return _error_response(e)

    space_name = page.get("space", {}).get("name", "")
    space_key = page.get("space", {}).get("key", "")
    version = page.get("version", {}).get("number", "")
    labels = _format_labels(page)
    ancestors = " → ".join(a["title"] for a in page.get("ancestors", []))
    url = _page_url(page)

    header = f"# {page['title']}\n"
    header += f"Space: {space_name} ({space_key}) | Version: {version}\n"
    if labels:
        header += f"Labels: {labels}\n"
    if ancestors:
        header += f"Path: {ancestors} → {page['title']}\n"
    if url:
        header += f"URL: {url}\n"

    if not include_body:
        return header

    raw_body = page.get("body", {}).get(format, {}).get("value", "")
    body = _strip_html(raw_body) if format == "view" else raw_body
    body = _truncate(body)

    return f"{header}\n---\n\n{body}"


@mcp.tool()
async def get_page_by_title(
    space_key: str,
    title: str,
) -> str:
    """Find a Confluence page by its exact title within a space.

    Args:
        space_key: The space key (e.g. 'DEV', 'TEAM', 'HR').
        title: The exact page title to look for.
    """
    try:
        data = await client.get_page_by_title(
            space_key=space_key,
            title=title,
            expand="body.storage,version,space,metadata.labels,ancestors",
        )
    except ConfluenceError as e:
        return _error_response(e)

    results = data.get("results", [])
    if not results:
        return (
            f"No page titled '{title}' found in space {space_key}.\n"
            f"Tip: titles are case-sensitive and must be exact. "
            f"Try search_confluence with: title~\"{title}\" AND space={space_key}"
        )

    page = results[0]
    space_name = page.get("space", {}).get("name", "")
    version = page.get("version", {}).get("number", "")
    labels = _format_labels(page)
    ancestors = " → ".join(a["title"] for a in page.get("ancestors", []))
    url = _page_url(page)
    raw_body = page.get("body", {}).get("storage", {}).get("value", "")

    header = f"# {page['title']}\n"
    header += f"ID: {page['id']} | Space: {space_name} ({space_key}) | Version: {version}\n"
    if labels:
        header += f"Labels: {labels}\n"
    if ancestors:
        header += f"Path: {ancestors} → {page['title']}\n"
    if url:
        header += f"URL: {url}\n"

    return f"{header}\n---\n\n{_truncate(raw_body)}"


@mcp.tool()
async def list_spaces(
    type: Literal["global", "personal", "all"] = "global",
    limit: int = 50,
) -> str:
    """List Confluence spaces the authenticated user can access.

    Args:
        type: Filter by space type — 'global', 'personal', or 'all'.
        limit: Maximum spaces to return (default 50).
    """
    try:
        data = await client.list_spaces(
            space_type=type if type != "all" else None,
            limit=limit,
            expand="description.plain",
        )
    except ConfluenceError as e:
        return _error_response(e)

    spaces = data.get("results", [])
    if not spaces:
        return "No spaces found."

    lines = [f"## Confluence Spaces ({len(spaces)} found)\n"]
    for s in spaces:
        desc = (
            s.get("description", {})
            .get("plain", {})
            .get("value", "")
            .strip()
        )
        desc_preview = (desc[:80] + "…") if len(desc) > 80 else desc
        entry = f"- **{s['name']}** — key: `{s['key']}` ({s.get('type', '?')})"
        if desc_preview:
            entry += f"\n  {desc_preview}"
        lines.append(entry)

    return "\n".join(lines)


@mcp.tool()
async def get_comments(
    page_id: str,
    limit: int = 25,
) -> str:
    """Get comments on a Confluence page (including inline and footer comments).

    Args:
        page_id: The numeric page ID.
        limit: Maximum comments to return (default 25).
    """
    try:
        data = await client.get_child(
            page_id=page_id,
            child_type="comment",
            expand="body.view,version,extensions.inlineProperties",
            limit=limit,
        )
    except ConfluenceError as e:
        return _error_response(e)

    comments = data.get("results", [])
    if not comments:
        return "No comments on this page."

    lines = [f"## Comments ({len(comments)})\n"]
    for c in comments:
        author = (
            c.get("version", {}).get("by", {}).get("displayName", "Unknown")
        )
        when = c.get("version", {}).get("when", "")
        location = c.get("extensions", {}).get("location", "footer")
        raw_body = c.get("body", {}).get("view", {}).get("value", "")
        body = _strip_html(raw_body)

        entry = f"**{author}** ({location})"
        if when:
            entry += f" — {when}"
        entry += f"\n{body}"
        lines.append(entry)

    return "\n\n---\n\n".join(lines)


@mcp.tool()
async def get_attachments(
    page_id: str,
    limit: int = 50,
) -> str:
    """List file attachments on a Confluence page with download URLs.

    Args:
        page_id: The numeric page ID.
        limit: Maximum attachments to return (default 50).
    """
    try:
        data = await client.get_child(
            page_id=page_id,
            child_type="attachment",
            expand="version",
            limit=limit,
        )
    except ConfluenceError as e:
        return _error_response(e)

    atts = data.get("results", [])
    if not atts:
        return "No attachments on this page."

    lines = [f"## Attachments ({len(atts)})\n"]
    for a in atts:
        media_type = a.get("metadata", {}).get("mediaType", "unknown")
        download = a.get("_links", {}).get("download", "")
        full_url = f"{config.confluence_url}{download}" if download else "N/A"
        version = a.get("version", {}).get("number", "?")

        lines.append(
            f"- **{a['title']}**\n"
            f"  Type: {media_type} | Version: {version}\n"
            f"  Download: {full_url}"
        )

    return "\n".join(lines)


# ── Entrypoint ──────────────────────────────────────────────────

def main():
    logger.info("Starting Confluence MCP Server")
    logger.info("  URL:       %s", config.confluence_url)
    logger.info("  Auth:      %s", config.auth_method)
    logger.info("  Transport: %s", config.transport)

    if config.transport == "http":
        logger.info("  Listening: %s:%s", config.host, config.port)
        mcp.run(transport="streamable-http", host=config.host, port=config.port)
    else:
        logger.info("  Running in stdio mode (Claude Desktop / Claude Code)")
        mcp.run(transport="stdio")


if __name__ == "__main__":
    main()
