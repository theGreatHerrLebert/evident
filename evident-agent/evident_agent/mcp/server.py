"""``evident-agent-mcp`` stdio server entry point.

Mirrors typed-trust's ``bin/mcp.rs`` + ``mcp/mod.rs``: parse
``--allow-root`` (repeatable) plus the capability flags
``--allow-docker`` / ``--allow-extract`` (default off), build the
allow-list policy, and serve JSON-RPC over stdio.

**stdout purity:** this is a stdio transport, so all logging goes to
stderr; only MCP frames touch stdout.
"""

from __future__ import annotations

import json
import logging
import sys
from typing import Optional, Sequence

import anyio
import mcp.types as types
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.shared.exceptions import McpError

from .errors import INTERNAL, ToolError, ToolErrorTier
from .policy import AllowListPathPolicy, PolicyDenied
from .tools import ServerState, dispatch_sync, tool_definitions

logger = logging.getLogger("evident_agent.mcp")

_USAGE = (
    "usage: evident-agent-mcp [--allow-root <path>] ... "
    "[--allow-docker] [--allow-extract]\n\n"
    "Exec MCP server for the EVIDENT agent. Reads JSON-RPC 2.0 frames "
    "from stdin, writes responses to stdout.\n"
    "  --allow-root <dir-or-file>  (repeatable) restrict which paths tools may touch\n"
    "  --allow-docker              permit replay to actually run docker (default off → dry-run)\n"
    "  --allow-extract             permit extract_* to call the Anthropic API (default off → dry-run)\n"
)


def _build_state(argv: Sequence[str]) -> ServerState:
    policy = AllowListPathPolicy()
    allow_docker = False
    allow_extract = False
    it = iter(argv)
    for arg in it:
        if arg == "--allow-root":
            try:
                p = next(it)
            except StopIteration:
                raise SystemExit("error: --allow-root requires a path")
            try:
                policy.allow(p)
            except PolicyDenied as exc:
                raise SystemExit(f"error: cannot register {p}: {exc.reason}")
        elif arg == "--allow-docker":
            allow_docker = True
        elif arg == "--allow-extract":
            allow_extract = True
        elif arg in ("-h", "--help"):
            sys.stderr.write(_USAGE)
            raise SystemExit(0)
        else:
            raise SystemExit(f"error: unknown argument {arg!r}\n\n{_USAGE}")
    return ServerState(policy=policy, allow_docker=allow_docker, allow_extract=allow_extract)


def _make_server(state: ServerState) -> Server:
    server = Server("evident-agent-mcp")

    @server.list_tools()
    async def _list_tools() -> list[types.Tool]:
        return [types.Tool(**d) for d in tool_definitions()]

    # Register the CallToolRequest handler DIRECTLY (not via @server.call_tool),
    # because the decorator swallows every exception into an isError result —
    # we need PROTOCOL ToolErrors to surface as real JSON-RPC errors (the outer
    # _handle_request turns a raised McpError into an error response).
    async def _call_tool(req: types.CallToolRequest) -> types.ServerResult:
        name = req.params.name
        arguments = req.params.arguments or {}
        try:
            result = await anyio.to_thread.run_sync(
                lambda: dispatch_sync(state, name, arguments)
            )
        except ToolError as exc:
            if exc.tier is ToolErrorTier.PROTOCOL:
                raise McpError(types.ErrorData(code=exc.code, message=exc.message))
            return types.ServerResult(
                types.CallToolResult(
                    content=[types.TextContent(type="text", text=exc.message)],
                    isError=True,
                )
            )
        except Exception:  # unexpected defect → sanitized protocol error
            logger.exception("unhandled error in tool %s", name)
            raise McpError(
                types.ErrorData(code=INTERNAL, message=f"internal error in tool {name!r}")
            )
        return types.ServerResult(
            types.CallToolResult(
                content=[types.TextContent(type="text", text=json.dumps(result))],
                isError=False,
            )
        )

    server.request_handlers[types.CallToolRequest] = _call_tool
    return server


async def _serve(server: Server) -> None:
    async with stdio_server() as (read_stream, write_stream):
        await server.run(
            read_stream,
            write_stream,
            server.create_initialization_options(),
        )


def main(argv: Optional[Sequence[str]] = None) -> int:
    # All logging to stderr — stdout is the MCP frame channel.
    logging.basicConfig(level=logging.INFO, stream=sys.stderr)
    if argv is None:
        argv = sys.argv[1:]
    state = _build_state(list(argv))
    server = _make_server(state)
    anyio.run(_serve, server)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
