"""Verify --dry-run does not mutate the sidecar."""

from __future__ import annotations

from pathlib import Path
from textwrap import dedent

import json
from click.testing import CliRunner

from evident_agent.cli import main


def _write_manifest(tmp_path: Path) -> Path:
    manifest = tmp_path / "evident.yaml"
    manifest.write_text(
        dedent(
            """
            version: 0.1
            project: test
            claims:
              - id: claim-A
                kind: measurement
                tier: ci
                source: .
                title: test
                claim: test
                tolerances:
                  - metric: relative_error
                    op: "<"
                    value: 0.01
                    prose: x
                evidence:
                  oracle: [Test]
                  command: "true"
                  artifact: out.json
            """
        ).strip()
        + "\n"
    )
    return manifest


def test_dry_run_does_not_create_sidecar(tmp_path: Path) -> None:
    manifest = _write_manifest(tmp_path)
    sidecar = tmp_path / "last_verified.json"
    assert not sidecar.exists()

    runner = CliRunner()
    result = runner.invoke(
        main,
        [
            "replay",
            "--manifest",
            str(manifest),
            "--claim",
            "claim-A",
            "--sidecar",
            str(sidecar),
            "--dry-run",
            "--no-execute",  # belt and suspenders — even if dry-run was buggy
        ],
    )
    assert result.exit_code == 0, result.output
    assert not sidecar.exists(), "dry-run must not create the sidecar"


def test_dry_run_does_not_overwrite_existing_sidecar(tmp_path: Path) -> None:
    manifest = _write_manifest(tmp_path)
    sidecar = tmp_path / "last_verified.json"
    pre_existing = {"claim-A": {"value": 0.5, "date": "2026-01-01"}}
    sidecar.write_text(json.dumps(pre_existing))
    original_mtime = sidecar.stat().st_mtime

    runner = CliRunner()
    result = runner.invoke(
        main,
        [
            "replay",
            "--manifest",
            str(manifest),
            "--claim",
            "claim-A",
            "--sidecar",
            str(sidecar),
            "--dry-run",
        ],
    )
    assert result.exit_code == 0, result.output
    # File must be unchanged — same contents AND same mtime.
    assert json.loads(sidecar.read_text()) == pre_existing
    assert sidecar.stat().st_mtime == original_mtime
