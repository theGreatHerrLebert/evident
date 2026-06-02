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
    Each prompt pops the next value from the relevant list.

    The tier callback receives (valid_targets, current_tier) — the
    ladder-aware signature. Each canned tier value must be in the
    valid_targets list at that step or the test deliberately scripts
    a decline (by returning the current_tier).
    """
    decisions = list(decisions)
    tiers = list(tiers) if tiers else []
    rationales = list(rationales) if rationales else []

    def _decision(_display: str) -> str:
        return decisions.pop(0)

    def _tier(_valid_targets: list[str], _current_tier: str) -> str:
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


def test_walkthrough_quit_breaks_early_and_records_remainder_as_unreviewed(
    tmp_path: Path,
):
    """Codex F-WALK-CR3 P2: claims that never got walked because
    of an early quit must appear in the records as `unreviewed`
    so the aggregator's len(extracted_claims) denominator stays
    correct."""
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
    decisions = [r.decision for r in result.records]
    # claim-a accepted; claim-b quit-skip; claim-c unreviewed.
    assert decisions == ["accept", "skip", "unreviewed"]
    assert result.records[1].notes == "walkthrough quit early"
    assert result.records[2].notes and "quit early" in result.records[2].notes


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
    # Top-level fields aligned with the experiment template
    # (codex F-WALK-CR2 P2).
    assert log["artifact_id"] == "extracted/test-paper"
    assert log["curator"] == "Jane"
    assert log["extraction"]["run_at"] == "2026-05-01T10:00:00Z"
    assert log["extraction"]["extractor_model"] == "claude-opus-4-7"
    assert log["extraction"]["extracted_claims_count"] == 3
    assert log["curation"]["accepted_count"] == 1
    assert log["curation"]["dropped_count"] == 1
    assert log["curation"]["skipped_count"] == 1
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


def test_walkthrough_preserves_prior_accept_records_on_rerun(tmp_path: Path):
    """Codex F-WALK-CR1 P1 (load-bearing): a rerun must NOT erase
    prior accept records. The cumulative curation log accumulates
    across runs."""
    manifest_path = _sample_manifest(tmp_path)
    log_path = tmp_path / "curation_log.yaml"

    # Run 1: accept claim-a, skip rest.
    decision, tier, text = _scripted(
        ["accept", "skip", "skip"], ["ci"], ["initial rationale"]
    )
    r1 = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        curation_log_path=log_path,
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    rw.write_curation_log(r1, log_path)

    # Run 2: claim-a already promoted; claim-b/c get walked.
    decision, tier, text = _scripted(
        ["drop", "skip"], [], []
    )
    r2 = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        curation_log_path=log_path,
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    # claim-a's accept survives.
    by_id = {r.extracted_id: r for r in r2.records}
    assert by_id["claim-a"].decision == "accept"
    assert by_id["claim-a"].rationale == "initial rationale"
    assert by_id["claim-b"].decision == "drop"
    assert by_id["claim-c"].decision == "skip"


def test_walkthrough_preserves_prior_drop_records_on_rerun(tmp_path: Path):
    """Codex F-WALK-CR1 P1: dropped claims aren't in the manifest
    anymore, but the curation log must still reflect that the
    curator dropped them. Without this, the experiment's drop
    tally would be lost on rerun."""
    manifest_path = _sample_manifest(tmp_path)
    log_path = tmp_path / "curation_log.yaml"

    # Run 1: drop claim-a, skip rest.
    decision, tier, text = _scripted(
        ["drop", "skip", "skip"], [], []
    )
    r1 = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        curation_log_path=log_path,
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    rw.write_curation_log(r1, log_path)

    # Run 2: claim-a is gone from the manifest. Walkthrough must
    # still carry its drop record into the new log.
    decision, tier, text = _scripted(
        ["skip", "skip"], [], []
    )
    r2 = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        curation_log_path=log_path,
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    by_id = {r.extracted_id: r for r in r2.records}
    assert "claim-a" in by_id
    assert by_id["claim-a"].decision == "drop"


def test_walkthrough_quit_then_resume_finishes_the_remaining_claims(
    tmp_path: Path,
):
    """Curator quits mid-walkthrough; second run picks up the
    unreviewed claims and finishes them."""
    manifest_path = _sample_manifest(tmp_path)
    log_path = tmp_path / "curation_log.yaml"

    decision, tier, text = _scripted(
        ["accept", "quit"], ["ci"], ["first run rationale"]
    )
    r1 = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        curation_log_path=log_path,
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    rw.write_curation_log(r1, log_path)

    # claim-a was accepted; claim-b was the active quit-skip;
    # claim-c was never reached. Run 2 must walk claim-b and
    # claim-c.
    decision, tier, text = _scripted(
        ["drop", "skip"], [], []
    )
    r2 = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        curation_log_path=log_path,
        prompt_decision=decision,
        prompt_tier=tier,
        prompt_text=text,
    )
    by_id = {r.extracted_id: r for r in r2.records}
    assert by_id["claim-a"].decision == "accept"
    assert by_id["claim-b"].decision == "drop"
    assert by_id["claim-c"].decision == "skip"


def test_walkthrough_omits_extracted_at_when_claims_disagree(tmp_path: Path):
    """Codex note: first-claim-wins is misleading when claims come
    from different extraction runs. When manifest claims disagree
    on extracted_at, the curation log records None for run_at."""
    manifest_path = _sample_manifest(tmp_path)
    manifest = yaml.safe_load(manifest_path.read_text())
    manifest["claims"][1]["provenance"]["extractor"]["extracted_at"] = (
        "2026-05-15T10:00:00Z"
    )
    manifest_path.write_text(yaml.safe_dump(manifest, sort_keys=False))
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
    assert result.extraction_started_at == ""


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
# Multi-step ladder awareness (PR after #28)
# ---------------------------------------------------------------------


def test_prompt_tier_receives_only_next_rung_for_research_claim(tmp_path: Path):
    """Ladder awareness: a research-tier claim's tier prompt is
    called with valid_targets=['ci'], not the old ['ci', 'release']."""
    manifest_path = _sample_manifest(tmp_path)
    seen_valid_targets: list[list[str]] = []
    seen_current_tiers: list[str] = []

    def _decision(_display):
        return "accept"

    def _tier(valid_targets, current_tier):
        seen_valid_targets.append(list(valid_targets))
        seen_current_tiers.append(current_tier)
        return valid_targets[0]

    def _text(_p):
        return "rationale"

    # All three claims are at research; each accept walks through
    # the prompt once.
    rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=_decision,
        prompt_tier=_tier,
        prompt_text=_text,
    )
    assert seen_valid_targets == [["ci"], ["ci"], ["ci"]]
    assert seen_current_tiers == ["research", "research", "research"]


def test_prompt_tier_receives_release_after_first_promotion(tmp_path: Path):
    """Multi-step: run 1 promotes research → ci; run 2 (which
    re-enters the same claim because tier is no longer research)
    should NOT re-prompt for that claim — already_curated. But
    if a future fixture has a tier:ci extracted claim, the prompt
    would receive valid_targets=['release']. Verify via direct
    helper call."""
    from evident_agent import curator as curator_mod
    assert curator_mod.adjacent_promotion_target("research") == "ci"
    assert curator_mod.adjacent_promotion_target("ci") == "release"
    assert curator_mod.adjacent_promotion_target("release") is None
    # Backward-compat alias still resolves to the same callable.
    assert (
        curator_mod._adjacent_promotion_target
        is curator_mod.adjacent_promotion_target
    )


def test_walkthrough_decline_confirm_records_skip(tmp_path: Path):
    """If the curator picks accept but then declines the confirmation
    prompt (returns the same tier), the walkthrough records a skip
    instead of a failed promote."""
    manifest_path = _sample_manifest(tmp_path)

    def _decision(_d):
        return "accept"

    declined = [False, False, False]

    def _tier(valid_targets, current_tier):
        # Decline by returning current_tier (simulating the
        # _click_prompt_tier behavior when the curator says "no"
        # to the confirm prompt).
        return current_tier

    def _text(_p):
        # Should NOT be called for declined accepts.
        raise AssertionError("rationale prompt should not run on decline")

    result = rw.walk_manifest(
        manifest_path=manifest_path,
        curator="Jane",
        prompt_decision=_decision,
        prompt_tier=_tier,
        prompt_text=_text,
    )
    # All three claims declined → all three skip.
    for r in result.records:
        assert r.decision == "skip"
        assert r.notes == "curator declined promotion"
    # Manifest unchanged: tier still research everywhere.
    parsed = yaml.safe_load(manifest_path.read_text())
    for c in parsed["claims"]:
        assert c["tier"] == "research"


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
    # Inputs: claim-a accept, confirm ci promotion (y), rationale;
    # claim-b drop; claim-c skip. The ladder-aware prompt asks
    # "promote research -> ci? [y/N]" with a single next-rung
    # option rather than the old "[ci|release]" choice.
    keystrokes = "\n".join([
        "a",
        "y",
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
