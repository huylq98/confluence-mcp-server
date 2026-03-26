"""
Bitbucket MCP tool definitions.
Call register_tools(mcp, client, config) to add all Bitbucket tools to a FastMCP instance.
"""

import logging

from atlassian.base_client import AtlassianError
from atlassian.bitbucket.client import BitbucketClient
from atlassian.shared.formatters import truncate
from config import AtlassianConfig

logger = logging.getLogger("atlassian-mcp.bitbucket")


def _error(err: AtlassianError) -> str:
    return f"Bitbucket API error (HTTP {err.status_code}): {err.message}"


def _format_commit(commit: dict, is_cloud: bool) -> str:
    if is_cloud:
        sha = commit.get("hash", "?")[:8]
        msg = commit.get("message", "").split("\n")[0]
        author = commit.get("author", {}).get("user", {}).get("display_name", "")
        if not author:
            author = commit.get("author", {}).get("raw", "?")
        date = (commit.get("date") or "")[:10]
    else:
        sha = commit.get("id", "?")[:8]
        msg = commit.get("message", "").split("\n")[0]
        author_obj = commit.get("author") or {}
        author = author_obj.get("name", author_obj.get("emailAddress", "?"))
        ts = commit.get("authorTimestamp", 0)
        date = ""
        if ts:
            import datetime
            date = datetime.datetime.fromtimestamp(ts / 1000, tz=datetime.timezone.utc).strftime(
                "%Y-%m-%d"
            )
    return f"`{sha}` {msg} — {author}{f' ({date})' if date else ''}"


def _format_pr(pr: dict, is_cloud: bool) -> str:
    if is_cloud:
        pr_id = pr.get("id", "?")
        title = pr.get("title", "?")
        state = pr.get("state", "?")
        author = pr.get("author", {}).get("display_name", "?")
        src = pr.get("source", {}).get("branch", {}).get("name", "?")
        dst = pr.get("destination", {}).get("branch", {}).get("name", "?")
        updated = (pr.get("updated_on") or "")[:10]
    else:
        pr_id = pr.get("id", "?")
        title = pr.get("title", "?")
        state = pr.get("state", "?")
        author = pr.get("author", {}).get("displayName", "?")
        src = pr.get("fromRef", {}).get("displayId", "?")
        dst = pr.get("toRef", {}).get("displayId", "?")
        updated = ""
        ts = pr.get("updatedDate", 0)
        if ts:
            import datetime
            updated = datetime.datetime.fromtimestamp(
                ts / 1000, tz=datetime.timezone.utc
            ).strftime("%Y-%m-%d")

    line = f"PR #{pr_id}: **{title}** [{state}]"
    line += f"\n   {src} → {dst} | Author: {author}"
    if updated:
        line += f" | Updated: {updated}"
    return line


def register_tools(mcp, client: BitbucketClient, config: AtlassianConfig) -> None:
    """Register all Bitbucket tools onto the FastMCP instance."""

    is_cloud = client.is_cloud

    @mcp.tool()
    async def bitbucket_list_repos(workspace: str, limit: int = 25) -> str:
        """List repositories in a Bitbucket workspace (Cloud) or project (Data Center).

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center), e.g. 'myteam' or 'PROJ'.
            limit: Maximum repositories to return (default 25).
        """
        try:
            data = await client.list_repos(workspace, limit=min(limit, 100))
        except AtlassianError as e:
            return _error(e)

        repos = data.get("values", [])
        if not repos:
            return f"No repositories found in '{workspace}'."

        total = data.get("size") or data.get("total") or len(repos)
        lines = [f"## Repositories in '{workspace}' ({total} total — showing {len(repos)})\n"]
        for r in repos:
            if is_cloud:
                slug = r.get("slug", "?")
                name = r.get("full_name", slug)
                desc = (r.get("description") or "").strip()
                lang = r.get("language", "")
                updated = (r.get("updated_on") or "")[:10]
            else:
                slug = r.get("slug", "?")
                name = r.get("name", slug)
                desc = (r.get("description") or "").strip()
                lang = ""
                updated = ""

            entry = f"- **{name}** (`{slug}`)"
            if lang:
                entry += f" [{lang}]"
            if updated:
                entry += f" — updated {updated}"
            if desc:
                entry += f"\n  {desc[:100]}"
            lines.append(entry)

        return "\n".join(lines)

    @mcp.tool()
    async def bitbucket_get_repo(workspace: str, repo_slug: str) -> str:
        """Get details for a specific Bitbucket repository.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug (e.g. 'my-repo').
        """
        try:
            repo = await client.get_repo(workspace, repo_slug)
        except AtlassianError as e:
            return _error(e)

        if is_cloud:
            name = repo.get("full_name", repo_slug)
            desc = (repo.get("description") or "").strip()
            lang = repo.get("language", "")
            created = (repo.get("created_on") or "")[:10]
            updated = (repo.get("updated_on") or "")[:10]
            clone_links = repo.get("links", {}).get("clone", [])
            clone_url = next(
                (c.get("href") for c in clone_links if c.get("name") == "https"), ""
            )
            size = repo.get("size", 0)
        else:
            name = repo.get("name", repo_slug)
            desc = (repo.get("description") or "").strip()
            lang = ""
            created = updated = ""
            clone_links = repo.get("links", {}).get("clone", [])
            clone_url = next(
                (c.get("href") for c in clone_links if c.get("name") == "http"), ""
            )
            size = 0

        lines = [f"## {name}"]
        if desc:
            lines.append(desc)
        if lang:
            lines.append(f"Language: {lang}")
        if created:
            lines.append(f"Created: {created} | Updated: {updated}")
        if size:
            lines.append(f"Size: {size:,} bytes")
        if clone_url:
            lines.append(f"Clone: {clone_url}")

        return "\n".join(lines)

    @mcp.tool()
    async def bitbucket_list_branches(
        workspace: str, repo_slug: str, limit: int = 25, filter: str = ""
    ) -> str:
        """List branches in a Bitbucket repository.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug.
            limit: Maximum branches to return (default 25).
            filter: Filter branches by name substring.
        """
        try:
            data = await client.list_branches(workspace, repo_slug, limit=min(limit, 100), filter_text=filter)
        except AtlassianError as e:
            return _error(e)

        branches = data.get("values", [])
        if not branches:
            return f"No branches found in '{workspace}/{repo_slug}'."

        lines = [f"## Branches in {workspace}/{repo_slug} ({len(branches)} shown)\n"]
        for b in branches:
            if is_cloud:
                name = b.get("name", "?")
                sha = (b.get("target") or {}).get("hash", "?")[:8]
            else:
                name = b.get("displayId", b.get("id", "?"))
                sha = (b.get("latestCommit") or "?")[:8]
            lines.append(f"- `{name}` — latest commit: {sha}")

        return "\n".join(lines)

    @mcp.tool()
    async def bitbucket_get_commits(
        workspace: str, repo_slug: str, branch: str = "", limit: int = 10
    ) -> str:
        """Get recent commits in a Bitbucket repository.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug.
            branch: Branch name to filter commits (leave empty for default branch).
            limit: Maximum commits to return (default 10).
        """
        try:
            data = await client.get_commits(workspace, repo_slug, branch=branch, limit=min(limit, 50))
        except AtlassianError as e:
            return _error(e)

        commits = data.get("values", [])
        if not commits:
            return "No commits found."

        scope = f" on `{branch}`" if branch else ""
        lines = [f"## Recent Commits in {workspace}/{repo_slug}{scope}\n"]
        for c in commits:
            lines.append(f"- {_format_commit(c, is_cloud)}")

        return "\n".join(lines)

    @mcp.tool()
    async def bitbucket_get_commit(workspace: str, repo_slug: str, commit_hash: str) -> str:
        """Get details for a specific commit.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug.
            commit_hash: Full or abbreviated commit hash.
        """
        try:
            commit = await client.get_commit(workspace, repo_slug, commit_hash)
        except AtlassianError as e:
            return _error(e)

        return f"## Commit {commit_hash[:8]}\n{_format_commit(commit, is_cloud)}"

    @mcp.tool()
    async def bitbucket_list_pull_requests(
        workspace: str,
        repo_slug: str,
        state: str = "OPEN",
        limit: int = 10,
    ) -> str:
        """List pull requests in a Bitbucket repository.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug.
            state: PR state filter — 'OPEN', 'MERGED', 'DECLINED' (or 'ALL' for Data Center).
            limit: Maximum PRs to return (default 10).
        """
        try:
            data = await client.list_pull_requests(workspace, repo_slug, state=state, limit=min(limit, 50))
        except AtlassianError as e:
            return _error(e)

        prs = data.get("values", [])
        if not prs:
            return f"No {state.lower()} pull requests in {workspace}/{repo_slug}."

        total = data.get("size") or data.get("totalCount") or len(prs)
        lines = [f"## {state} Pull Requests in {workspace}/{repo_slug} ({total} total)\n"]
        for i, pr in enumerate(prs, 1):
            lines.append(f"{i}. {_format_pr(pr, is_cloud)}")

        return "\n\n".join(lines)

    @mcp.tool()
    async def bitbucket_get_pull_request(
        workspace: str, repo_slug: str, pr_id: int
    ) -> str:
        """Get full details of a Bitbucket pull request.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug.
            pr_id: The pull request ID number.
        """
        try:
            pr = await client.get_pull_request(workspace, repo_slug, pr_id)
        except AtlassianError as e:
            return _error(e)

        lines = [_format_pr(pr, is_cloud)]

        if is_cloud:
            desc = (pr.get("description") or "").strip()
            reviewers = pr.get("reviewers", [])
        else:
            desc = (pr.get("description") or "").strip()
            reviewers = pr.get("reviewers", [])

        if desc:
            lines.append(f"\n## Description\n{desc}")

        if reviewers:
            if is_cloud:
                names = [r.get("display_name", "?") for r in reviewers]
            else:
                names = [r.get("user", {}).get("displayName", "?") for r in reviewers]
            lines.append(f"\nReviewers: {', '.join(names)}")

        return "\n".join(lines)

    @mcp.tool()
    async def bitbucket_create_pull_request(
        workspace: str,
        repo_slug: str,
        title: str,
        source_branch: str,
        destination_branch: str,
        description: str = "",
        reviewer_ids: str = "",
    ) -> str:
        """Create a new Bitbucket pull request.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug.
            title: Pull request title.
            source_branch: The branch with your changes.
            destination_branch: The branch to merge into (e.g. 'main', 'master', 'develop').
            description: Optional PR description.
            reviewer_ids: Comma-separated reviewer UUIDs (Cloud) or usernames (Data Center).
        """
        reviewer_list = [r.strip() for r in reviewer_ids.split(",") if r.strip()] if reviewer_ids else None
        try:
            result = await client.create_pull_request(
                workspace_or_project=workspace,
                repo_slug=repo_slug,
                title=title,
                source_branch=source_branch,
                dest_branch=destination_branch,
                description=description,
                reviewer_ids=reviewer_list,
            )
        except AtlassianError as e:
            return _error(e)

        pr_id = result.get("id", "?")
        pr_title = result.get("title", title)
        if is_cloud:
            pr_url = result.get("links", {}).get("html", {}).get("href", "")
        else:
            pr_url = (result.get("links", {}).get("self") or [{}])[0].get("href", "")

        out = f"Pull request created: **PR #{pr_id}** — {pr_title}\n{source_branch} → {destination_branch}"
        if pr_url:
            out += f"\nURL: {pr_url}"
        return out

    @mcp.tool()
    async def bitbucket_get_pr_diff(workspace: str, repo_slug: str, pr_id: int) -> str:
        """Get the unified diff for a Bitbucket pull request.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug.
            pr_id: The pull request ID number.
        """
        try:
            diff_text = await client.get_pr_diff(workspace, repo_slug, pr_id)
        except AtlassianError as e:
            return _error(e)

        return truncate(str(diff_text), config.max_content_length)

    @mcp.tool()
    async def bitbucket_add_pr_comment(
        workspace: str, repo_slug: str, pr_id: int, comment: str
    ) -> str:
        """Add a comment to a Bitbucket pull request.

        Args:
            workspace: Workspace slug (Cloud) or project key (Data Center).
            repo_slug: Repository slug.
            pr_id: The pull request ID number.
            comment: The comment text.
        """
        try:
            result = await client.add_pr_comment(workspace, repo_slug, pr_id, comment)
        except AtlassianError as e:
            return _error(e)

        comment_id = result.get("id", "?")
        return f"Comment added to PR #{pr_id} (comment ID: {comment_id})."
