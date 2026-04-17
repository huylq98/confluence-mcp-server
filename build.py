"""Build script — creates a single-file executable using PyInstaller."""

import shutil
import subprocess
import sys
from pathlib import Path

UPX_DIR = Path(__file__).parent / "tools" / "upx-4.2.4-win64"


def build():
    cmd = [
        sys.executable, "-m", "PyInstaller",
        "--onefile",
        "--windowed",
        "--name", "ConfluenceMCPSetup",
        # Bundle frontend assets
        "--add-data", "configurator/assets;configurator/assets",
        # MCP and its dependencies
        "--collect-all", "mcp",
        "--collect-all", "pydantic",
        "--collect-all", "pydantic_core",
        # HTTP stack
        "--hidden-import", "httpx",
        "--hidden-import", "httpcore",
        "--hidden-import", "h11",
        "--hidden-import", "anyio",
        "--hidden-import", "anyio._backends._asyncio",
        "--hidden-import", "sniffio",
        "--hidden-import", "certifi",
        # pywebview
        "--hidden-import", "webview",
        # dotenv
        "--hidden-import", "dotenv",
        # SSL support
        "--hidden-import", "ssl",
        # Entry point
        "main.py",
    ]

    upx_exe = UPX_DIR / "upx.exe"
    if upx_exe.exists():
        cmd.extend(["--upx-dir", str(UPX_DIR)])
        print(f"UPX found at {upx_exe} — compression enabled.")
    elif shutil.which("upx"):
        print("UPX found on PATH — compression enabled.")
    else:
        print("UPX not found — building without compression.")
        print("  Download: https://github.com/upx/upx/releases (extract to tools/upx-4.2.4-win64/)")

    print("Building ConfluenceMCPSetup.exe ...")
    print(f"Command: {' '.join(cmd)}")
    result = subprocess.run(cmd)
    if result.returncode == 0:
        print("\nBuild successful! Output: dist/ConfluenceMCPSetup.exe")
    else:
        print(f"\nBuild failed with exit code {result.returncode}")
        sys.exit(1)


if __name__ == "__main__":
    build()
