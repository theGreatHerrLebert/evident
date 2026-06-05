"""``evident-agent-mcp`` — stdio MCP server exposing EVIDENT exec tools.

Sibling to the Rust ``typed-trust-mcp`` read server. Wraps the existing
``evident-agent`` CLI logic (replay, extract) as MCP tools so external
agent runtimes (Claude Code, Codex) can drive the EVIDENT workflow.

The ``main`` entry point is wired in ``pyproject.toml`` as
``evident-agent-mcp``.
"""

from __future__ import annotations

from typing import Optional, Sequence

__all__ = ["main"]


def main(argv: Optional[Sequence[str]] = None) -> int:
    """Console-script entry point. Imported lazily so ``policy`` and
    ``errors`` (and their tests) don't require the ``mcp`` SDK."""
    from .server import main as _main

    return _main(argv)
