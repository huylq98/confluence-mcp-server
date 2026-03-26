"""
Confluence REST API client.
Supports both Atlassian Cloud (/wiki/rest/api) and Data Center (/rest/api).
"""

from typing import Any

from atlassian.base_client import BaseAtlassianClient
from config import ServiceConfig


class ConfluenceClient(BaseAtlassianClient):
    """Async client for the Confluence REST API."""

    _service_name = "confluence"

    def __init__(self, service_config: ServiceConfig, deployment_type: str = "datacenter"):
        # Cloud Confluence lives under /wiki; Data Center does not
        if deployment_type == "cloud":
            base_url = f"{service_config.url}/wiki/rest/api"
        else:
            base_url = f"{service_config.url}/rest/api"

        super().__init__(
            base_url=base_url,
            token=service_config.token,
            username=service_config.username,
            password=service_config.password,
            ssl_verify=service_config.ssl_verify,
            timeout=service_config.timeout,
            rate_limit=service_config.rate_limit,
        )
        self._service_config = service_config
        self._base_url_root = service_config.url

    # ── Convenience methods ─────────────────────────────────────

    async def get_page(self, page_id: str, expand: str) -> dict:
        return await self.get(f"/content/{page_id}", {"expand": expand})

    async def get_page_by_title(self, space_key: str, title: str, expand: str) -> dict:
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
