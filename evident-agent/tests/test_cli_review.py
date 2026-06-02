"""Tests for the `evident-agent review` CLI subcommand."""

from __future__ import annotations

import json
from pathlib import Path
from textwrap import dedent

from click.testing import CliRunner

from evident_agent.cli import main


def _write_claim_artifact(tmp_path: Path, value: float = 0.008) -> Path:
    """Single-claim manifest + a JSON artifact that matches its
    tolerance.metric path.

    Layout:
      tmp_path/evident.yaml
      tmp_path/source/out.json
    """
    src = tmp_path / "source"
    src.mkdir(parents=True, exist_ok=True)
    (src / "out.json").write_text(json.dumps({"relative_error": value}))

    manifest = tmp_path / "evident.yaml"
    manifest.write_text(
        dedent(
            f"""
            version: 0.1
            project: test
            claims:
              - id: claim-A
                kind: measurement
                tier: ci
                source: source
                title: test
                claim: relative error stays under tolerance
                tolerances:
                  - metric: relative_error
                    op: "<"
                    value: 0.02
                    prose: stay within 2% relative error
                evidence:
                  oracle: [Test]
                  command: "true"
                  artifact: out.json
            """
        ).strip()
        + "\n"
    )
    return manifest


def test_review_no_api_prints_digest_and_writes_nothing(tmp_path: Path) -> None:
    manifest = _write_claim_artifact(tmp_path)
    sidecar = tmp_path / "review_events.json"
    assert not sidecar.exists()

    runner = CliRunner()
    result = runner.invoke(
        main,
        [
            "review",
            "--manifest",
            str(manifest),
            "--claim",
            "claim-A",
            "--model",
            "claude-opus-4-7",
            "--review-sidecar",
            str(sidecar),
            "--no-api",
        ],
    )
    assert result.exit_code == 0, result.output
    # The digest header should appear in stderr (digest line).
    assert "format=json" in result.output
    assert "metric_present=pass" in result.output
    # --no-api must not touch the sidecar.
    assert not sidecar.exists()


def test_review_unmatched_claim_filter_exits_nonzero(tmp_path: Path) -> None:
    manifest = _write_claim_artifact(tmp_path)
    runner = CliRunner()
    result = runner.invoke(
        main,
        [
            "review",
            "--manifest",
            str(manifest),
            "--claim",
            "nonexistent-claim",
            "--model",
            "claude-opus-4-7",
            "--no-api",
        ],
    )
    assert result.exit_code != 0


def test_review_skips_claim_with_no_evidence_artifact(tmp_path: Path) -> None:
    """A claim that lacks evidence.artifact is logged as a skip
    without raising."""
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
                claim: x
                tolerances:
                  - metric: relative_error
                    op: "<"
                    value: 0.02
                    prose: x
            """
        ).strip()
        + "\n"
    )
    runner = CliRunner()
    result = runner.invoke(
        main,
        [
            "review",
            "--manifest",
            str(manifest),
            "--model",
            "claude-opus-4-7",
            "--no-api",
        ],
    )
    # No API call should be made but the run completes.
    assert "no evidence.artifact" in result.output
