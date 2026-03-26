"""
Bitbucket REST API client.
Supports both Bitbucket Cloud (api.bitbucket.org/2.0) and Bitbucket Data Center (REST API 1.0).
"""

from typing import Any

from atlassian.base_client import BaseAtlassianClient
from config import ServiceConfig


class BitbucketClient(BaseAtlassianClient):
    """Async client for the Bitbucket REST API."""

    _service_name = "bitbucket"

    def __init__(self, service_config: ServiceConfig, deployment_type: str = "datacenter"):
        if deployment_type == "cloud":
            base_url = "https://api.bitbucket.org/2.0"
        else:
            base_url = f"{service_config.url}/rest/api/1.0"

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

    # ── Repository methods ──────────────────────────────────────

    async def list_repos(
        self, workspace_or_project: str, limit: int = 25, start: int = 0
    ) -> dict:
        if self.is_cloud:
            return await self.get(
                f"/repositories/{workspace_or_project}",
                {"pagelen": limit, "page": (start // limit) + 1},
            )
        return await self.get(
            f"/projects/{workspace_or_project}/repos",
            {"limit": limit, "start": start},
        )

    async def get_repo(self, workspace_or_project: str, repo_slug: str) -> dict:
        if self.is_cloud:
            return await self.get(f"/repositories/{workspace_or_project}/{repo_slug}")
        return await self.get(f"/projects/{workspace_or_project}/repos/{repo_slug}")

    # ── Branch methods ──────────────────────────────────────────

    async def list_branches(
        self,
        workspace_or_project: str,
        repo_slug: str,
        limit: int = 25,
        filter_text: str = "",
    ) -> dict:
        params: dict[str, Any] = {}
        if self.is_cloud:
            params["pagelen"] = limit
            if filter_text:
                params["q"] = f'name ~ "{filter_text}"'
            return await self.get(
                f"/repositories/{workspace_or_project}/{repo_slug}/refs/branches",
                params,
            )
        params["limit"] = limit
        if filter_text:
            params["filterText"] = filter_text
        return await self.get(
            f"/projects/{workspace_or_project}/repos/{repo_slug}/branches",
            params,
        )

    # ── Commit methods ──────────────────────────────────────────

    async def get_commits(
        self,
        workspace_or_project: str,
        repo_slug: str,
        branch: str = "",
        limit: int = 10,
    ) -> dict:
        params: dict[str, Any] = {}
        if self.is_cloud:
            params["pagelen"] = limit
            if branch:
                params["include"] = branch
            return await self.get(
                f"/repositories/{workspace_or_project}/{repo_slug}/commits",
                params,
            )
        params["limit"] = limit
        if branch:
            params["until"] = branch
        return await self.get(
            f"/projects/{workspace_or_project}/repos/{repo_slug}/commits",
            params,
        )

    async def get_commit(
        self, workspace_or_project: str, repo_slug: str, commit_hash: str
    ) -> dict:
        if self.is_cloud:
            return await self.get(
                f"/repositories/{workspace_or_project}/{repo_slug}/commit/{commit_hash}"
            )
        return await self.get(
            f"/projects/{workspace_or_project}/repos/{repo_slug}/commits/{commit_hash}"
        )

    # ── Pull request methods ─────────────────────────────────────

    async def list_pull_requests(
        self,
        workspace_or_project: str,
        repo_slug: str,
        state: str = "OPEN",
        limit: int = 10,
    ) -> dict:
        if self.is_cloud:
            return await self.get(
                f"/repositories/{workspace_or_project}/{repo_slug}/pullrequests",
                {"state": state, "pagelen": limit},
            )
        return await self.get(
            f"/projects/{workspace_or_project}/repos/{repo_slug}/pull-requests",
            {"state": state, "limit": limit},
        )

    async def get_pull_request(
        self, workspace_or_project: str, repo_slug: str, pr_id: int
    ) -> dict:
        if self.is_cloud:
            return await self.get(
                f"/repositories/{workspace_or_project}/{repo_slug}/pullrequests/{pr_id}"
            )
        return await self.get(
            f"/projects/{workspace_or_project}/repos/{repo_slug}/pull-requests/{pr_id}"
        )

    async def create_pull_request(
        self,
        workspace_or_project: str,
        repo_slug: str,
        title: str,
        source_branch: str,
        dest_branch: str,
        description: str = "",
        reviewer_ids: list[str] | None = None,
    ) -> dict:
        if self.is_cloud:
            body: dict[str, Any] = {
                "title": title,
                "source": {"branch": {"name": source_branch}},
                "destination": {"branch": {"name": dest_branch}},
            }
            if description:
                body["description"] = description
            if reviewer_ids:
                body["reviewers"] = [{"uuid": rid} for rid in reviewer_ids]
            return await self.post(
                f"/repositories/{workspace_or_project}/{repo_slug}/pullrequests",
                body,
            )
        # Data Center
        body = {
            "title": title,
            "fromRef": {"id": f"refs/heads/{source_branch}"},
            "toRef": {"id": f"refs/heads/{dest_branch}"},
        }
        if description:
            body["description"] = description
        if reviewer_ids:
            body["reviewers"] = [{"user": {"slug": rid}} for rid in reviewer_ids]
        return await self.post(
            f"/projects/{workspace_or_project}/repos/{repo_slug}/pull-requests",
            body,
        )

    async def get_pr_diff(
        self, workspace_or_project: str, repo_slug: str, pr_id: int
    ) -> str:
        if self.is_cloud:
            return await self.get(
                f"/repositories/{workspace_or_project}/{repo_slug}/pullrequests/{pr_id}/diff",
                raw=True,
            )
        return await self.get(
            f"/projects/{workspace_or_project}/repos/{repo_slug}/pull-requests/{pr_id}/diff",
            raw=True,
        )

    async def add_pr_comment(
        self,
        workspace_or_project: str,
        repo_slug: str,
        pr_id: int,
        text: str,
    ) -> dict:
        if self.is_cloud:
            return await self.post(
                f"/repositories/{workspace_or_project}/{repo_slug}/pullrequests/{pr_id}/comments",
                {"content": {"raw": text}},
            )
        return await self.post(
            f"/projects/{workspace_or_project}/repos/{repo_slug}/pull-requests/{pr_id}/comments",
            {"text": text},
        )
