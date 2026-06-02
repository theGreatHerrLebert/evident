"""Review walkthrough tests.

Cover the load-bearing behaviour:

- per-claim accept/drop/skip/quit
- idempotency on already-curated claims (re-run continues partial
  session)
- promote_claim invoked correctly on accept
- drop_claim invoked correctly on drop
- per-claim timing recorded (minutes_spent)
- curation log shape matches the experiment template
- cited.md anchor surfacing
- prompt-shim pattern so tests don't need a real terminal
"""

from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from evident_agent import review_walkthrough as rw
from evident_agent.curator import promote_claim
from evident_agent.review_sidecar import read_events


# ---------------------------------------------------------------------
# Manifest + cited.md fixture builders
# ---------------------------------------------------------------------


def _sample_manifest(tmp_path: Path) -> Path:
    body = {
        "version": "0.1",
        "project": "extracted/test-paper",
        "claims": [
            {
                "id": "claim-a",
                "title": "Claim A",
                "kind": "measurement",
                "tier": "research",
                "source": "source/cited.md",
                "case": "source/cited.md#claim-a",
                "claim": "Our method achieves rmsd < 0.5.",
                "tolerances": [
                    {
                        "metric": "rmsd",
                        "op": "<",
                        "value": 0.5,
                        "prose": "stated 0.5",
                    }
                ],
                "evidence": {
                    "oracle": ["Paper-Authority"],
                    "command": "no-replay-path",
                    "artifact": "source/cited.md#claim-a",
                    "replay_status": "unavailable_artifacts",
                    "replay_reason": "code_private",
                },
                "provenance": {
                    "kind": "extracted-from-paper",
                    "source_id": "arxiv:2501.99999v1",
                    "extractor": {
                        "model": "claude-opus-4-7",
                        "extracted_at": "2026-05-01T10:00:00Z",
                    },
                    "curator": None,
                },
            },
            {
                "id": "claim-b",
                "title": "Claim B",
                "kind": "measurement",
                "tier": "research",
                "source": "source/cited.md",
                "case": "source/cited.md#claim-b",
                "claim": "Our method achieves throughput > 1000.",
                "tolerances": [
                    {
                        "metric": "throughput",
                        "op": ">",
                        "value": 1000,
                        "prose": "stated 1000",
                    }
                ],
                "evidence": {
                    "oracle": ["Paper-Authority"],
                    "command": "no-replay-path",
                    "artifact": "source/cited.md#claim-b",
                    "replay_status": "unavailable_artifacts",
                    "replay_reason": "code_private",
                },
                "provenance": {
                    "kind": "extracted-from-paper",
                    "source_id": "arxiv:2501.99999v1",
                    "extractor": {
                        "model": "claude-opus-4-7",
                        "extracted_at": "2026-05-01T10:00:00Z",
                    },
                    "curator": None,
                },
            },
            {
                "id": "claim-c",
                "title": "Claim C",
                "kind": "measurement",
                "tier": "research",
                "source": "source/cited.md",
                "case": "source/cited.md#claim-c",
                "claim": "Our method achieves error < 0.01.",
                "tolerances": [
                    {
                        "metric": "error",
                        "op": "<",
                        "value": 0.01,
                        "prose": "stated 0.01",
                    }
                ],
                "evidence": {
                    "oracle": ["Paper-Authority"],
                    "command": "no-replay-path",
                    "artifact": "source/cited.md#claim-c",
                    "replay_status": "unavailable_artifacts",
                    "replay_reason": "code_private",
                },
                "provenance": {
                    "kind": "extracted-from-paper",
                    "source_id": "arxiv:2501.99999v1",
                    "extractor": {
                        "model": "claude-opus-4-7",
                        "extracted_at": "2026-05-01T10:00:00Z",
                    },
                    "curator": None,
                },
            },
        ],
    }
    path = tmp_path / "evident.yaml"
    path.write_text(yaml.safe_dump(body, sort_keys=False))
    cited_dir = tmp_path / "source"
    cited_dir.mkdir(exist_ok=True)
    (cited_dir / "cited.md").write_text(
        "# Citations\n\n"
        '## <a id="claim-a"></a>claim-a\n\n'
        "rmsd less than 0.5 across BPTI\n\n"
        '## <a id="claim-b"></a>claim-b\n\n'
        "throughput greater than 1000\n\n"
        '## <a id="claim-c"></a>claim-c\n\n'
        "error less than 0.01\n",
        encoding="utf-8",
    )
    return path


def _scripted(decisions: list[str], tiers: list[str] | None = None, rationales: list[str] | None = None):
    """Return three prompt callbacks driven by canned answers.
    Each prompt pops the next value from the relevant list."""
    decisions = list(decisions)
    tiers = list(tiers) if tiers else []
    rationales = list(rationales) if rationales else []

    def _decision(_display: str) -> str:
        return decisions.pop(0)

    def _tier() -> str:
        return tiers.pop(0)

    def _text(_prompt: str) -> str:
        return rationales.pop(0)

    return _decision, _tier, _text


# ---------------------------------------------------------------------
# Per-decision behaviour
# ---------------------------------------------------------------------


def test_walkthrough_accept_calls_promote_and_records_decision(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    decision, tier, text = _scripted(
        ["accept", "skip", "skip"], ["ci"], ["rationale a"]
    )
    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    # Claim A promoted.
    parsed = yaml.safe_load(manifest_path.read_text())
    by_id = {c["id"]: c for c in parsed["claims"]}
    assert by_id["claim-a"]["tier"] == "ci"
    assert by_id["claim-b"]["tier"] == "research"
    # Record reflects the accept.
    recs = {r.extracted_id: r for r in result.records}
    assert recs["claim-a"].decision == "accept"
    assert recs["claim-a"].to_tier == "ci"
    assert recs["claim-a"].rationale == "rationale a"
    # Sidecar event written.
    events = read_events(tmp_path / "review_events.json")
    assert len(events) == 1
    assert events[0].kind == "promote_from_extracted"


def test_walkthrough_drop_removes_claim_and_records_decision(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    decision, tier, text = _scripted(
        ["drop", "skip", "skip"], [], []
    )
    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    parsed = yaml.safe_load(manifest_path.read_text())
    ids = [c["id"] for c in parsed["claims"]]
    assert "claim-a" not in ids
    recs = {r.extracted_id: r for r in result.records}
    assert recs["claim-a"].decision == "drop"


def test_walkthrough_skip_leaves_manifest_unchanged(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    pre = manifest_path.read_text()
    decision, tier, text = _scripted(
        ["skip", "skip", "skip"], [], []
    )
    rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    assert manifest_path.read_text() == pre


def test_walkthrough_quit_breaks_early_and_records_remainder_as_unseen(
    tmp_path: Path,
):
    manifest_path = _sample_manifest(tmp_path)
    decision, tier, text = _scripted(
        ["accept", "quit"], ["ci"], ["rationale a"]
    )
    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    assert result.quit_early
    # Only the first claim got a real decision; the quit produces
    # a skip record for the second claim, and the third never
    # entered the loop.
    decisions = [r.decision for r in result.records]
    assert decisions == ["accept", "skip"]
    assert result.records[1].notes == "walkthrough quit early"


# ---------------------------------------------------------------------
# Idempotency: already-curated claims are skipped, not re-prompted
# ---------------------------------------------------------------------


def test_walkthrough_skips_already_promoted_claims(tmp_path: Path):
    """A second walkthrough run after some claims have already been
    promoted must NOT re-prompt for them. The record marks them
    as already_curated."""
    manifest_path = _sample_manifest(tmp_path)
    # First run: promote claim-a.
    promote_claim(
        manifest_path=manifest_path,
        claim_id="claim-a",
        to_tier="ci",
        rationale="first run",
        curator="Jane",
    )
    # Second run: scripted to accept the next two. Note we only
    # provide TWO decisions; if the walkthrough re-prompted for
    # claim-a it would IndexError trying to pop a third.
    decision, tier, text = _scripted(
        ["skip", "skip"], [], []
    )
    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    by_id = {r.extracted_id: r for r in result.records}
    assert by_id["claim-a"].decision == "already_curated"
    assert by_id["claim-a"].to_tier == "ci"
    assert by_id["claim-b"].decision == "skip"
    assert by_id["claim-c"].decision == "skip"


# ---------------------------------------------------------------------
# Timing recorded
# ---------------------------------------------------------------------


def test_walkthrough_records_minutes_spent_per_claim(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    decision, tier, text = _scripted(
        ["accept", "skip", "skip"], ["ci"], ["rationale"]
    )
    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    for r in result.records:
        assert r.started_at, f"{r.extracted_id} missing started_at"
        assert r.ended_at, f"{r.extracted_id} missing ended_at"
        # minutes_spent is a non-negative float.
        assert r.minutes_spent >= 0


# ---------------------------------------------------------------------
# Curation log shape
# ---------------------------------------------------------------------


def test_render_curation_log_matches_experiment_template_shape(tmp_path: Path):
    """The curation log must carry the fields the experiment's
    aggregate.py expects to read."""
    manifest_path = _sample_manifest(tmp_path)
    decision, tier, text = _scripted(
        ["accept", "drop", "skip"], ["ci"], ["rationale a"]
    )
    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    log = rw.render_curation_log(result)
    # Top-level fields.
    assert log["artifact_id"] == "extracted/test-paper"
    assert log["curator"] == "Jane"
    assert "extraction" in log
    assert "curation" in log
    assert log["extraction"]["accepted"] == 1
    assert log["extraction"]["dropped"] == 1
    assert log["extraction"]["skipped"] == 1
    assert isinstance(log["curation"]["minutes_total"], float)
    # Per-claim shape.
    extracted = log["extracted_claims"]
    assert len(extracted) == 3
    by_id = {c["extracted_id"]: c for c in extracted}
    assert by_id["claim-a"]["decision"] == "accept"
    assert by_id["claim-a"]["to_tier"] == "ci"
    assert by_id["claim-a"]["rationale"] == "rationale a"
    assert by_id["claim-b"]["decision"] == "drop"


def test_write_curation_log_writes_yaml(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    decision, tier, text = _scripted(
        ["skip", "skip", "skip"], [], []
    )
    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    log_path = tmp_path / "curation_log.yaml"
    rw.write_curation_log(result, log_path)
    assert log_path.is_file()
    parsed = yaml.safe_load(log_path.read_text())
    assert parsed["artifact_id"] == "extracted/test-paper"


# ---------------------------------------------------------------------
# Cited.md anchor surfacing
# ---------------------------------------------------------------------


def test_display_includes_cited_md_section_when_present(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    manifest = yaml.safe_load(manifest_path.read_text())
    claim_a = manifest["claims"][0]
    anchor = rw._read_cited_md_anchor(
        tmp_path / "source" / "cited.md", "claim-a"
    )
    assert anchor is not None
    assert "rmsd less than 0.5" in anchor
    display = rw._format_claim_for_display(claim_a, anchor)
    assert "Claim id:    claim-a" in display
    assert "rmsd less than 0.5" in display


def test_walkthrough_tolerates_missing_cited_md(tmp_path: Path):
    """If cited.md doesn't exist, the walkthrough still runs and
    just omits the cited-source block from the display."""
    manifest_path = _sample_manifest(tmp_path)
    (tmp_path / "source" / "cited.md").unlink()
    decision, tier, text = _scripted(
        ["skip", "skip", "skip"], [], []
    )
    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    assert len(result.records) == 3


# ---------------------------------------------------------------------
# CLI integration
# ---------------------------------------------------------------------


def test_cli_review_extracted_runs_against_canned_input(tmp_path: Path):
    """End-to-end: invoke the `evident-agent review-extracted`
    subcommand with click.testing.CliRunner and feed canned
    keystrokes via stdin. Verify the curation log is written and
    the manifest reflects the curator's decisions."""
    from click.testing import CliRunner
    from evident_agent.cli import main

    manifest_path = _sample_manifest(tmp_path)
    runner = CliRunner()
    # Inputs: claim-a accept→ci, rationale; claim-b drop; claim-c skip
    keystrokes = "\n".join([
        "a",
        "ci",
        "rationale a",
        "d",
        "s",
        "",
    ])
    result = runner.invoke(
        main,
        [
            "review-extracted",
            "--manifest", str(manifest_path),
            "--curator", "Jane Doe",
            "--curation-log", str(tmp_path / "curation_log.yaml"),
        ],
        input=keystrokes,
    )
    assert result.exit_code == 0, (
        f"CLI exited non-zero. stderr:\n{result.output}"
    )
    log = yaml.safe_load(
        (tmp_path / "curation_log.yaml").read_text()
    )
    decisions = {
        c["extracted_id"]: c["decision"]
        for c in log["extracted_claims"]
    }
    assert decisions == {
        "claim-a": "accept",
        "claim-b": "drop",
        "claim-c": "skip",
    }
    parsed = yaml.safe_load(manifest_path.read_text())
    by_id = {c["id"]: c for c in parsed["claims"]}
    assert by_id["claim-a"]["tier"] == "ci"
    assert "claim-b" not in by_id  # dropped
    assert by_id["claim-c"]["tier"] == "research"
