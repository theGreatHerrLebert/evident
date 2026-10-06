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


# ---------------------------------------------------------------------
# Fake docker for #5 and #6: logs argv; behaviour from FAKE_DOCKER_MODE
# ---------------------------------------------------------------------
_FAKE_DOCKER = """#!/usr/bin/env bash
echo "$@" >> "$FAKE_DOCKER_LOG"
[ "$1" = "rm" ] && exit 0
case "$FAKE_DOCKER_MODE" in
  sleep) exec sleep 30 ;;
  exit125) echo "docker: Error response from daemon" >&2; exit 125 ;;
  flood) head -c 50000000 /dev/zero | tr '\\\\0' 'x'; exit 0 ;;
  *) exit 0 ;;
esac
"""


@pytest.fixture
def fake_docker(tmp_path: Path, monkeypatch):
    bindir = tmp_path / "fakebin"
    bindir.mkdir()
    script = bindir / "docker"
    script.write_text(_FAKE_DOCKER)
    script.chmod(0o755)
    log = tmp_path / "docker.log"
    monkeypatch.setenv("PATH", f"{bindir}:{os.environ['PATH']}")
    monkeypatch.setenv("FAKE_DOCKER_LOG", str(log))

    def set_mode(mode: str) -> Path:
        monkeypatch.setenv("FAKE_DOCKER_MODE", mode)
        return log

    return set_mode


from evident_agent import docker as docker_mod  # noqa: E402
from evident_agent import sidecar as sidecar_mod  # noqa: E402


def test_timeout_removes_named_container(tmp_path: Path, fake_docker) -> None:
    log = fake_docker("sleep")
    res = docker_mod.run("img", "claim-A", tmp_path, budget_seconds=1.0)
    assert res.timed_out and res.exit_code == 124
    calls = log.read_text().splitlines()
    run_call = next(c for c in calls if c.startswith("run "))
    name = run_call.split("--name ")[1].split()[0]
    assert name.startswith("evident-replay-")
    assert f"rm -f {name}" in calls


def test_output_flood_is_bounded(tmp_path: Path, fake_docker) -> None:
    fake_docker("flood")
    res = docker_mod.run("img", "claim-A", tmp_path, budget_seconds=60, tail_bytes=2048)
    assert res.exit_code == 0
    assert len(res.stdout_tail) <= 2048 + len("...[truncated]...\n")


def _seed_sidecar(path: Path, claim_id: str) -> None:
    sidecar_mod.write(
        path,
        {claim_id: sidecar_mod.LastVerifiedEntry(commit="abc", date="2026-01-01", value=0.5, corpus_sha=None)},
    )


@pytest.mark.parametrize("mode,outcome", [("exit125", "infrastructure_error"), ("sleep", "timed_out")])
def test_failed_attempt_keeps_previous_verification(tmp_path: Path, fake_docker, mode, outcome) -> None:
    fake_docker(mode)
    manifest = _write_measurement_manifest(tmp_path)
    sidecar_path = tmp_path / "last_verified.json"
    _seed_sidecar(sidecar_path, "claim-A")
    before = sidecar_path.read_text()

    result = replay_mod.run_replay(
        manifest_path=manifest, claim_filter="claim-A", image="img",
        budget=1.0, sidecar_path=sidecar_path,
    )
    assert [c.outcome for c in result.claims] == [outcome]
    assert json.loads(sidecar_path.read_text()) == json.loads(before)


def test_mcp_replay_exit125_is_tool_error(tmp_path: Path, fake_docker) -> None:
    fake_docker("exit125")
    manifest = _write_measurement_manifest(tmp_path)
    proc = McpProc(["--allow-root", str(tmp_path), "--allow-docker", "--allow-image", "img"],
                   env=dict(os.environ))
    try:
        proc.initialize()
        frame = proc.call("replay", {"manifest_path": str(manifest), "claim": "claim-A", "image": "img"})
        assert frame["result"]["isError"] is True, frame
        assert "infrastructure_error" in frame["result"]["content"][0]["text"]
    finally:
        proc.close()


@pytest.mark.parametrize("budget", [0, -5, 1e12])  # JSON has no infinity
def test_mcp_replay_rejects_bad_budget(tmp_path: Path, budget) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    proc = McpProc(["--allow-root", str(tmp_path)])
    try:
        proc.initialize()
        frame = proc.call(
            "replay",
            {"manifest_path": str(manifest), "claim": "claim-A", "dry_run": True, "budget": budget},
        )
        assert "error" in frame or frame["result"]["isError"] is True, frame
    finally:
        proc.close()


# ---------------------------------------------------------------------
# Low #14: dry runs create nothing; malformed sidecars are data errors
# ---------------------------------------------------------------------
from test_mcp_loadbearing import FIXTURE_PYPROJECT  # noqa: E402


@pytest.mark.parametrize("server_args,call_args", [
    ([], {}),                                  # capability-gated (no --allow-docker)
    (["--allow-docker"], {"dry_run": True}),   # explicit dry run
])
def test_replay_dry_run_creates_no_directories(tmp_path: Path, server_args, call_args) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    new_dir = tmp_path / "not" / "yet"
    proc = McpProc(["--allow-root", str(tmp_path), *server_args])
    try:
        proc.initialize()
        frame = proc.call(
            "replay",
            {"manifest_path": str(manifest), "claim": "claim-A",
             "sidecar": str(new_dir / "last_verified.json"), **call_args},
        )
        assert frame["result"]["isError"] is False, frame
        assert _result_payload(frame)["dry_run"] is True
    finally:
        proc.close()
    assert not (tmp_path / "not").exists()


def test_extract_repo_dry_run_writes_preview_inside_authorized_dir(tmp_path: Path) -> None:
    """Unlike replay, a dry extraction writes preview files by design; the
    output dir must still be authorized (and is materialized + rechecked)."""
    out = tmp_path / "gen" / "deep"
    proc = McpProc(["--allow-root", str(FIXTURE_PYPROJECT), "--allow-root", str(tmp_path)])
    try:
        proc.initialize()
        frame = proc.call(
            "extract_repo",
            {"repo_path": str(FIXTURE_PYPROJECT), "output_dir": str(out), "dry_run": True},
        )
        assert frame["result"]["isError"] is False, frame
        assert _result_payload(frame)["dry_run"] is True
    finally:
        proc.close()
    assert out.is_dir()


@pytest.mark.parametrize("content", ["[]", "{not json", '"a string"'])
def test_malformed_sidecar_is_data_error(tmp_path: Path, content: str) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    (tmp_path / "last_verified.json").write_text(content)
    proc = McpProc(["--allow-root", str(tmp_path)])
    try:
        proc.initialize()
        frame = proc.call(
            "replay", {"manifest_path": str(manifest), "claim": "claim-A", "no_execute": True}
        )
        # Tier-2 (the model can react), naming the problem, not -32603 internal.
        assert "result" in frame and frame["result"]["isError"] is True, frame
        assert "sidecar" in frame["result"]["content"][0]["text"]
    finally:
        proc.close()


# ---------------------------------------------------------------------
# Host networking is an operator setting (--docker-network)
# ---------------------------------------------------------------------
@pytest.mark.parametrize("mode", ["host", "bridge", "none"])
def test_build_command_network_modes(mode: str) -> None:
    argv = build_command("img", "claim-A", Path("/tmp"), network=mode)
    assert argv[argv.index("--network") + 1] == mode


def test_build_command_rejects_unknown_network() -> None:
    with pytest.raises(ValueError):
        build_command("img", "claim-A", Path("/tmp"), network="container:other")


def test_mcp_docker_network_flag_reaches_docker(tmp_path: Path, fake_docker) -> None:
    log = fake_docker("ok")
    (tmp_path / "out.json").write_text("{}")
    manifest = _write_measurement_manifest(tmp_path)
    proc = McpProc(["--allow-root", str(tmp_path), "--allow-docker", "--allow-image", "img",
                    "--docker-network", "none"], env=dict(os.environ))
    try:
        proc.initialize()
        frame = proc.call("replay", {"manifest_path": str(manifest), "claim": "claim-A", "image": "img"})
        assert "result" in frame, frame
    finally:
        proc.close()
    run_call = next(c for c in log.read_text().splitlines() if c.startswith("run "))
    assert "--network none" in run_call


def test_mcp_server_rejects_unknown_network(tmp_path: Path) -> None:
    import subprocess
    import sys

    res = subprocess.run(
        [sys.executable, "-m", "evident_agent.mcp", "--allow-root", str(tmp_path),
         "--docker-network", "container:x"],
        capture_output=True, text=True, timeout=30, stdin=subprocess.DEVNULL,
    )
    assert res.returncode != 0 and "--docker-network" in res.stderr + res.stdout
