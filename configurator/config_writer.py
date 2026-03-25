"""Read and write Claude Desktop configuration for the Confluence MCP server."""

import json
import os
import shutil
import sys
from pathlib import Path


def get_config_path() -> Path:
    """Get the Claude Desktop config file path for this platform."""
    if sys.platform == "win32":
        return Path(os.environ.get("APPDATA", "")) / "Claude" / "claude_desktop_config.json"
    return Path.home() / ".config" / "Claude" / "claude_desktop_config.json"


def get_exe_path() -> str:
    """Get the command path for Claude Desktop config.

    - Frozen (PyInstaller): path to the exe itself
    - Dev mode: python executable + main.py
    """
    if getattr(sys, "frozen", False):
        return str(sys.executable).replace("\\", "/")
    return str(sys.executable).replace("\\", "/")


def get_exe_args() -> list[str]:
    """Get the args list for Claude Desktop config.

    - Frozen: ["--serve"]
    - Dev mode: ["path/to/main.py", "--serve"]
    """
    if getattr(sys, "frozen", False):
        return ["--serve"]
    main_py = Path(__file__).resolve().parent.parent / "main.py"
    return [str(main_py).replace("\\", "/"), "--serve"]


def load_existing_config() -> dict:
    """Load existing Claude Desktop config and extract confluence entry if present.

    Returns dict with:
        - config_exists: bool
        - confluence_configured: bool
        - url, username, password, token, ssl_verify: pre-fill values
    """
    config_path = get_config_path()
    result = {
        "config_exists": False,
        "confluence_configured": False,
        "url": "",
        "username": "",
        "password": "",
        "token": "",
        "ssl_verify": True,
    }

    if not config_path.is_file():
        return result

    try:
        with open(config_path, "r", encoding="utf-8") as f:
            data = json.load(f)
        result["config_exists"] = True
    except (json.JSONDecodeError, OSError):
        return result

    confluence = data.get("mcpServers", {}).get("confluence")
    if not confluence:
        return result

    result["confluence_configured"] = True
    env = confluence.get("env", {})
    result["url"] = env.get("CONFLUENCE_URL", "")
    result["username"] = env.get("CONFLUENCE_USERNAME", "")
    result["password"] = env.get("CONFLUENCE_PASSWORD", "")
    result["token"] = env.get("CONFLUENCE_TOKEN", "")
    result["ssl_verify"] = env.get("CONFLUENCE_SSL_VERIFY", "true").lower() != "false"
    return result


def save_config(url: str, username: str, password: str, token: str,
                ssl_verify: bool) -> dict:
    """Write the Confluence MCP server entry into Claude Desktop config.

    Uses read-modify-write to preserve other MCP server entries.
    Creates a backup of the existing file before writing.

    Returns dict with success/message/config_path.
    """
    config_path = get_config_path()
    exe_path = get_exe_path()
    exe_args = get_exe_args()

    # Read existing config
    data = {"mcpServers": {}}
    if config_path.is_file():
        try:
            with open(config_path, "r", encoding="utf-8") as f:
                data = json.load(f)
            # Backup before modifying
            backup_path = config_path.with_suffix(".json.backup")
            shutil.copy2(config_path, backup_path)
        except (json.JSONDecodeError, OSError):
            data = {"mcpServers": {}}

    if "mcpServers" not in data:
        data["mcpServers"] = {}

    # Build env vars
    env = {
        "CONFLUENCE_URL": url.rstrip("/"),
        "MCP_TRANSPORT": "stdio",
    }
    if token:
        env["CONFLUENCE_TOKEN"] = token
    else:
        env["CONFLUENCE_USERNAME"] = username
        env["CONFLUENCE_PASSWORD"] = password
    if not ssl_verify:
        env["CONFLUENCE_SSL_VERIFY"] = "false"

    # Build server entry — points to this exe with --serve flag
    entry = {
        "command": exe_path,
        "args": exe_args,
        "env": env,
    }

    data["mcpServers"]["confluence"] = entry

    # Write config
    try:
        config_path.parent.mkdir(parents=True, exist_ok=True)
        with open(config_path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)
    except OSError as e:
        return {
            "success": False,
            "message": f"Cannot write config file: {e}. Try running as Administrator.",
        }

    return {
        "success": True,
        "message": "Configuration saved! Restart Claude Desktop to activate.",
        "config_path": str(config_path),
    }
