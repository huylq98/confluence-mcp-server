"""
Jira REST API client.
Supports both Atlassian Cloud (/rest/api/3) and Data Center (/rest/api/2).
"""

from typing import Any

from atlassian.base_client import BaseAtlassianClient
from config import ServiceConfig


class JiraClient(BaseAtlassianClient):
    """Async client for the Jira REST API."""

    _service_name = "jira"

    def __init__(self, service_config: ServiceConfig, deployment_type: str = "datacenter"):
        # Cloud uses API v3 (ADF); Data Center uses v2 (plain text)
        api_version = "3" if deployment_type == "cloud" else "2"
        base_url = f"{service_config.url}/rest/api/{api_version}"

        super().__init__(
            base_url=base_url,
            token=service_config.token,
            username=service_config.username,
            password=service_config.password,
            ssl_verify=service_config.ssl_verify,
            timeout=service_config.timeout,
            rate_limit=service_config.rate_limit,
        )
        self._deployment_type = deployment_type
        self._service_config = service_config

    @property
    def is_cloud(self) -> bool:
        return self._deployment_type == "cloud"

    def _description_body(self, text: str) -> dict | str:
        """Return description payload in the correct format for this deployment."""
        if self.is_cloud:
            # Atlassian Document Format (ADF)
            return {
                "type": "doc",
                "version": 1,
                "content": [
                    {
                        "type": "paragraph",
                        "content": [{"type": "text", "text": text}],
                    }
                ],
            }
        return text

    # ── Issue methods ───────────────────────────────────────────

    async def get_issue(self, issue_key: str, expand: str = "") -> dict:
        params: dict[str, Any] = {}
        if expand:
            params["expand"] = expand
        return await self.get(f"/issue/{issue_key}", params or None)

    async def search_issues(
        self,
        jql: str,
        limit: int = 10,
        fields: str = "summary,status,assignee,priority,updated,issuetype",
        start_at: int = 0,
    ) -> dict:
        return await self.get(
            "/search",
            {"jql": jql, "maxResults": limit, "fields": fields, "startAt": start_at},
        )

    async def create_issue(
        self,
        project_key: str,
        summary: str,
        issue_type: str,
        description: str = "",
        priority: str = "",
        labels: list[str] | None = None,
        assignee_id: str = "",
    ) -> dict:
        fields: dict[str, Any] = {
            "project": {"key": project_key},
            "summary": summary,
            "issuetype": {"name": issue_type},
        }
        if description:
            fields["description"] = self._description_body(description)
        if priority:
            fields["priority"] = {"name": priority}
        if labels:
            fields["labels"] = labels
        if assignee_id:
            key = "accountId" if self.is_cloud else "name"
            fields["assignee"] = {key: assignee_id}

        return await self.post("/issue", {"fields": fields})

    async def update_issue(
        self,
        issue_key: str,
        summary: str = "",
        description: str = "",
        priority: str = "",
        labels: list[str] | None = None,
    ) -> dict:
        fields: dict[str, Any] = {}
        if summary:
            fields["summary"] = summary
        if description:
            fields["description"] = self._description_body(description)
        if priority:
            fields["priority"] = {"name": priority}
        if labels is not None:
            fields["labels"] = labels

        return await self.put(f"/issue/{issue_key}", {"fields": fields})

    async def get_transitions(self, issue_key: str) -> dict:
        return await self.get(f"/issue/{issue_key}/transitions")

    async def transition_issue(self, issue_key: str, transition_id: str) -> dict:
        return await self.post(
            f"/issue/{issue_key}/transitions",
            {"transition": {"id": transition_id}},
        )

    async def add_comment(self, issue_key: str, body: str) -> dict:
        comment_body = self._description_body(body) if self.is_cloud else body
        return await self.post(
            f"/issue/{issue_key}/comment",
            {"body": comment_body},
        )

    async def get_comments(self, issue_key: str, limit: int = 25) -> dict:
        return await self.get(
            f"/issue/{issue_key}/comment",
            {"maxResults": limit},
        )

    async def assign_issue(self, issue_key: str, assignee_id: str) -> dict:
        key = "accountId" if self.is_cloud else "name"
        return await self.put(
            f"/issue/{issue_key}/assignee",
            {key: assignee_id},
        )

    # ── Project methods ─────────────────────────────────────────

    async def list_projects(self, limit: int = 50, start_at: int = 0) -> dict:
        return await self.get(
            "/project/search",
            {"maxResults": limit, "startAt": start_at},
        )

    async def get_project(self, project_key: str) -> dict:
        return await self.get(f"/project/{project_key}", {"expand": "issueTypes"})
