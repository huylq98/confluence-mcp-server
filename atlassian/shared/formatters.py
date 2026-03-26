"""
Shared text formatting utilities for all Atlassian service modules.
"""

import re


def strip_html(html: str) -> str:
    """Lightweight HTML tag stripper for readable output."""
    text = re.sub(r"<br\s*/?>", "\n", html)
    text = re.sub(r"</(p|div|h[1-6]|li|tr)>", "\n", text)
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"&nbsp;", " ", text)
    text = re.sub(r"&amp;", "&", text)
    text = re.sub(r"&lt;", "<", text)
    text = re.sub(r"&gt;", ">", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


def truncate(text: str, max_len: int = 50000) -> str:
    """Truncate text to max_len characters with an indicator."""
    if len(text) <= max_len:
        return text
    return text[:max_len] + f"\n\n... [truncated — {len(text)} chars total]"


def adf_to_text(adf: dict) -> str:
    """Convert Atlassian Document Format (ADF) to plain text (best-effort)."""
    if not isinstance(adf, dict):
        return str(adf)

    node_type = adf.get("type", "")
    content = adf.get("content", [])
    text_val = adf.get("text", "")

    if node_type == "text":
        return text_val

    parts = [adf_to_text(child) for child in content]

    if node_type in ("paragraph", "heading"):
        return " ".join(p for p in parts if p) + "\n"
    if node_type == "bulletList":
        return "\n".join(f"• {p.strip()}" for p in parts if p.strip())
    if node_type == "orderedList":
        return "\n".join(f"{i + 1}. {p.strip()}" for i, p in enumerate(parts) if p.strip())
    if node_type == "listItem":
        return " ".join(p for p in parts if p)
    if node_type == "hardBreak":
        return "\n"
    if node_type == "codeBlock":
        code = "".join(parts)
        return f"```\n{code}\n```"
    if node_type == "blockquote":
        return "\n".join(f"> {p}" for p in parts if p)

    return " ".join(p for p in parts if p)
