"""
Confluence URL parsing utilities.
"""

import re
from urllib.parse import urlparse, parse_qs, unquote


def parse_confluence_url(url: str) -> dict[str, str | None]:
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
