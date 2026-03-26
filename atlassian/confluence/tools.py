"""
Confluence MCP tool definitions.
Call register_tools(mcp, client, config) to add all Confluence tools to a FastMCP instance.
"""

import logging
from typing import Literal

from atlassian.base_client import AtlassianError
from atlassian.confluence.client import ConfluenceClient
from atlassian.shared.formatters import strip_html, truncate
from atlassian.shared.url_utils import parse_confluence_url
from config import AtlassianConfig

logger = logging.getLogger("atlassian-mcp.confluence")


def _format_labels(page: dict) -> str:
    labels = page.get("metadata", {}).get("labels", {}).get("results", [])
    return ", ".join(lb["name"] for lb in labels) if labels else ""


def _page_url(page: dict, base_url: str) -> str:
    links = page.get("_links", {})
    base = links.get("base", base_url)
    webui = links.get("webui", "")
    return f"{base}{webui}" if webui else ""


def _error(err: AtlassianError) -> str:
    return f"Confluence API error (HTTP {err.status_code}): {err.message}"


def register_tools(mcp, client: ConfluenceClient, config: AtlassianConfig) -> None:
    """Register all Confluence tools onto the FastMCP instance."""

    base_url = config.confluence.url

    @mcp.tool()
    async def confluence_search(cql: str, limit: int = 10) -> str:
        """Search Confluence pages using CQL (Confluence Query Language).

        Use this to find pages across all spaces by keyword, label, author, or date.

        Args:
            cql: A CQL query string. Examples:
                 - 'type=page AND text~"deployment guide"'
                 - 'type=page AND space=DEV AND text~"API"'
                 - 'type=page AND label="architecture"'
                 - 'type=page AND lastmodified > now("-7d") ORDER BY lastmodified DESC'
                 - 'title~"release notes" AND space IN (DEV, OPS)'
            limit: Maximum results to return (1–50, default 10).
        """
        try:
            data = await client.search(
                cql=cql,
                limit=min(max(limit, 1), 50),
                expand="space,version,metadata.labels",
            )
        except AtlassianError as e:
            return _error(e)

        results = data.get("results", [])
        if not results:
            return "No results found. Try broadening your CQL or check the space key."

        total = data.get("totalSize", len(results))
        lines = [f"Found {total} result(s) — showing {len(results)}:\n"]

        for i, page in enumerate(results, 1):
            sk = page.get("space", {}).get("key", "?")
            version = page.get("version", {}).get("number", "?")
            labels = _format_labels(page)
            url = _page_url(page, base_url)

            entry = (
                f"{i}. **{page['title']}**\n"
                f"   ID: {page['id']} | Space: {sk} | v{version}"
            )
            if labels:
                entry += f" | Labels: {labels}"
            if url:
                entry += f"\n   URL: {url}"
            lines.append(entry)

        return "\n\n".join(lines)

    @mcp.tool()
    async def confluence_get_page(
        page_id: str,
        format: Literal["storage", "view"] = "storage",
        include_body: bool = True,
    ) -> str:
        """Retrieve a Confluence page's full content by its numeric ID.

        Args:
            page_id: The numeric page ID (e.g. '3965072').
            format: Body format — 'storage' (raw XHTML) or 'view' (rendered HTML).
            include_body: Set False to fetch only metadata.
        """
        expand_parts = ["version", "space", "metadata.labels", "ancestors"]
        if include_body:
            expand_parts.append(f"body.{format}")

        try:
            page = await client.get_page(page_id, expand=",".join(expand_parts))
        except AtlassianError as e:
            return _error(e)

        space_name = page.get("space", {}).get("name", "")
        space_key = page.get("space", {}).get("key", "")
        version = page.get("version", {}).get("number", "")
        labels = _format_labels(page)
        ancestors = " → ".join(a["title"] for a in page.get("ancestors", []))
        url = _page_url(page, base_url)

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
        body = strip_html(raw_body) if format == "view" else raw_body
        return f"{header}\n---\n\n{truncate(body, config.max_content_length)}"

    @mcp.tool()
    async def confluence_get_page_by_url(
        url: str,
        format: Literal["storage", "view"] = "storage",
    ) -> str:
        """Retrieve a Confluence page by its full URL.

        Use this when a user pastes a Confluence link. Supports all URL formats:
        /pages/viewpage.action?pageId=123, /display/SPACE/Title,
        /spaces/SPACE/pages/123/Title, etc.

        Args:
            url: Any Confluence page URL (full or relative path).
            format: Body format — 'storage' (raw XHTML) or 'view' (rendered HTML).
        """
        parsed = parse_confluence_url(url)
        page_id = parsed.get("page_id")
        space_key = parsed.get("space_key")
        title = parsed.get("title")

        if not page_id and not (space_key and title):
            return (
                f"Could not parse the Confluence URL: {url}\n\n"
                "Supported formats:\n"
                "  - http://confluence/pages/viewpage.action?pageId=12345\n"
                "  - http://confluence/display/SPACEKEY/Page+Title\n"
                "  - http://confluence/spaces/SPACEKEY/pages/12345/Title\n\n"
                "Try using confluence_get_page(page_id) or "
                "confluence_get_page_by_title(space_key, title) directly."
            )

        if page_id and page_id.startswith("tinyurl:"):
            return (
                "Tiny URLs (/x/...) need server-side resolution.\n"
                "Open the link in a browser first to get the full URL, then paste that instead."
            )

        expand = f"body.{format},version,space,metadata.labels,ancestors"

        try:
            if page_id:
                page = await client.get_page(page_id, expand=expand)
            else:
                data = await client.get_page_by_title(
                    space_key=space_key, title=title, expand=expand
                )
                results = data.get("results", [])
                if not results:
                    return (
                        f"No page titled '{title}' found in space {space_key}.\n"
                        f'Tip: Try confluence_search with: title~"{title}" AND space={space_key}'
                    )
                page = results[0]
        except AtlassianError as e:
            return _error(e)

        space_name = page.get("space", {}).get("name", "")
        sk = page.get("space", {}).get("key", "")
        version = page.get("version", {}).get("number", "")
        labels = _format_labels(page)
        ancestors = " → ".join(a["title"] for a in page.get("ancestors", []))
        page_url = _page_url(page, base_url)

        header = f"# {page['title']}\n"
        header += f"ID: {page['id']} | Space: {space_name} ({sk}) | Version: {version}\n"
        if labels:
            header += f"Labels: {labels}\n"
        if ancestors:
            header += f"Path: {ancestors} → {page['title']}\n"
        if page_url:
            header += f"URL: {page_url}\n"

        raw_body = page.get("body", {}).get(format, {}).get("value", "")
        body = strip_html(raw_body) if format == "view" else raw_body
        return f"{header}\n---\n\n{truncate(body, config.max_content_length)}"

    @mcp.tool()
    async def confluence_get_page_by_title(space_key: str, title: str) -> str:
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
        except AtlassianError as e:
            return _error(e)

        results = data.get("results", [])
        if not results:
            return (
                f"No page titled '{title}' found in space {space_key}.\n"
                "Tip: titles are case-sensitive and must be exact. "
                f'Try confluence_search with: title~"{title}" AND space={space_key}'
            )

        page = results[0]
        space_name = page.get("space", {}).get("name", "")
        version = page.get("version", {}).get("number", "")
        labels = _format_labels(page)
        ancestors = " → ".join(a["title"] for a in page.get("ancestors", []))
        url = _page_url(page, base_url)
        raw_body = page.get("body", {}).get("storage", {}).get("value", "")

        header = f"# {page['title']}\n"
        header += f"ID: {page['id']} | Space: {space_name} ({space_key}) | Version: {version}\n"
        if labels:
            header += f"Labels: {labels}\n"
        if ancestors:
            header += f"Path: {ancestors} → {page['title']}\n"
        if url:
            header += f"URL: {url}\n"

        return f"{header}\n---\n\n{truncate(raw_body, config.max_content_length)}"

    @mcp.tool()
    async def confluence_list_spaces(
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
        except AtlassianError as e:
            return _error(e)

        spaces = data.get("results", [])
        if not spaces:
            return "No spaces found."

        lines = [f"## Confluence Spaces ({len(spaces)} found)\n"]
        for s in spaces:
            desc = s.get("description", {}).get("plain", {}).get("value", "").strip()
            desc_preview = (desc[:80] + "…") if len(desc) > 80 else desc
            entry = f"- **{s['name']}** — key: `{s['key']}` ({s.get('type', '?')})"
            if desc_preview:
                entry += f"\n  {desc_preview}"
            lines.append(entry)

        return "\n".join(lines)

    @mcp.tool()
    async def confluence_get_comments(page_id: str, limit: int = 25) -> str:
        """Get comments on a Confluence page (inline and footer comments).

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
        except AtlassianError as e:
            return _error(e)

        comments = data.get("results", [])
        if not comments:
            return "No comments on this page."

        lines = [f"## Comments ({len(comments)})\n"]
        for c in comments:
            author = c.get("version", {}).get("by", {}).get("displayName", "Unknown")
            when = c.get("version", {}).get("when", "")
            location = c.get("extensions", {}).get("location", "footer")
            raw_body = c.get("body", {}).get("view", {}).get("value", "")
            body = strip_html(raw_body)

            entry = f"**{author}** ({location})"
            if when:
                entry += f" — {when}"
            entry += f"\n{body}"
            lines.append(entry)

        return "\n\n---\n\n".join(lines)

    @mcp.tool()
    async def confluence_get_attachments(page_id: str, limit: int = 50) -> str:
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
        except AtlassianError as e:
            return _error(e)

        atts = data.get("results", [])
        if not atts:
            return "No attachments on this page."

        lines = [f"## Attachments ({len(atts)})\n"]
        for a in atts:
            media_type = a.get("metadata", {}).get("mediaType", "unknown")
            download = a.get("_links", {}).get("download", "")
            full_url = f"{base_url}{download}" if download else "N/A"
            version = a.get("version", {}).get("number", "?")

            lines.append(
                f"- **{a['title']}**\n"
                f"  Type: {media_type} | Version: {version}\n"
                f"  Download: {full_url}"
            )

        return "\n".join(lines)
