"""Unit tests for the drive launcher's pure builders (no spawn)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from evident_agent import drive


def test_compose_prompt_injects_runtime_context() -> None:
    out = drive.compose_prompt("BODY", Path("/corpus"), allow_docker=False, allow_extract=True)
    assert "EVIDENT corpus root" in out
    assert "/corpus" in out
    assert "docker=off" in out
    assert "extract=on" in out
    assert out.rstrip().endswith("BODY")


def test_exec_allow_args() -> None:
    base = drive._exec_allow_args(Path("/r"), False, False)
    assert base == ["--allow-root", "/r"]
    full = drive._exec_allow_args(Path("/r"), True, True)
    assert "--allow-docker" in full and "--allow-extract" in full


def test_prepare_claude_writes_config_and_argv(tmp_path: Path) -> None:
    argv = drive.prepare_claude(
        tmp_path,
        tt_cmd="/abs/typed-trust-mcp",
        tt_args=["--allow-manifest", "/r"],
        exec_cmd="/abs/python",
        exec_args=["-m", "evident_agent.mcp", "--allow-root", "/r"],
        prompt_text="PROMPT",
    )
    assert argv[0] == "claude"
    assert "--strict-mcp-config" in argv
    assert "--mcp-config" in argv
    assert "--append-system-prompt-file" in argv
    cfg = json.loads((tmp_path / "mcp.json").read_text())
    assert set(cfg["mcpServers"]) == {"typed-trust", "evident-exec"}
    assert cfg["mcpServers"]["typed-trust"]["command"] == "/abs/typed-trust-mcp"
    assert (tmp_path / "EVIDENT_DRIVER.md").read_text() == "PROMPT"


def test_prepare_codex_writes_agents_and_invocation_local_argv(tmp_path: Path) -> None:
    argv = drive.prepare_codex(
        tmp_path,
        tt_cmd="/abs/typed-trust-mcp",
        tt_args=["--allow-manifest", "/r"],
        exec_cmd="/abs/python",
        exec_args=["-m", "evident_agent.mcp", "--allow-root", "/r"],
        prompt_text="PROMPT",
    )
    assert argv[0] == "codex"
    # invocation-local: no `codex mcp add`, ephemeral cwd, user config ignored
    assert "--ignore-user-config" in argv
    assert "-C" in argv and str(tmp_path) in argv
    assert "mcp add" not in " ".join(argv)
    # mcp servers configured via -c overrides, TOML-parseable values
    joined = " ".join(argv)
    assert 'mcp_servers.typed_trust.command="/abs/typed-trust-mcp"' in joined
    assert 'mcp_servers.evident_exec.args=["-m", "evident_agent.mcp", "--allow-root", "/r"]' in joined
    assert (tmp_path / "AGENTS.md").read_text() == "PROMPT"


def test_resolve_servers_uses_python_module_for_exec(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setattr(drive, "find_typed_trust_mcp", lambda: "/abs/typed-trust-mcp")
    tt, tt_args, exec_cmd, exec_args = drive.resolve_servers(tmp_path)
    assert tt == "/abs/typed-trust-mcp"
    assert tt_args == ["--allow-manifest", str(tmp_path)]
    assert exec_args == ["-m", "evident_agent.mcp"]


def test_resolve_servers_errors_without_binary(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setattr(drive, "find_typed_trust_mcp", lambda: None)
    with pytest.raises(drive.DriveError):
        drive.resolve_servers(tmp_path)


def test_run_drive_rejects_unknown_model(tmp_path: Path) -> None:
    with pytest.raises(drive.DriveError):
        drive.run_drive(model="gpt", root=tmp_path)
