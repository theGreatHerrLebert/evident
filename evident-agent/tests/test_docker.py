"""Tests for docker invocation argv construction.

We don't actually invoke docker in unit tests — that needs the
real image. We just verify the argv we'd hand subprocess.run is
shaped correctly.
"""

from __future__ import annotations

from pathlib import Path

from evident_agent.docker import DockerResult, build_command, run


def test_build_command_basic() -> None:
    argv = build_command(
        image="proteon-evident:latest",
        claim_id="proteon-sasa-vs-biopython-ci",
        source_dir=Path("/scratch/TMAlign/proteon"),
    )
    assert argv[0] == "docker"
    assert "run" in argv
    assert "--rm" in argv
    # Volume mount of the resolved source dir.
    assert any("/scratch/TMAlign/proteon" in a for a in argv)
    # Image and subcommand.
    assert "proteon-evident:latest" in argv
    assert argv[-2:] == ["replay", "proteon-sasa-vs-biopython-ci"]


def test_build_command_with_extra_volumes() -> None:
    argv = build_command(
        image="img",
        claim_id="c1",
        source_dir=Path("/work"),
        extra_volumes=["/cache:/cache:ro"],
    )
    assert "/cache:/cache:ro" in argv


def test_dry_run_returns_placeholder() -> None:
    """dry_run=True must not invoke subprocess.run."""
    result = run(
        image="never-invoked",
        claim_id="claim-X",
        source_dir=Path("/tmp"),
        dry_run=True,
    )
    assert isinstance(result, DockerResult)
    assert result.exit_code == 0
    assert result.duration_s == 0.0
    assert "dry-run" in result.stdout_tail
