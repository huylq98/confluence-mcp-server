"""Test Confluence connectivity using the existing client code."""

import asyncio
import os
import sys

# Add project root to path so we can import confluence_client and config
if not getattr(sys, "frozen", False):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from config import Config
from confluence_client import ConfluenceClient, ConfluenceError

import httpx


def test_confluence_connection(url: str, username: str, password: str,
                               token: str, ssl_verify: bool) -> dict:
    """Test connection to Confluence and return result dict.

    Returns {"success": bool, "message": str, "spaces": [...]}.
    """
    url = url.strip().rstrip("/")

    # Validate inputs before attempting connection
    if not url:
        return {"success": False, "message": "Please enter the Confluence URL."}
    if not token and not (username and password):
        return {
            "success": False,
            "message": "Please enter either a Personal Access Token "
                       "or both Username and Password.",
        }

    config = Config(
        confluence_url=url,
        username=username or None,
        password=password or None,
        token=token or None,
        ssl_verify=ssl_verify,
        timeout=15,
        rate_limit=5,
        transport="stdio",
        host="0.0.0.0",
        port=8000,
        max_content_length=50000,
        default_search_limit=10,
        log_level="WARNING",
    )

    errors = config.validate()
    if errors:
        return {"success": False, "message": "; ".join(errors)}

    return asyncio.run(_test_async(config))


async def _test_async(config: Config) -> dict:
    """Run the actual async connection test."""
    client = ConfluenceClient(config)
    try:
        data = await client.list_spaces(space_type="global", limit=5, expand="")
        spaces = data.get("results", [])
        space_list = [{"name": s["name"], "key": s["key"]} for s in spaces]
        return {
            "success": True,
            "message": f"Connected! Found {len(spaces)} space(s).",
            "spaces": space_list,
        }
    except ConfluenceError as e:
        if e.status_code == 401:
            msg = "Authentication failed. Please check your username/password or token."
        elif e.status_code == 403:
            msg = "Permission denied. Your account may not have Confluence access."
        elif e.status_code == 0:
            msg = (
                "Cannot reach the server. Please check:\n"
                "- The URL is correct\n"
                "- You are connected to VPN (if required)\n"
                "- The server is running"
            )
        else:
            msg = f"Error (HTTP {e.status_code}): {e.message}"
        return {"success": False, "message": msg}
    except httpx.ConnectError:
        return {
            "success": False,
            "message": "Cannot connect to the server. Check the URL and your network/VPN.",
        }
    except Exception as e:
        error_str = str(e).lower()
        if "ssl" in error_str or "certificate" in error_str:
            return {
                "success": False,
                "message": "SSL certificate error. If your company uses a self-signed "
                           "certificate, try unchecking 'Verify SSL Certificate'.",
            }
        return {"success": False, "message": f"Unexpected error: {e}"}
