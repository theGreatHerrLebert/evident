"""``python -m evident_agent.mcp`` → start the stdio exec-MCP server."""

from __future__ import annotations

from .server import main

if __name__ == "__main__":
    raise SystemExit(main())
