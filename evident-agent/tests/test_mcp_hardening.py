"""Regressions for the exec-MCP hardening (Codex MCP review, 2026-10-06).

Each test reproduces one finding against the real server over stdio, or
against the function the server calls. Docker is replaced by a fake
``docker`` script on PATH that logs its argv and exits with a chosen code,
so no container runtime is needed.
"""

from __future__ import annotations

import json
import os
from pathlib import Path

import pytest

from evident_agent import replay as replay_mod
from test_mcp_loadbearing import McpProc, _result_payload, _write_measurement_manifest


# ---------------------------------------------------------------------
# #1 Lock file: a symlink at <sidecar>.lock must not truncate its target
# ---------------------------------------------------------------------
def test_sidecar_lock_refuses_symlink_and_keeps_target(tmp_path: Path) -> None:
    victim = tmp_path / "outside" / "victim.txt"
    victim.parent.mkdir()
    victim.write_text("precious")
    sidecar = tmp_path / "root" / "last_verified.json"
    sidecar.parent.mkdir()
    replay_mod.sidecar_path_lock(sidecar).symlink_to(victim)

    with pytest.raises(OSError):
        with replay_mod._sidecar_lock(sidecar):
            pass
    assert victim.read_text() == "precious"


def test_sidecar_lock_does_not_truncate_existing_lock(tmp_path: Path) -> None:
    sidecar = tmp_path / "last_verified.json"
    lock = replay_mod.sidecar_path_lock(sidecar)
    lock.write_text("keep")
    with replay_mod._sidecar_lock(sidecar):
        pass
    assert lock.read_text() == "keep"


def test_mcp_replay_lock_symlink_escape_denied(tmp_path: Path) -> None:
    root = tmp_path / "root"
    root.mkdir()
    victim = tmp_path / "victim.txt"
    victim.write_text("precious")
    manifest = _write_measurement_manifest(root)
    (root / "last_verified.json.lock").symlink_to(victim)

    proc = McpProc(["--allow-root", str(root)])
    try:
        proc.initialize()
        frame = proc.call(
            "replay",
            {"manifest_path": str(manifest), "claim": "claim-A", "no_execute": True},
        )
        # Refused (as a protocol error or a tool error), never written through.
        is_refused = "error" in frame or frame["result"]["isError"] is True
        assert is_refused, frame
    finally:
        proc.close()
    assert victim.read_text() == "precious"


# ---------------------------------------------------------------------
# #2 Manifest-derived source dirs must pass the allow-list
# ---------------------------------------------------------------------
def test_mcp_replay_manifest_source_outside_root_denied(tmp_path: Path) -> None:
    root = tmp_path / "root"
    root.mkdir()
    (tmp_path / "outside").mkdir()
    manifest = _write_measurement_manifest(root)
    manifest.write_text(manifest.read_text().replace("source: .", "source: ../outside"))

    proc = McpProc(["--allow-root", str(root)])
    try:
        proc.initialize()
        for mode in ({"dry_run": True}, {"no_execute": True}):
            frame = proc.call(
                "replay", {"manifest_path": str(manifest), "claim": "claim-A", **mode}
            )
            assert "error" in frame or frame["result"]["isError"] is True, (mode, frame)
            assert "outside" in json.dumps(frame), frame
    finally:
        proc.close()
    assert not (root / "last_verified.json").exists()


def test_mcp_replay_manifest_source_inside_root_allowed(tmp_path: Path) -> None:
    root = tmp_path / "root"
    (root / "src").mkdir(parents=True)
    manifest = _write_measurement_manifest(root)
    manifest.write_text(manifest.read_text().replace("source: .", "source: src"))
    proc = McpProc(["--allow-root", str(root)])
    try:
        proc.initialize()
        frame = proc.call(
            "replay", {"manifest_path": str(manifest), "claim": "claim-A", "dry_run": True}
        )
        assert frame["result"]["isError"] is False, frame
    finally:
        proc.close()


# ---------------------------------------------------------------------
# #3 Image references cannot inject docker options
# ---------------------------------------------------------------------
from evident_agent.docker import build_command, validate_image  # noqa: E402


@pytest.mark.parametrize(
    "ref",
    [
        "proteon-evident:latest",
        "img",
        "ghcr.io/org/tool:1.2.3",
        "localhost:5000/team/tool",
        "registry.example.org/a/b@sha256:" + "0" * 64,
    ],
)
def test_validate_image_accepts_references(ref: str) -> None:
    assert validate_image(ref) == ref


@pytest.mark.parametrize(
    "ref",
    ["--privileged", "--volume=/:/host", "-v", "img --privileged", "", "Img Upper", "img;rm -rf /"],
)
def test_validate_image_rejects_option_like(ref: str) -> None:
    with pytest.raises(ValueError):
        validate_image(ref)
    with pytest.raises(ValueError):
        build_command(ref, "claim-A", Path("/tmp"))


def test_build_command_drops_privilege_escalation() -> None:
    argv = build_command("img", "claim-A", Path("/tmp"))
    assert argv[argv.index("--security-opt") + 1] == "no-new-privileges"
    assert "--pids-limit" in argv
    assert argv.index("img") == len(argv) - 3  # image right before `replay <claim>`


# ---------------------------------------------------------------------
# #4 The client may only pick operator-allowed images
# ---------------------------------------------------------------------
def test_mcp_replay_rejects_unlisted_image(tmp_path: Path) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    proc = McpProc(["--allow-root", str(tmp_path)])
    try:
        proc.initialize()
        for image in ("attacker/evil:latest", "--privileged"):
            frame = proc.call(
                "replay",
                {"manifest_path": str(manifest), "claim": "claim-A", "dry_run": True, "image": image},
            )
            assert "error" in frame or frame["result"]["isError"] is True, (image, frame)
            assert "not allowed" in json.dumps(frame), frame
    finally:
        proc.close()


def test_mcp_replay_allows_operator_listed_image(tmp_path: Path) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    proc = McpProc(["--allow-root", str(tmp_path), "--allow-image", "ghcr.io/org/tool:1"])
    try:
        proc.initialize()
        ok = proc.call(
            "replay",
            {"manifest_path": str(manifest), "claim": "claim-A", "dry_run": True, "image": "ghcr.io/org/tool:1"},
        )
        assert ok["result"]["isError"] is False, ok
        # The operator list replaces the default.
        default = proc.call(
            "replay", {"manifest_path": str(manifest), "claim": "claim-A", "dry_run": True}
        )
        assert "error" in default or default["result"]["isError"] is True, default
    finally:
        proc.close()


def test_mcp_server_refuses_invalid_allow_image(tmp_path: Path) -> None:
    import subprocess
    import sys

    res = subprocess.run(
        [sys.executable, "-m", "evident_agent.mcp", "--allow-root", str(tmp_path),
         "--allow-image", "--privileged"],
        capture_output=True, text=True, timeout=30, stdin=subprocess.DEVNULL,
    )
    assert res.returncode != 0
    assert "not a valid docker image reference" in res.stderr + res.stdout
