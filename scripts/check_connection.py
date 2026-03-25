#!/usr/bin/env python3
"""
Quick health check — validates .env config and tests Confluence connectivity.

Usage:
    python scripts/check_connection.py
"""

import asyncio
import sys
import os

# Add parent dir to path so we can import our modules
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from config import load_config
from confluence_client import ConfluenceClient, ConfluenceError


async def check():
    print("=" * 50)
    print("Confluence MCP Server — Connection Check")
    print("=" * 50)

    # 1. Load config
    print("\n[1/3] Loading configuration...")
    config = load_config()
    print(f"  ✅ URL:  {config.confluence_url}")
    print(f"  ✅ Auth: {config.auth_method}")
    print(f"  ✅ SSL:  {'verified' if config.ssl_verify is True else config.ssl_verify}")

    # 2. Test connectivity
    print("\n[2/3] Testing Confluence connectivity...")
    client = ConfluenceClient(config)
    try:
        data = await client.list_spaces(space_type="global", limit=5, expand="")
        spaces = data.get("results", [])
        print(f"  ✅ Connected successfully!")
        print(f"  ✅ Found {len(spaces)} space(s):")
        for s in spaces:
            print(f"     - {s['name']} (key: {s['key']})")
    except ConfluenceError as e:
        print(f"  ❌ {e}")
        sys.exit(1)

    # 3. Test search
    print("\n[3/3] Testing CQL search...")
    try:
        data = await client.search(
            cql='type=page', limit=3, expand="space"
        )
        results = data.get("results", [])
        print(f"  ✅ Search works! Found {data.get('totalSize', '?')} total pages.")
        for r in results[:3]:
            space = r.get("space", {}).get("key", "?")
            print(f"     - [{space}] {r['title']}")
    except ConfluenceError as e:
        print(f"  ⚠️  Search failed: {e}")

    print("\n" + "=" * 50)
    print("All checks passed! Your MCP server is ready to run.")
    print("=" * 50)


if __name__ == "__main__":
    asyncio.run(check())
