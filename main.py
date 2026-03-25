"""
Confluence MCP Server — Single entry point.

Double-click (no args)  → GUI configurator wizard
--serve flag            → MCP server over stdio (called by Claude Desktop)
"""

import sys


def main():
    if "--serve" in sys.argv:
        from server import main as server_main
        server_main()
    else:
        from configurator.app import main as gui_main
        gui_main()


if __name__ == "__main__":
    main()
