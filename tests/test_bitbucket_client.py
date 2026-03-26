"""Unit tests for atlassian.bitbucket.client.BitbucketClient."""

import pytest
import httpx
import respx
from config import ServiceConfig
from atlassian.bitbucket.client import BitbucketClient


def make_service_config(**kwargs) -> ServiceConfig:
    defaults = dict(
        url="https://bitbucket.example.com",
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


class TestBitbucketClientUrls:
    def test_datacenter_base_url(self):
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        assert client._base == "https://bitbucket.example.com/rest/api/1.0"

    def test_cloud_base_url(self):
        client = BitbucketClient(make_service_config(), deployment_type="cloud")
        assert client._base == "https://api.bitbucket.org/2.0"

    def test_is_cloud_property(self):
        assert BitbucketClient(make_service_config(), deployment_type="cloud").is_cloud is True
        assert BitbucketClient(make_service_config(), deployment_type="datacenter").is_cloud is False


@pytest.mark.asyncio
class TestBitbucketClientDatacenter:
    @respx.mock
    async def test_list_repos_datacenter(self):
        respx.get("https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos").mock(
            return_value=httpx.Response(200, json={"values": [{"slug": "my-repo", "name": "My Repo"}]})
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.list_repos("PROJ", limit=25)
        assert result["values"][0]["slug"] == "my-repo"

    @respx.mock
    async def test_get_repo_datacenter(self):
        respx.get("https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos/my-repo").mock(
            return_value=httpx.Response(200, json={"slug": "my-repo", "name": "My Repo"})
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.get_repo("PROJ", "my-repo")
        assert result["slug"] == "my-repo"

    @respx.mock
    async def test_list_branches_datacenter(self):
        respx.get(
            "https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos/my-repo/branches"
        ).mock(
            return_value=httpx.Response(
                200, json={"values": [{"displayId": "main", "latestCommit": "abc1234"}]}
            )
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.list_branches("PROJ", "my-repo", limit=25)
        assert result["values"][0]["displayId"] == "main"

    @respx.mock
    async def test_get_commits_datacenter(self):
        respx.get(
            "https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos/my-repo/commits"
        ).mock(
            return_value=httpx.Response(
                200, json={"values": [{"id": "abc1234", "message": "Initial commit"}]}
            )
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.get_commits("PROJ", "my-repo")
        assert result["values"][0]["id"] == "abc1234"

    @respx.mock
    async def test_list_pull_requests_datacenter(self):
        respx.get(
            "https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos/my-repo/pull-requests"
        ).mock(
            return_value=httpx.Response(200, json={"values": [{"id": 1, "title": "Add feature"}]})
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.list_pull_requests("PROJ", "my-repo")
        assert result["values"][0]["id"] == 1

    @respx.mock
    async def test_get_pull_request_datacenter(self):
        respx.get(
            "https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos/my-repo/pull-requests/1"
        ).mock(
            return_value=httpx.Response(200, json={"id": 1, "title": "Add feature"})
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.get_pull_request("PROJ", "my-repo", 1)
        assert result["title"] == "Add feature"

    @respx.mock
    async def test_create_pull_request_datacenter(self):
        route = respx.post(
            "https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos/my-repo/pull-requests"
        ).mock(
            return_value=httpx.Response(
                201, json={"id": 42, "title": "My PR", "links": {"self": [{"href": "http://..."}]}}
            )
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.create_pull_request(
            "PROJ", "my-repo", "My PR", "feature/foo", "main"
        )
        assert result["id"] == 42
        import json
        body = json.loads(route.calls[0].request.content)
        assert body["fromRef"]["id"] == "refs/heads/feature/foo"
        assert body["toRef"]["id"] == "refs/heads/main"

    @respx.mock
    async def test_get_pr_diff_datacenter(self):
        respx.get(
            "https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos/my-repo/pull-requests/1/diff"
        ).mock(
            return_value=httpx.Response(200, text="--- a/file.py\n+++ b/file.py\n@@ -1 +1 @@\n+new line")
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.get_pr_diff("PROJ", "my-repo", 1)
        assert isinstance(result, str)
        assert "--- a/file.py" in result

    @respx.mock
    async def test_add_pr_comment_datacenter(self):
        route = respx.post(
            "https://bitbucket.example.com/rest/api/1.0/projects/PROJ/repos/my-repo/pull-requests/1/comments"
        ).mock(
            return_value=httpx.Response(201, json={"id": 10, "text": "LGTM"})
        )
        client = BitbucketClient(make_service_config(), deployment_type="datacenter")
        result = await client.add_pr_comment("PROJ", "my-repo", 1, "LGTM")
        assert result["id"] == 10
        import json
        body = json.loads(route.calls[0].request.content)
        assert body["text"] == "LGTM"


@pytest.mark.asyncio
class TestBitbucketClientCloud:
    @respx.mock
    async def test_list_repos_cloud(self):
        respx.get("https://api.bitbucket.org/2.0/repositories/myworkspace").mock(
            return_value=httpx.Response(
                200, json={"values": [{"slug": "my-repo", "full_name": "myworkspace/my-repo"}]}
            )
        )
        client = BitbucketClient(make_service_config(), deployment_type="cloud")
        result = await client.list_repos("myworkspace")
        assert result["values"][0]["slug"] == "my-repo"

    @respx.mock
    async def test_create_pull_request_cloud(self):
        route = respx.post(
            "https://api.bitbucket.org/2.0/repositories/myworkspace/my-repo/pullrequests"
        ).mock(
            return_value=httpx.Response(
                201,
                json={
                    "id": 5,
                    "title": "My PR",
                    "links": {"html": {"href": "https://bitbucket.org/..."}},
                },
            )
        )
        client = BitbucketClient(make_service_config(), deployment_type="cloud")
        result = await client.create_pull_request(
            "myworkspace", "my-repo", "My PR", "feature/bar", "main"
        )
        assert result["id"] == 5
        import json
        body = json.loads(route.calls[0].request.content)
        assert body["source"]["branch"]["name"] == "feature/bar"
        assert body["destination"]["branch"]["name"] == "main"

    @respx.mock
    async def test_add_pr_comment_cloud(self):
        route = respx.post(
            "https://api.bitbucket.org/2.0/repositories/myworkspace/my-repo/pullrequests/5/comments"
        ).mock(
            return_value=httpx.Response(201, json={"id": 99})
        )
        client = BitbucketClient(make_service_config(), deployment_type="cloud")
        await client.add_pr_comment("myworkspace", "my-repo", 5, "Looks good!")
        import json
        body = json.loads(route.calls[0].request.content)
        assert body["content"]["raw"] == "Looks good!"
