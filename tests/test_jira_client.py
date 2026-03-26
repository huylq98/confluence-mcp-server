"""Unit tests for atlassian.jira.client.JiraClient."""

import pytest
import httpx
import respx
from config import ServiceConfig
from atlassian.jira.client import JiraClient


def make_service_config(**kwargs) -> ServiceConfig:
    defaults = dict(
        url="https://jira.example.com",
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


class TestJiraClientUrls:
    def test_datacenter_uses_api_v2(self):
        client = JiraClient(make_service_config(), deployment_type="datacenter")
        assert client._base == "https://jira.example.com/rest/api/2"

    def test_cloud_uses_api_v3(self):
        client = JiraClient(make_service_config(), deployment_type="cloud")
        assert client._base == "https://jira.example.com/rest/api/3"

    def test_is_cloud_property(self):
        assert JiraClient(make_service_config(), deployment_type="cloud").is_cloud is True
        assert JiraClient(make_service_config(), deployment_type="datacenter").is_cloud is False


class TestJiraDescriptionBody:
    def test_datacenter_returns_plain_string(self):
        client = JiraClient(make_service_config(), deployment_type="datacenter")
        result = client._description_body("My description")
        assert result == "My description"

    def test_cloud_returns_adf(self):
        client = JiraClient(make_service_config(), deployment_type="cloud")
        result = client._description_body("My description")
        assert isinstance(result, dict)
        assert result["type"] == "doc"
        assert result["version"] == 1
        content = result["content"][0]
        assert content["type"] == "paragraph"
        assert content["content"][0]["text"] == "My description"


@pytest.mark.asyncio
class TestJiraClientMethods:
    @respx.mock
    async def test_get_issue(self):
        respx.get("https://jira.example.com/rest/api/2/issue/PROJ-1").mock(
            return_value=httpx.Response(
                200,
                json={
                    "key": "PROJ-1",
                    "fields": {"summary": "Test issue", "status": {"name": "Open"}},
                },
            )
        )
        client = JiraClient(make_service_config())
        result = await client.get_issue("PROJ-1")
        assert result["key"] == "PROJ-1"

    @respx.mock
    async def test_search_issues(self):
        respx.get("https://jira.example.com/rest/api/2/search").mock(
            return_value=httpx.Response(
                200, json={"issues": [{"key": "PROJ-1"}], "total": 1}
            )
        )
        client = JiraClient(make_service_config())
        result = await client.search_issues("project = PROJ")
        assert len(result["issues"]) == 1

    @respx.mock
    async def test_create_issue(self):
        respx.post("https://jira.example.com/rest/api/2/issue").mock(
            return_value=httpx.Response(201, json={"id": "10001", "key": "PROJ-2"})
        )
        client = JiraClient(make_service_config())
        result = await client.create_issue("PROJ", "New Issue", "Task", "A description")
        assert result["key"] == "PROJ-2"

    @respx.mock
    async def test_create_issue_cloud_uses_adf(self):
        route = respx.post("https://jira.example.com/rest/api/3/issue").mock(
            return_value=httpx.Response(201, json={"id": "10001", "key": "PROJ-3"})
        )
        client = JiraClient(make_service_config(), deployment_type="cloud")
        await client.create_issue("PROJ", "New Issue", "Task", "Some description")
        import json
        body = json.loads(route.calls[0].request.content)
        assert isinstance(body["fields"]["description"], dict)
        assert body["fields"]["description"]["type"] == "doc"

    @respx.mock
    async def test_update_issue(self):
        respx.put("https://jira.example.com/rest/api/2/issue/PROJ-1").mock(
            return_value=httpx.Response(204)
        )
        client = JiraClient(make_service_config())
        result = await client.update_issue("PROJ-1", summary="Updated Summary")
        assert result == {}

    @respx.mock
    async def test_get_transitions(self):
        respx.get("https://jira.example.com/rest/api/2/issue/PROJ-1/transitions").mock(
            return_value=httpx.Response(
                200,
                json={
                    "transitions": [
                        {"id": "11", "name": "Start Progress", "to": {"name": "In Progress"}},
                        {"id": "21", "name": "Done", "to": {"name": "Done"}},
                    ]
                },
            )
        )
        client = JiraClient(make_service_config())
        result = await client.get_transitions("PROJ-1")
        assert len(result["transitions"]) == 2

    @respx.mock
    async def test_transition_issue(self):
        respx.post("https://jira.example.com/rest/api/2/issue/PROJ-1/transitions").mock(
            return_value=httpx.Response(204)
        )
        client = JiraClient(make_service_config())
        result = await client.transition_issue("PROJ-1", "21")
        assert result == {}

    @respx.mock
    async def test_add_comment_datacenter(self):
        respx.post("https://jira.example.com/rest/api/2/issue/PROJ-1/comment").mock(
            return_value=httpx.Response(201, json={"id": "10100", "body": "Test comment"})
        )
        client = JiraClient(make_service_config(), deployment_type="datacenter")
        result = await client.add_comment("PROJ-1", "Test comment")
        assert result["id"] == "10100"

    @respx.mock
    async def test_list_projects(self):
        respx.get("https://jira.example.com/rest/api/2/project/search").mock(
            return_value=httpx.Response(
                200,
                json={"values": [{"key": "PROJ", "name": "My Project"}], "total": 1},
            )
        )
        client = JiraClient(make_service_config())
        result = await client.list_projects()
        assert result["values"][0]["key"] == "PROJ"

    @respx.mock
    async def test_get_project(self):
        respx.get("https://jira.example.com/rest/api/2/project/PROJ").mock(
            return_value=httpx.Response(
                200,
                json={"key": "PROJ", "name": "My Project", "issueTypes": []},
            )
        )
        client = JiraClient(make_service_config())
        result = await client.get_project("PROJ")
        assert result["key"] == "PROJ"

    @respx.mock
    async def test_assign_issue_cloud(self):
        route = respx.put("https://jira.example.com/rest/api/3/issue/PROJ-1/assignee").mock(
            return_value=httpx.Response(204)
        )
        client = JiraClient(make_service_config(), deployment_type="cloud")
        await client.assign_issue("PROJ-1", "account123")
        import json
        body = json.loads(route.calls[0].request.content)
        assert body.get("accountId") == "account123"

    @respx.mock
    async def test_assign_issue_datacenter(self):
        route = respx.put("https://jira.example.com/rest/api/2/issue/PROJ-1/assignee").mock(
            return_value=httpx.Response(204)
        )
        client = JiraClient(make_service_config(), deployment_type="datacenter")
        await client.assign_issue("PROJ-1", "jsmith")
        import json
        body = json.loads(route.calls[0].request.content)
        assert body.get("name") == "jsmith"
