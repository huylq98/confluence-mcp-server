"""
Jira MCP tool definitions.
Call register_tools(mcp, client, config) to add all Jira tools to a FastMCP instance.
"""

import logging

from atlassian.base_client import AtlassianError
from atlassian.jira.client import JiraClient
from atlassian.shared.formatters import adf_to_text, truncate
from config import AtlassianConfig

logger = logging.getLogger("atlassian-mcp.jira")


def _error(err: AtlassianError) -> str:
    return f"Jira API error (HTTP {err.status_code}): {err.message}"


def _format_issue_summary(issue: dict) -> str:
    """Format a single issue into a compact summary line."""
    key = issue.get("key", "?")
    fields = issue.get("fields", {})
    summary = fields.get("summary", "")
    status = fields.get("status", {}).get("name", "?")
    assignee = (fields.get("assignee") or {}).get("displayName", "Unassigned")
    priority = (fields.get("priority") or {}).get("name", "?")
    issue_type = (fields.get("issuetype") or {}).get("name", "?")
    updated = fields.get("updated", "")[:10]  # YYYY-MM-DD

    line = f"**{key}** [{issue_type}] {summary}"
    line += f"\n   Status: {status} | Priority: {priority} | Assignee: {assignee}"
    if updated:
        line += f" | Updated: {updated}"
    return line


def _format_issue_detail(issue: dict) -> str:
    """Format a single issue with full details."""
    key = issue.get("key", "?")
    fields = issue.get("fields", {})

    summary = fields.get("summary", "")
    status = fields.get("status", {}).get("name", "?")
    issue_type = (fields.get("issuetype") or {}).get("name", "?")
    priority = (fields.get("priority") or {}).get("name", "?")
    assignee = (fields.get("assignee") or {}).get("displayName", "Unassigned")
    reporter = (fields.get("reporter") or {}).get("displayName", "Unknown")
    created = (fields.get("created") or "")[:10]
    updated = (fields.get("updated") or "")[:10]
    labels = ", ".join(fields.get("labels") or [])
    components = ", ".join(c.get("name", "") for c in (fields.get("components") or []))
    fix_versions = ", ".join(v.get("name", "") for v in (fields.get("fixVersions") or []))

    # Description — handle both ADF (Cloud v3) and plain text (DC v2)
    raw_desc = fields.get("description") or ""
    if isinstance(raw_desc, dict):
        description = adf_to_text(raw_desc).strip()
    else:
        description = str(raw_desc).strip()

    lines = [
        f"# [{key}] {summary}",
        f"Type: {issue_type} | Status: {status} | Priority: {priority}",
        f"Assignee: {assignee} | Reporter: {reporter}",
        f"Created: {created} | Updated: {updated}",
    ]
    if labels:
        lines.append(f"Labels: {labels}")
    if components:
        lines.append(f"Components: {components}")
    if fix_versions:
        lines.append(f"Fix Versions: {fix_versions}")

    if description:
        lines.append(f"\n## Description\n{description}")

    # Linked issues
    links = fields.get("issuelinks") or []
    if links:
        link_lines = []
        for lnk in links:
            if "outwardIssue" in lnk:
                rel = lnk.get("type", {}).get("outward", "relates to")
                linked_key = lnk["outwardIssue"].get("key", "?")
                linked_summary = lnk["outwardIssue"].get("fields", {}).get("summary", "")
                link_lines.append(f"  - {rel}: {linked_key} — {linked_summary}")
            elif "inwardIssue" in lnk:
                rel = lnk.get("type", {}).get("inward", "is related to")
                linked_key = lnk["inwardIssue"].get("key", "?")
                linked_summary = lnk["inwardIssue"].get("fields", {}).get("summary", "")
                link_lines.append(f"  - {rel}: {linked_key} — {linked_summary}")
        if link_lines:
            lines.append("\n## Linked Issues\n" + "\n".join(link_lines))

    return "\n".join(lines)


def register_tools(mcp, client: JiraClient, config: AtlassianConfig) -> None:
    """Register all Jira tools onto the FastMCP instance."""

    @mcp.tool()
    async def jira_get_issue(issue_key: str) -> str:
        """Retrieve full details of a Jira issue by its key.

        Args:
            issue_key: The issue key (e.g. 'PROJ-123', 'BUG-42').
        """
        try:
            issue = await client.get_issue(
                issue_key,
                expand="names,renderedFields,transitions,issuelinks",
            )
        except AtlassianError as e:
            return _error(e)

        return truncate(_format_issue_detail(issue), config.max_content_length)

    @mcp.tool()
    async def jira_search_issues(
        jql: str,
        limit: int = 10,
        fields: str = "summary,status,assignee,priority,updated,issuetype",
    ) -> str:
        """Search Jira issues using JQL (Jira Query Language).

        Args:
            jql: A JQL query string. Examples:
                 - 'project = PROJ AND status = "In Progress"'
                 - 'assignee = currentUser() AND status != Done ORDER BY updated DESC'
                 - 'priority = High AND created >= -7d'
                 - 'text ~ "login error" AND project IN (APP, API)'
                 - 'sprint in openSprints() AND assignee = jsmith'
                 - 'labels = "backend" AND fixVersion = "2.0"'
            limit: Maximum results to return (1–50, default 10).
            fields: Comma-separated fields to include in results.
        """
        try:
            data = await client.search_issues(
                jql=jql,
                limit=min(max(limit, 1), 50),
                fields=fields,
            )
        except AtlassianError as e:
            return _error(e)

        issues = data.get("issues", [])
        if not issues:
            return "No issues found for that JQL query. Try broadening the search or checking the project key."

        total = data.get("total", len(issues))
        lines = [f"Found {total} issue(s) — showing {len(issues)}:\n"]
        for i, issue in enumerate(issues, 1):
            lines.append(f"{i}. {_format_issue_summary(issue)}")

        return "\n\n".join(lines)

    @mcp.tool()
    async def jira_get_my_issues(limit: int = 10) -> str:
        """List Jira issues currently assigned to you, ordered by most recently updated.

        Args:
            limit: Maximum issues to return (default 10).
        """
        jql = "assignee = currentUser() AND status != Done ORDER BY updated DESC"
        try:
            data = await client.search_issues(jql=jql, limit=min(limit, 50))
        except AtlassianError as e:
            return _error(e)

        issues = data.get("issues", [])
        if not issues:
            return "No open issues assigned to you."

        lines = [f"## Your Open Issues ({len(issues)})\n"]
        for i, issue in enumerate(issues, 1):
            lines.append(f"{i}. {_format_issue_summary(issue)}")

        return "\n\n".join(lines)

    @mcp.tool()
    async def jira_create_issue(
        project_key: str,
        summary: str,
        issue_type: str = "Task",
        description: str = "",
        priority: str = "",
        labels: str = "",
        assignee_id: str = "",
    ) -> str:
        """Create a new Jira issue.

        Args:
            project_key: The project key (e.g. 'PROJ', 'BUG').
            summary: Short title for the issue.
            issue_type: Issue type name (e.g. 'Task', 'Bug', 'Story', 'Epic').
            description: Detailed description (plain text).
            priority: Priority name (e.g. 'High', 'Medium', 'Low').
            labels: Comma-separated list of labels (e.g. 'backend,api').
            assignee_id: Assignee account ID (Cloud) or username (Data Center).
        """
        label_list = [l.strip() for l in labels.split(",") if l.strip()] if labels else None
        try:
            result = await client.create_issue(
                project_key=project_key,
                summary=summary,
                issue_type=issue_type,
                description=description,
                priority=priority,
                labels=label_list,
                assignee_id=assignee_id,
            )
        except AtlassianError as e:
            return _error(e)

        issue_key = result.get("key", "?")
        issue_id = result.get("id", "?")
        return f"Issue created: **{issue_key}** (ID: {issue_id})\nSummary: {summary}"

    @mcp.tool()
    async def jira_update_issue(
        issue_key: str,
        summary: str = "",
        description: str = "",
        priority: str = "",
        labels: str = "",
    ) -> str:
        """Update fields on an existing Jira issue.

        Only the fields you provide will be changed. Pass an empty string to skip a field.

        Args:
            issue_key: The issue key (e.g. 'PROJ-123').
            summary: New summary/title (leave empty to keep unchanged).
            description: New description text (leave empty to keep unchanged).
            priority: New priority name (e.g. 'High', 'Medium', 'Low').
            labels: Comma-separated labels. Replaces all existing labels if provided.
        """
        if not any([summary, description, priority, labels]):
            return "No fields to update — provide at least one of: summary, description, priority, labels."

        label_list = [l.strip() for l in labels.split(",") if l.strip()] if labels else None
        try:
            await client.update_issue(
                issue_key=issue_key,
                summary=summary,
                description=description,
                priority=priority,
                labels=label_list,
            )
        except AtlassianError as e:
            return _error(e)

        updated = []
        if summary:
            updated.append(f"summary → '{summary}'")
        if description:
            updated.append("description updated")
        if priority:
            updated.append(f"priority → '{priority}'")
        if label_list is not None:
            updated.append(f"labels → {label_list}")
        return f"Updated {issue_key}: {', '.join(updated)}"

    @mcp.tool()
    async def jira_get_transitions(issue_key: str) -> str:
        """List the valid status transitions for a Jira issue.

        Use this before calling jira_transition_issue to get the correct transition ID.

        Args:
            issue_key: The issue key (e.g. 'PROJ-123').
        """
        try:
            data = await client.get_transitions(issue_key)
        except AtlassianError as e:
            return _error(e)

        transitions = data.get("transitions", [])
        if not transitions:
            return f"No transitions available for {issue_key}."

        lines = [f"## Available Transitions for {issue_key}\n"]
        for t in transitions:
            tid = t.get("id", "?")
            name = t.get("name", "?")
            to_status = t.get("to", {}).get("name", "?")
            lines.append(f"- ID: `{tid}` — **{name}** → {to_status}")

        lines.append(
            "\nUse jira_transition_issue(issue_key, transition_id) "
            "with one of the IDs above."
        )
        return "\n".join(lines)

    @mcp.tool()
    async def jira_transition_issue(issue_key: str, transition_id: str) -> str:
        """Move a Jira issue to a new status using a transition.

        First call jira_get_transitions(issue_key) to get the valid transition IDs.

        Args:
            issue_key: The issue key (e.g. 'PROJ-123').
            transition_id: The transition ID from jira_get_transitions.
        """
        try:
            await client.transition_issue(issue_key, transition_id)
        except AtlassianError as e:
            return _error(e)

        return f"Successfully transitioned {issue_key} using transition {transition_id}."

    @mcp.tool()
    async def jira_add_comment(issue_key: str, body: str) -> str:
        """Add a comment to a Jira issue.

        Args:
            issue_key: The issue key (e.g. 'PROJ-123').
            body: The comment text (plain text).
        """
        try:
            result = await client.add_comment(issue_key, body)
        except AtlassianError as e:
            return _error(e)

        comment_id = result.get("id", "?")
        author = (result.get("author") or result.get("updateAuthor") or {}).get(
            "displayName", "You"
        )
        return f"Comment added to {issue_key} by {author} (comment ID: {comment_id})."

    @mcp.tool()
    async def jira_get_comments(issue_key: str, limit: int = 25) -> str:
        """Get comments on a Jira issue.

        Args:
            issue_key: The issue key (e.g. 'PROJ-123').
            limit: Maximum comments to return (default 25).
        """
        try:
            data = await client.get_comments(issue_key, limit=limit)
        except AtlassianError as e:
            return _error(e)

        comments = data.get("comments", [])
        if not comments:
            return f"No comments on {issue_key}."

        total = data.get("total", len(comments))
        lines = [f"## Comments on {issue_key} ({total} total)\n"]
        for c in comments:
            author = (c.get("author") or {}).get("displayName", "Unknown")
            created = (c.get("created") or "")[:10]
            raw_body = c.get("body", "")
            if isinstance(raw_body, dict):
                body = adf_to_text(raw_body).strip()
            else:
                body = str(raw_body).strip()

            entry = f"**{author}** — {created}\n{body}"
            lines.append(entry)

        return "\n\n---\n\n".join(lines)

    @mcp.tool()
    async def jira_list_projects(limit: int = 50) -> str:
        """List Jira projects accessible to the authenticated user.

        Args:
            limit: Maximum projects to return (default 50).
        """
        try:
            data = await client.list_projects(limit=min(limit, 100))
        except AtlassianError as e:
            return _error(e)

        projects = data.get("values", [])
        if not projects:
            return "No projects found."

        total = data.get("total", len(projects))
        lines = [f"## Jira Projects ({total} total — showing {len(projects)})\n"]
        for p in projects:
            key = p.get("key", "?")
            name = p.get("name", "?")
            ptype = p.get("projectTypeKey", "?")
            lead = (p.get("lead") or {}).get("displayName", "")
            entry = f"- **{key}** — {name} ({ptype})"
            if lead:
                entry += f" | Lead: {lead}"
            lines.append(entry)

        return "\n".join(lines)

    @mcp.tool()
    async def jira_get_project(project_key: str) -> str:
        """Get details and issue types for a Jira project.

        Args:
            project_key: The project key (e.g. 'PROJ').
        """
        try:
            project = await client.get_project(project_key)
        except AtlassianError as e:
            return _error(e)

        name = project.get("name", "?")
        description = (project.get("description") or "").strip()
        lead = (project.get("lead") or {}).get("displayName", "Unknown")
        ptype = project.get("projectTypeKey", "?")

        issue_types = project.get("issueTypes", [])
        type_names = [it.get("name", "") for it in issue_types if it.get("name")]

        lines = [
            f"## Project: {name} ({project_key})",
            f"Type: {ptype} | Lead: {lead}",
        ]
        if description:
            lines.append(f"Description: {description}")
        if type_names:
            lines.append(f"\nIssue Types: {', '.join(type_names)}")

        return "\n".join(lines)

    @mcp.tool()
    async def jira_assign_issue(issue_key: str, assignee_id: str) -> str:
        """Assign a Jira issue to a user.

        Args:
            issue_key: The issue key (e.g. 'PROJ-123').
            assignee_id: Account ID (Cloud) or username (Data Center).
                         Use empty string to unassign.
        """
        try:
            await client.assign_issue(issue_key, assignee_id)
        except AtlassianError as e:
            return _error(e)

        if assignee_id:
            return f"Assigned {issue_key} to '{assignee_id}'."
        return f"Unassigned {issue_key}."
