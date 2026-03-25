"""
Confluence Server REST API client.
Handles authentication, rate limiting, retries, and response parsing.
"""

import asyncio
import logging
import time
from typing import Any

import httpx

from config import Config

logger = logging.getLogger("confluence-mcp")


class ConfluenceError(Exception):
    """Raised when a Confluence API call fails."""

    def __init__(self, status_code: int, message: str):
        self.status_code = status_code
        self.message = message
        super().__init__(f"HTTP {status_code}: {message}")


class ConfluenceClient:
    """Async client for Confluence Server REST API."""

    def __init__(self, config: Config):
        self._config = config
        self._base = f"{config.confluence_url}/rest/api"
        self._semaphore = asyncio.Semaphore(config.rate_limit)
        self._last_request: float = 0.0
        self._min_interval = 1.0 / config.rate_limit

        # Build auth
        self._auth: httpx.BasicAuth | None = None
        self._extra_headers: dict[str, str] = {"Accept": "application/json"}

        if config.token:
            self._extra_headers["Authorization"] = f"Bearer {config.token}"
            logger.info("Using Personal Access Token authentication")
        elif config.username and config.password:
            self._auth = httpx.BasicAuth(config.username, config.password)
            logger.info("Using Basic Auth (username/password)")

        # SSL settings
        self._verify = config.ssl_verify

    def _build_client(self) -> httpx.AsyncClient:
        return httpx.AsyncClient(
            timeout=httpx.Timeout(self._config.timeout),
            auth=self._auth,
            headers=self._extra_headers,
            verify=self._verify,
        )

    async def _throttle(self):
        """Simple rate limiter — enforce minimum interval between requests."""
        async with self._semaphore:
            now = time.monotonic()
            wait = self._min_interval - (now - self._last_request)
            if wait > 0:
                await asyncio.sleep(wait)
            self._last_request = time.monotonic()

    async def get(
        self, endpoint: str, params: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        """
        Make a GET request to the Confluence REST API.
        Includes retry with exponential backoff for 429/503.
        """
        url = f"{self._base}{endpoint}"
        await self._throttle()

        max_retries = 3
        for attempt in range(max_retries):
            async with self._build_client() as client:
                try:
                    resp = await client.get(url, params=params)
                except httpx.ConnectError as exc:
                    raise ConfluenceError(
                        0, f"Cannot connect to {self._config.confluence_url}: {exc}"
                    ) from exc
                except httpx.TimeoutException as exc:
                    raise ConfluenceError(
                        0, f"Request timed out after {self._config.timeout}s: {exc}"
                    ) from exc

                if resp.status_code in (429, 503) and attempt < max_retries - 1:
                    delay = 2 ** (attempt + 1)
                    logger.warning(
                        "Rate limited (%s), retrying in %ds…",
                        resp.status_code,
                        delay,
                    )
                    await asyncio.sleep(delay)
                    continue

                if resp.status_code == 401:
                    raise ConfluenceError(
                        401,
                        "Authentication failed — check CONFLUENCE_USERNAME/PASSWORD "
                        "or CONFLUENCE_TOKEN in your .env file.",
                    )
                if resp.status_code == 403:
                    raise ConfluenceError(
                        403,
                        "Permission denied — the authenticated user lacks access "
                        "to this resource.",
                    )
                if resp.status_code == 404:
                    raise ConfluenceError(
                        404,
                        "Not found — the page/space does not exist or the user "
                        "lacks permission to view it.",
                    )
                if resp.status_code >= 400:
                    body = resp.text[:500]
                    raise ConfluenceError(resp.status_code, body)

                return resp.json()

        # Should not reach here, but just in case
        raise ConfluenceError(503, "Max retries exceeded")

    # ── Convenience methods ─────────────────────────────────────

    async def get_page(self, page_id: str, expand: str) -> dict:
        return await self.get(f"/content/{page_id}", {"expand": expand})

    async def get_page_by_title(
        self, space_key: str, title: str, expand: str
    ) -> dict:
        return await self.get(
            "/content",
            {"spaceKey": space_key, "title": title, "expand": expand, "limit": 1},
        )

    async def search(self, cql: str, limit: int, expand: str) -> dict:
        return await self.get(
            "/content/search",
            {"cql": cql, "limit": limit, "expand": expand},
        )

    async def list_spaces(
        self, space_type: str | None, limit: int, expand: str
    ) -> dict:
        params: dict[str, Any] = {"limit": limit, "expand": expand}
        if space_type and space_type != "all":
            params["type"] = space_type
        return await self.get("/space", params)

    async def get_child(
        self, page_id: str, child_type: str, expand: str, limit: int
    ) -> dict:
        return await self.get(
            f"/content/{page_id}/child/{child_type}",
            {"expand": expand, "limit": limit},
        )

    async def get_labels(self, page_id: str) -> dict:
        return await self.get(f"/content/{page_id}/label")
