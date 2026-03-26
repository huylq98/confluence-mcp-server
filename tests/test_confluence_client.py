"""Unit tests for atlassian.confluence.client.ConfluenceClient."""

import pytest
import httpx
import respx
from config import ServiceConfig
from atlassian.confluence.client import ConfluenceClient


def make_service_config(**kwargs) -> ServiceConfig:
    defaults = dict(
        url="https://wiki.example.com",
        token="test-token",
        username=None,
        password=None,
        ssl_verify=True,
        timeout=10,
        rate_limit=10,
        enabled=True,
    )
    defaults.update(kwargs)
    return ServiceConfig(**defaults)


class TestConfluenceClientUrls:
    def test_datacenter_base_url(self):
        client = ConfluenceClient(make_service_config(), deployment_type="datacenter")
        assert client._base == "https://wiki.example.com/rest/api"

    def test_cloud_base_url(self):
        client = ConfluenceClient(make_service_config(), deployment_type="cloud")
        assert client._base == "https://wiki.example.com/wiki/rest/api"


@pytest.mark.asyncio
class TestConfluenceClientMethods:
    @respx.mock
    async def test_get_page(self):
        respx.get("https://wiki.example.com/rest/api/content/123").mock(
            return_value=httpx.Response(200, json={"id": "123", "title": "My Page"})
        )
        client = ConfluenceClient(make_service_config())
        result = await client.get_page("123", expand="body.storage")
        assert result["id"] == "123"
        assert result["title"] == "My Page"

    @respx.mock
    async def test_get_page_by_title(self):
        respx.get("https://wiki.example.com/rest/api/content").mock(
            return_value=httpx.Response(200, json={"results": [{"id": "456", "title": "Hello"}]})
        )
        client = ConfluenceClient(make_service_config())
        result = await client.get_page_by_title("DEV", "Hello", expand="body.storage")
        assert result["results"][0]["id"] == "456"

    @respx.mock
    async def test_search(self):
        respx.get("https://wiki.example.com/rest/api/content/search").mock(
            return_value=httpx.Response(
                200, json={"results": [{"id": "789", "title": "Result"}], "totalSize": 1}
            )
        )
        client = ConfluenceClient(make_service_config())
        result = await client.search("type=page AND text~'hello'", limit=10, expand="space")
        assert len(result["results"]) == 1

    @respx.mock
    async def test_list_spaces(self):
        respx.get("https://wiki.example.com/rest/api/space").mock(
            return_value=httpx.Response(
                200, json={"results": [{"key": "DEV", "name": "Development"}]}
            )
        )
        client = ConfluenceClient(make_service_config())
        result = await client.list_spaces(space_type="global", limit=50, expand="description.plain")
        assert result["results"][0]["key"] == "DEV"

    @respx.mock
    async def test_get_child_comments(self):
        respx.get("https://wiki.example.com/rest/api/content/123/child/comment").mock(
            return_value=httpx.Response(200, json={"results": []})
        )
        client = ConfluenceClient(make_service_config())
        result = await client.get_child("123", "comment", expand="body.view", limit=25)
        assert result["results"] == []

    @respx.mock
    async def test_get_labels(self):
        respx.get("https://wiki.example.com/rest/api/content/123/label").mock(
            return_value=httpx.Response(200, json={"results": [{"name": "api"}]})
        )
        client = ConfluenceClient(make_service_config())
        result = await client.get_labels("123")
        assert result["results"][0]["name"] == "api"
