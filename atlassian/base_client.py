"""
Base HTTP client for all Atlassian service clients.
Handles authentication, rate limiting, retries, and error handling.
"""

import asyncio
import logging
import time
from typing import Any

import httpx

logger = logging.getLogger("atlassian-mcp")


class AtlassianError(Exception):
    """Raised when an Atlassian API call fails."""

    def __init__(self, status_code: int, message: str, service: str = "atlassian"):
        self.status_code = status_code
        self.message = message
        self.service = service
        super().__init__(f"[{service}] HTTP {status_code}: {message}")


# Backward-compatible alias
ConfluenceError = AtlassianError


class BaseAtlassianClient:
    """
    Async base client for Atlassian REST APIs.

    Subclasses set self._base to their service's API root URL and define
    service-specific convenience methods that call get/post/put.
    """

    _service_name: str = "atlassian"

    def __init__(
        self,
        base_url: str,
        token: str | None,
        username: str | None,
        password: str | None,
        ssl_verify: bool | str,
        timeout: int,
        rate_limit: int,
    ):
        self._base = base_url.rstrip("/")
        self._timeout = timeout
        self._semaphore = asyncio.Semaphore(rate_limit)
        self._last_request: float = 0.0
        self._min_interval = 1.0 / rate_limit

        self._auth: httpx.BasicAuth | None = None
        self._extra_headers: dict[str, str] = {"Accept": "application/json"}

        if token:
            self._extra_headers["Authorization"] = f"Bearer {token}"
            logger.info("[%s] Using token authentication", self._service_name)
        elif username and password:
            self._auth = httpx.BasicAuth(username, password)
            logger.info("[%s] Using Basic Auth", self._service_name)

        self._verify = ssl_verify

    def _build_client(self) -> httpx.AsyncClient:
        return httpx.AsyncClient(
            timeout=httpx.Timeout(self._timeout),
            auth=self._auth,
            headers=self._extra_headers,
            verify=self._verify,
        )

    async def _throttle(self) -> None:
        async with self._semaphore:
            now = time.monotonic()
            wait = self._min_interval - (now - self._last_request)
            if wait > 0:
                await asyncio.sleep(wait)
            self._last_request = time.monotonic()

    def _raise_for_status(self, resp: httpx.Response) -> None:
        """Raise AtlassianError for known HTTP error codes."""
        if resp.status_code == 401:
            raise AtlassianError(
                401,
                f"Authentication failed — check credentials for {self._service_name}.",
                self._service_name,
            )
        if resp.status_code == 403:
            raise AtlassianError(
                403,
                "Permission denied — the authenticated user lacks access to this resource.",
                self._service_name,
            )
        if resp.status_code == 404:
            raise AtlassianError(
                404,
                "Not found — the resource does not exist or you lack permission to view it.",
                self._service_name,
            )
        if resp.status_code >= 400:
            body = resp.text[:500]
            raise AtlassianError(resp.status_code, body, self._service_name)

    async def get(
        self,
        endpoint: str,
        params: dict[str, Any] | None = None,
        raw: bool = False,
    ) -> dict[str, Any] | str:
        """
        GET request with retry on 429/503.
        Set raw=True to return response text instead of parsed JSON.
        """
        url = f"{self._base}{endpoint}"
        await self._throttle()

        max_retries = 3
        for attempt in range(max_retries):
            async with self._build_client() as client:
                try:
                    resp = await client.get(url, params=params)
                except httpx.ConnectError as exc:
                    raise AtlassianError(
                        0, f"Cannot connect: {exc}", self._service_name
                    ) from exc
                except httpx.TimeoutException as exc:
                    raise AtlassianError(
                        0,
                        f"Request timed out after {self._timeout}s: {exc}",
                        self._service_name,
                    ) from exc

                if resp.status_code in (429, 503) and attempt < max_retries - 1:
                    delay = 2 ** (attempt + 1)
                    logger.warning(
                        "[%s] Rate limited (%s), retrying in %ds…",
                        self._service_name,
                        resp.status_code,
                        delay,
                    )
                    await asyncio.sleep(delay)
                    continue

                self._raise_for_status(resp)
                return resp.text if raw else resp.json()

        raise AtlassianError(503, "Max retries exceeded", self._service_name)

    async def post(
        self,
        endpoint: str,
        json_body: dict[str, Any] | None = None,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """POST request (no retry — write operations should not be retried automatically)."""
        url = f"{self._base}{endpoint}"
        await self._throttle()

        async with self._build_client() as client:
            try:
                resp = await client.post(url, json=json_body, params=params)
            except httpx.ConnectError as exc:
                raise AtlassianError(0, f"Cannot connect: {exc}", self._service_name) from exc
            except httpx.TimeoutException as exc:
                raise AtlassianError(
                    0, f"Request timed out after {self._timeout}s: {exc}", self._service_name
                ) from exc

            self._raise_for_status(resp)
            if resp.status_code == 204 or not resp.content:
                return {}
            return resp.json()

    async def put(
        self,
        endpoint: str,
        json_body: dict[str, Any] | None = None,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """PUT request (no retry — write operations should not be retried automatically)."""
        url = f"{self._base}{endpoint}"
        await self._throttle()

        async with self._build_client() as client:
            try:
                resp = await client.put(url, json=json_body, params=params)
            except httpx.ConnectError as exc:
                raise AtlassianError(0, f"Cannot connect: {exc}", self._service_name) from exc
            except httpx.TimeoutException as exc:
                raise AtlassianError(
                    0, f"Request timed out after {self._timeout}s: {exc}", self._service_name
                ) from exc

            self._raise_for_status(resp)
            if resp.status_code == 204 or not resp.content:
                return {}
            return resp.json()
