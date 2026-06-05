"""Contract-fidelity tests for the run_replay refactor (Codex review):
exit codes, stderr routing, and nl=False render fidelity must match the
former inline callback."""

from __future__ import annotations

from pathlib import Path
from textwrap import dedent

import pytest
from click.testing import CliRunner

import evident_agent.replay as rmod
import evident_agent.typed_trust as tt
from evident_agent.cli import main


def _manifest(tmp_path: Path) -> Path:
    m = tmp_path / "evident.yaml"
    m.write_text(
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
    return m


def test_cli_no_match_exits_2(tmp_path: Path) -> None:
    manifest = _manifest(tmp_path)
    runner = CliRunner()
    result = runner.invoke(
        main,
        ["replay", "--manifest", str(manifest), "--claim", "ghost", "--no-execute"],
    )
    assert result.exit_code == 2
    assert "no measurement claims matched" in result.stderr


def test_run_replay_no_match_raises(tmp_path: Path) -> None:
    manifest = _manifest(tmp_path)
    with pytest.raises(rmod.NoClaimsMatched):
        rmod.run_replay(manifest_path=manifest, claim_filter="ghost", no_execute=True)


def test_render_emits_nl_false(tmp_path: Path, monkeypatch) -> None:
    manifest = _manifest(tmp_path)
    monkeypatch.setattr(
        rmod.typed_trust,
        "run",
        lambda **_k: tt.TypedTrustResult(exit_code=0, stdout="RENDERED", stderr=""),
    )
    events: list[tuple] = []

    def cb(msg, *, err=False, nl=True):
        events.append((msg, err, nl))

    rmod.run_replay(
        manifest_path=manifest,
        claim_filter="claim-A",
        no_execute=True,
        render="json",
        on_event=cb,
    )
    # The render line is the only nl=False emission.
    assert ("RENDERED", False, False) in events


def test_render_failure_raises_with_exit_code(tmp_path: Path, monkeypatch) -> None:
    manifest = _manifest(tmp_path)
    monkeypatch.setattr(
        rmod.typed_trust,
        "run",
        lambda **_k: tt.TypedTrustResult(exit_code=3, stdout="", stderr="boom"),
    )
    with pytest.raises(rmod.RenderFailed) as ei:
        rmod.run_replay(
            manifest_path=manifest,
            claim_filter="claim-A",
            no_execute=True,
            render="json",
        )
    assert ei.value.exit_code == 3
    assert ei.value.stderr == "boom"


def test_cli_render_failure_passthrough(tmp_path: Path, monkeypatch) -> None:
    manifest = _manifest(tmp_path)
    monkeypatch.setattr(
        "evident_agent.typed_trust.run",
        lambda **_k: tt.TypedTrustResult(exit_code=7, stdout="", stderr="render-broke"),
    )
    runner = CliRunner()
    result = runner.invoke(
        main,
        [
            "replay",
            "--manifest",
            str(manifest),
            "--claim",
            "claim-A",
            "--no-execute",
            "--render",
            "json",
        ],
    )
    assert result.exit_code == 7
    assert "render-broke" in result.stderr
