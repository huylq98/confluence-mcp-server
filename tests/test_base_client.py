"""Unit tests for atlassian.base_client.BaseAtlassianClient."""

import pytest
import httpx
import respx
from atlassian.base_client import BaseAtlassianClient, AtlassianError


def make_client(**kwargs) -> BaseAtlassianClient:
    defaults = dict(
        base_url="https://api.example.com",
        token="test-token",
        username=None,
        password=None,
        ssl_verify=True,
        timeout=10,
        rate_limit=10,
    )
    defaults.update(kwargs)
    return BaseAtlassianClient(**defaults)


@pytest.mark.asyncio
class TestBaseClientGet:
    @respx.mock
    async def test_successful_get(self):
        respx.get("https://api.example.com/items").mock(
            return_value=httpx.Response(200, json={"items": [1, 2, 3]})
        )
        client = make_client()
        result = await client.get("/items")
        assert result == {"items": [1, 2, 3]}

    @respx.mock
    async def test_get_with_params(self):
        route = respx.get("https://api.example.com/search").mock(
            return_value=httpx.Response(200, json={"results": []})
        )
        client = make_client()
        await client.get("/search", {"q": "hello", "limit": 10})
        assert route.called

    @respx.mock
    async def test_401_raises_atlassian_error(self):
        respx.get("https://api.example.com/secure").mock(
            return_value=httpx.Response(401, text="Unauthorized")
        )
        client = make_client()
        with pytest.raises(AtlassianError) as exc_info:
            await client.get("/secure")
        assert exc_info.value.status_code == 401
        assert "Authentication failed" in exc_info.value.message

    @respx.mock
    async def test_403_raises_atlassian_error(self):
        respx.get("https://api.example.com/forbidden").mock(
            return_value=httpx.Response(403, text="Forbidden")
        )
        client = make_client()
        with pytest.raises(AtlassianError) as exc_info:
            await client.get("/forbidden")
        assert exc_info.value.status_code == 403

    @respx.mock
    async def test_404_raises_atlassian_error(self):
        respx.get("https://api.example.com/missing").mock(
            return_value=httpx.Response(404, text="Not Found")
        )
        client = make_client()
        with pytest.raises(AtlassianError) as exc_info:
            await client.get("/missing")
        assert exc_info.value.status_code == 404

    @respx.mock
    async def test_500_raises_atlassian_error(self):
        respx.get("https://api.example.com/error").mock(
            return_value=httpx.Response(500, text="Internal Server Error")
        )
        client = make_client()
        with pytest.raises(AtlassianError) as exc_info:
            await client.get("/error")
        assert exc_info.value.status_code == 500

    @respx.mock
    async def test_raw_mode_returns_text(self):
        respx.get("https://api.example.com/diff").mock(
            return_value=httpx.Response(200, text="--- a/file.py\n+++ b/file.py")
        )
        client = make_client()
        result = await client.get("/diff", raw=True)
        assert isinstance(result, str)
        assert "--- a/file.py" in result

    @respx.mock
    async def test_bearer_token_in_header(self):
        route = respx.get("https://api.example.com/me").mock(
            return_value=httpx.Response(200, json={"name": "user"})
        )
        client = make_client(token="my-secret-token")
        await client.get("/me")
        assert route.called
        request = route.calls[0].request
        assert request.headers.get("authorization") == "Bearer my-secret-token"

    @respx.mock
    async def test_basic_auth_used_when_no_token(self):
        route = respx.get("https://api.example.com/me").mock(
            return_value=httpx.Response(200, json={"name": "user"})
        )
        client = make_client(token=None, username="user", password="pass")
        await client.get("/me")
        assert route.called
        request = route.calls[0].request
        assert "authorization" in request.headers
        assert request.headers["authorization"].startswith("Basic ")


@pytest.mark.asyncio
class TestBaseClientPost:
    @respx.mock
    async def test_successful_post(self):
        respx.post("https://api.example.com/issues").mock(
            return_value=httpx.Response(201, json={"id": "1", "key": "PROJ-1"})
        )
        client = make_client()
        result = await client.post("/issues", {"summary": "Test issue"})
        assert result["key"] == "PROJ-1"

    @respx.mock
    async def test_post_204_returns_empty_dict(self):
        respx.post("https://api.example.com/transition").mock(
            return_value=httpx.Response(204)
        )
        client = make_client()
        result = await client.post("/transition", {})
        assert result == {}

    @respx.mock
    async def test_post_401_raises_error(self):
        respx.post("https://api.example.com/issues").mock(
            return_value=httpx.Response(401, text="Unauthorized")
        )
        client = make_client()
        with pytest.raises(AtlassianError) as exc_info:
            await client.post("/issues", {})
        assert exc_info.value.status_code == 401


@pytest.mark.asyncio
class TestBaseClientPut:
    @respx.mock
    async def test_successful_put(self):
        respx.put("https://api.example.com/issue/PROJ-1").mock(
            return_value=httpx.Response(204)
        )
        client = make_client()
        result = await client.put("/issue/PROJ-1", {"fields": {"summary": "Updated"}})
        assert result == {}

    @respx.mock
    async def test_put_with_response_body(self):
        respx.put("https://api.example.com/issue/PROJ-1").mock(
            return_value=httpx.Response(200, json={"id": "1", "key": "PROJ-1"})
        )
        client = make_client()
        result = await client.put("/issue/PROJ-1", {})
        assert result["key"] == "PROJ-1"
