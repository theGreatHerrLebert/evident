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


def test_last_verified_commit_reaches_the_digest_header(tmp_path: Path) -> None:
    """Codex F-CR1 regression: the model must see the commit hash
    from the last_verified sidecar so the reproducible_chain check
    can pass. Without this overlay, the digest header has
    ``commit: null`` and an Endorse becomes hallucination.

    We assert by reading the digest line printed by --no-api: the
    ``format=…`` summary line proves the digest was built, and a
    second --no-api flag would suppress sidecar writes — but the
    digest header in the rendered output should now include the
    commit. We trigger the same code path and verify the digest
    rendered text contains the commit hex.
    """
    manifest = _write_claim_artifact(tmp_path)
    last_verified = tmp_path / "last_verified.json"
    last_verified.write_text(
        json.dumps(
            {
                "claim-A": {
                    "commit": "deadbeefcafe1234",
                    "date": "2026-06-02",
                    "value": 0.008,
                }
            }
        )
    )

    # Build a digest the same way the CLI would and assert commit
    # propagates. Going via the public surface (evidence.make_digest)
    # so the test catches future regressions in the digest pipeline.
    from evident_agent import evidence as evidence_mod
    from evident_agent import sidecar as sidecar_mod
    from evident_agent.cli import _resolve_commit_for_claim

    entries = sidecar_mod.read(last_verified)
    assert "claim-A" in entries
    commit = _resolve_commit_for_claim(
        {"last_verified": None}, entries.get("claim-A")
    )
    assert commit == "deadbeefcafe1234"

    digest = evidence_mod.make_digest(
        tmp_path / "source" / "out.json",
        "relative_error",
        source_dir=tmp_path / "source",
        commit=commit,
    )
    rendered = digest.render()
    assert "deadbeefcafe1234" in rendered


def test_record_path_sanitization_codex_3_cr1(tmp_path: Path) -> None:
    """Codex F-CR3-1 regression: claim ids containing path separators
    or traversal segments must be sanitized before becoming filename
    components. The recorded file must land inside the requested
    record dir."""
    from evident_agent.cli import _safe_fixture_path

    record_dir = tmp_path / "record"
    record_dir.mkdir()

    # Slash separators get replaced.
    p = _safe_fixture_path(record_dir, "org/claim")
    assert p.parent == record_dir.resolve()
    assert p.name == "org_claim.json"

    # Backslash separators get replaced too.
    p = _safe_fixture_path(record_dir, "org\\claim")
    assert p.name == "org_claim.json"

    # Dot-prefixed traversal segments get neutralized.
    p = _safe_fixture_path(record_dir, "../escape")
    assert p.parent == record_dir.resolve()
    assert ".." not in p.name

    # Bare `.` and `..` become `_unnamed`-ish but stay inside.
    p = _safe_fixture_path(record_dir, ".")
    assert p.parent == record_dir.resolve()
    p = _safe_fixture_path(record_dir, "..")
    assert p.parent == record_dir.resolve()


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
