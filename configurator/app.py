"""
Confluence MCP Server Configurator — Desktop GUI.

A simple wizard that helps non-technical users:
  1. Enter Confluence credentials
  2. Test the connection
  3. Save Claude Desktop configuration

Usage:
    python main.py          (GUI mode)
    python main.py --serve  (MCP server mode)
"""

import os
import subprocess
import sys
from pathlib import Path

import webview

from configurator.config_writer import (
    get_config_path,
    get_exe_path,
    get_exe_args,
    load_existing_config,
    save_config,
)
from configurator.connection_tester import test_confluence_connection


class Api:
    """Bridge API exposed to the JavaScript frontend via pywebview."""

    def test_connection(self, url, username, password, token, ssl_verify):
        """Test connectivity to the Confluence server."""
        return test_confluence_connection(url, username, password, token, ssl_verify)

    def save_config(self, url, username, password, token, ssl_verify):
        """Write MCP server entry into Claude Desktop config."""
        return save_config(url, username, password, token, ssl_verify)

    def load_existing_config(self):
        """Load existing config for pre-filling the form."""
        return load_existing_config()

    def get_exe_path(self):
        """Return the exe path and args for config preview."""
        return {
            "command": get_exe_path(),
            "args": get_exe_args(),
        }

    def open_claude_desktop(self):
        """Attempt to launch Claude Desktop."""
        if sys.platform == "win32":
            # Find Claude Desktop's AppUserModelId dynamically (works for any MSIX install)
            try:
                result = subprocess.run(
                    ["powershell", "-Command",
                     "(Get-StartApps | Where-Object { $_.Name -eq 'Claude' }).AppID"],
                    capture_output=True, text=True, timeout=10,
                )
                app_id = result.stdout.strip()
                if app_id:
                    subprocess.Popen(["explorer.exe", f"shell:AppsFolder\\{app_id}"])
                    return
            except (OSError, subprocess.TimeoutExpired):
                pass
            # Fallback: traditional exe install
            local_app = Path(os.environ.get("LOCALAPPDATA", "")) / "Programs" / "claude" / "Claude.exe"
            if local_app.is_file():
                subprocess.Popen([str(local_app)])
                return
        elif sys.platform == "darwin":
            subprocess.Popen(["open", "-a", "Claude"])


def _check_webview2():
    """On Windows, check if WebView2 runtime is available."""
    if sys.platform != "win32":
        return True
    # Check for msedgewebview2.exe in known locations
    search_dirs = [
        Path(os.environ.get("ProgramFiles(x86)", "")) / "Microsoft" / "EdgeWebView" / "Application",
        Path(os.environ.get("ProgramFiles(x86)", "")) / "Microsoft" / "Edge" / "Application",
        Path(os.environ.get("ProgramFiles", "")) / "Microsoft" / "EdgeWebView" / "Application",
        Path(os.environ.get("ProgramFiles", "")) / "Microsoft" / "Edge" / "Application",
    ]
    for d in search_dirs:
        if d.is_dir():
            # msedgewebview2.exe is inside a version-numbered subfolder
            for child in d.iterdir():
                if (child / "msedgewebview2.exe").is_file():
                    return True
    # Fallback: check registry (some installs only register here)
    try:
        import winreg
        for hive in (winreg.HKEY_LOCAL_MACHINE, winreg.HKEY_CURRENT_USER):
            for subkey in (
                r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}",
                r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BEF-535EB6BD9CFE}",
            ):
                try:
                    key = winreg.OpenKey(hive, subkey)
                    winreg.CloseKey(key)
                    return True
                except (FileNotFoundError, OSError):
                    continue
    except ImportError:
        pass
    return False


def _get_assets_dir() -> Path:
    """Get assets directory, handling both dev and frozen (PyInstaller) mode."""
    if getattr(sys, "frozen", False):
        return Path(sys._MEIPASS) / "configurator" / "assets"
    return Path(__file__).parent / "assets"


def main():
    # Check WebView2 on Windows
    if not _check_webview2():
        import ctypes
        ctypes.windll.user32.MessageBoxW(
            0,
            "Microsoft WebView2 Runtime is required but not installed.\n\n"
            "Please download it from:\n"
            "https://developer.microsoft.com/en-us/microsoft-edge/webview2/\n\n"
            "Install the 'Evergreen Bootstrapper' and try again.",
            "Confluence MCP Setup — Missing Component",
            0x30,  # MB_ICONWARNING
        )
        sys.exit(1)

    api = Api()
    assets_dir = _get_assets_dir()

    window = webview.create_window(
        "Confluence MCP Setup",
        url=str(assets_dir / "index.html"),
        js_api=api,
        width=600,
        height=720,
        resizable=False,
    )
    webview.start()


if __name__ == "__main__":
    main()
