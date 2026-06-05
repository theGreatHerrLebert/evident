"""Two-tier error model, mirroring typed-trust's ``handlers.rs``.

- **PROTOCOL** errors become JSON-RPC ``error`` responses (bad input
  shape, unauthorized path, unknown method, internal defect). The
  client cannot recover by retrying with the same arguments.
- **DATA** errors become ``result`` payloads with ``isError: true`` —
  the model CAN react (a claim didn't match, docker is down, the API
  failed, a PDF was unusable).
"""

from __future__ import annotations

from enum import Enum


class ToolErrorTier(Enum):
    PROTOCOL = "protocol"
    DATA = "data"


# JSON-RPC error codes (mirrors typed-trust mod.rs:10-22)
INVALID_PARAMS = -32602
METHOD_NOT_FOUND = -32601
UNAUTHORIZED = -32001
INTERNAL = -32603


class ToolError(Exception):
    """Carries the tier + JSON-RPC code so the dispatch chokepoint can
    route it to the right MCP response shape."""

    def __init__(self, tier: ToolErrorTier, code: int, message: str):
        super().__init__(message)
        self.tier = tier
        self.code = code
        self.message = message

    # --- tier-1 (PROTOCOL) ---
    @classmethod
    def invalid_params(cls, message: str) -> "ToolError":
        return cls(ToolErrorTier.PROTOCOL, INVALID_PARAMS, message)

    @classmethod
    def unauthorized(cls, message: str) -> "ToolError":
        return cls(ToolErrorTier.PROTOCOL, UNAUTHORIZED, message)

    @classmethod
    def method_not_found(cls, message: str) -> "ToolError":
        return cls(ToolErrorTier.PROTOCOL, METHOD_NOT_FOUND, message)

    @classmethod
    def internal(cls, message: str) -> "ToolError":
        return cls(ToolErrorTier.PROTOCOL, INTERNAL, message)

    # --- tier-2 (DATA) ---
    @classmethod
    def data(cls, message: str) -> "ToolError":
        return cls(ToolErrorTier.DATA, 0, message)
