"""Dispatch-level unit tests for the exec-MCP tools (no subprocess):
error-tier mapping for protocol vs data failures."""

from __future__ import annotations

from pathlib import Path

import pytest

import evident_agent.extract.cli as extract_cli
from evident_agent.mcp.errors import ToolError, ToolErrorTier
from evident_agent.mcp.policy import AllowListPathPolicy
from evident_agent.mcp.tools import ServerState, dispatch_sync


def _state(tmp_path: Path, **kw) -> ServerState:
    pol = AllowListPathPolicy()
    pol.allow(tmp_path)
    return ServerState(policy=pol, **kw)


def test_unknown_tool_is_protocol_error(tmp_path: Path) -> None:
    with pytest.raises(ToolError) as ei:
        dispatch_sync(_state(tmp_path), "nope", {})
    assert ei.value.tier is ToolErrorTier.PROTOCOL
    assert ei.value.code == -32601


def test_missing_required_arg_is_invalid_params(tmp_path: Path) -> None:
    with pytest.raises(ToolError) as ei:
        dispatch_sync(_state(tmp_path), "extract_metadata", {"output_dir": str(tmp_path)})
    assert ei.value.tier is ToolErrorTier.PROTOCOL
    assert ei.value.code == -32602


def test_unauthorized_path_is_unauthorized(tmp_path: Path) -> None:
    other = tmp_path.parent  # outside the allowed root
    with pytest.raises(ToolError) as ei:
        dispatch_sync(
            _state(tmp_path),
            "extract_metadata",
            {"repo": str(other), "output_dir": str(tmp_path / "o")},
        )
    assert ei.value.tier is ToolErrorTier.PROTOCOL
    assert ei.value.code == -32001


def test_extract_repo_api_failure_is_tier2(tmp_path: Path, monkeypatch) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "README.md").write_text("# x")

    def boom(**_kw):
        raise RuntimeError("Anthropic SDK not installed")

    monkeypatch.setattr(extract_cli, "run_extract_repo", boom)
    with pytest.raises(ToolError) as ei:
        dispatch_sync(
            _state(tmp_path, allow_extract=True),
            "extract_repo",
            {"repo_path": str(repo), "output_dir": str(tmp_path / "out")},
        )
    assert ei.value.tier is ToolErrorTier.DATA


def test_extract_transport_error_is_tier2(tmp_path: Path, monkeypatch) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "README.md").write_text("# x")

    def boom(**_kw):
        raise extract_cli.ExtractTransportError("no tool_use block")

    monkeypatch.setattr(extract_cli, "run_extract_repo", boom)
    with pytest.raises(ToolError) as ei:
        dispatch_sync(
            _state(tmp_path, allow_extract=True),
            "extract_repo",
            {"repo_path": str(repo), "output_dir": str(tmp_path / "out")},
        )
    assert ei.value.tier is ToolErrorTier.DATA
