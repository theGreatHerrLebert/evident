"""Curator tooling tests.

Covers the in-place manifest edit + sidecar append for promote/drop,
the reviewed_extraction_sha discipline (pre-edit sha), and the
cross-language integration: a promoted manifest must satisfy
typed-trust's PR3 promotion validator at tier:ci.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
from pathlib import Path

import pytest
import yaml

from evident_agent.curator import (
    CuratorError,
    drop_claim,
    promote_claim,
)
from evident_agent.review_sidecar import read_events


def _sample_manifest(tmp_path: Path) -> Path:
    body = {
        "version": "0.1",
        "project": "extracted/test",
        "claims": [
            {
                "id": "test-claim-one",
                "title": "Test claim",
                "kind": "measurement",
                "tier": "research",
                "source": "source/cited.md",
                "case": "source/cited.md#test-claim-one",
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
                    "artifact": "source/cited.md#test-claim-one",
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
                "id": "test-claim-two",
                "title": "Test claim two",
                "kind": "measurement",
                "tier": "research",
                "source": "source/cited.md",
                "case": "source/cited.md#test-claim-two",
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
                    "artifact": "source/cited.md#test-claim-two",
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
    return path


# ---------------------------------------------------------------------
# promote_claim
# ---------------------------------------------------------------------


def test_promote_updates_tier_in_manifest(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="Reviewed and the bound 0.5 is stated clearly.",
        curator="Jane Doe",
    )
    parsed = yaml.safe_load(manifest_path.read_text())
    claims_by_id = {c["id"]: c for c in parsed["claims"]}
    assert claims_by_id["test-claim-one"]["tier"] == "ci"
    # Sibling claim is untouched.
    assert claims_by_id["test-claim-two"]["tier"] == "research"


def test_promote_appends_sidecar_event_with_pre_edit_sha(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    pre_edit_sha = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="rationale",
        curator="Jane Doe",
    )
    events = read_events(tmp_path / "review_events.json")
    assert len(events) == 1
    e = events[0]
    assert e.kind == "promote_from_extracted"
    assert e.promote_from_extracted == {
        "target_claim": "test-claim-one",
        "from_tier": "research",
        "to_tier": "ci",
        "reviewed_extraction_sha": pre_edit_sha,
    }
    assert e.author.kind == "human"
    assert e.author.name == "Jane Doe"


def test_promote_with_orcid_in_curator_string(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="rationale",
        curator="Jane Doe <orcid:0000-0001-2345-6789>",
    )
    events = read_events(tmp_path / "review_events.json")
    assert events[0].author.name == "Jane Doe"
    assert events[0].author.orcid == "0000-0001-2345-6789"


def test_promote_rejects_unknown_claim_id(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    with pytest.raises(CuratorError) as exc:
        promote_claim(
            manifest_path=manifest_path,
            claim_id="does-not-exist",
            to_tier="ci",
            rationale="rationale",
            curator="Jane",
        )
    assert "does-not-exist" in str(exc.value)


def test_promote_research_to_ci_then_ci_to_release_succeeds(tmp_path: Path):
    """Multi-step promotion: the curator promotes research -> ci,
    then later promotes ci -> release. Both are valid. The
    manifest ends at tier:release."""
    manifest_path = _sample_manifest(tmp_path)
    r1 = promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="initial promotion",
        curator="Jane",
    )
    assert r1.from_tier == "research"
    assert r1.to_tier == "ci"
    r2 = promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="release",
        rationale="release-readiness review",
        curator="Jane",
    )
    assert r2.from_tier == "ci"
    assert r2.to_tier == "release"
    parsed = yaml.safe_load(manifest_path.read_text())
    by_id = {c["id"]: c for c in parsed["claims"]}
    assert by_id["test-claim-one"]["tier"] == "release"


def test_promote_rejects_skipping_ci_rung(tmp_path: Path):
    """Multi-step rule: each promotion must advance ONE rung. A
    direct research -> release promotion is rejected because the
    typed-trust validator requires an event for each leg."""
    manifest_path = _sample_manifest(tmp_path)
    with pytest.raises(CuratorError) as exc:
        promote_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            to_tier="release",
            rationale="skipping",
            curator="Jane",
        )
    assert "adjacent" in str(exc.value) or "research" in str(exc.value)


def test_promote_rejects_demotion(tmp_path: Path):
    """Already at tier:release? Can't promote further (and no
    demotion path exists in the ladder)."""
    manifest_path = _sample_manifest(tmp_path)
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="step 1",
        curator="Jane",
    )
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="release",
        rationale="step 2",
        curator="Jane",
    )
    with pytest.raises(CuratorError) as exc:
        promote_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            to_tier="ci",
            rationale="demote",
            curator="Jane",
        )
    assert "not a valid promotion source" in str(exc.value) or "ladder" in str(exc.value)


def test_promote_rejects_invalid_target_tier(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    with pytest.raises(CuratorError):
        promote_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            to_tier="invalid",
            rationale="rationale",
            curator="Jane",
        )


def test_promote_rejects_empty_rationale(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    with pytest.raises(CuratorError):
        promote_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            to_tier="ci",
            rationale="   ",
            curator="Jane",
        )


def test_promote_rejects_unknown_angle_bracket_curator_token(tmp_path: Path):
    """Codex F-CURATOR-CR2 (P2): an email or other unknown token
    inside angle brackets must be rejected rather than silently
    dropped. ReviewAuthor has no email field today."""
    manifest_path = _sample_manifest(tmp_path)
    with pytest.raises(CuratorError) as exc:
        promote_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            to_tier="ci",
            rationale="rationale",
            curator="Jane Doe <jane@example.com>",
        )
    assert "angle-bracket" in str(exc.value) or "orcid" in str(exc.value)


def test_promote_partial_commit_on_sidecar_failure_keeps_manifest_at_research(
    tmp_path: Path, monkeypatch,
):
    """Codex F-CURATOR-CR1 (P1): if the sidecar append fails AFTER
    we've already written the manifest at tier:ci, the gate would
    have been violated. The fix is to append the sidecar FIRST. This
    test patches append_events to fail and verifies the manifest
    stays at tier:research — gate invariant preserved."""
    from evident_agent import curator as curator_mod

    manifest_path = _sample_manifest(tmp_path)

    def _failing_append(*args, **kwargs):
        raise RuntimeError("simulated sidecar IO failure")

    monkeypatch.setattr(curator_mod, "append_events", _failing_append)

    with pytest.raises(RuntimeError):
        promote_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            to_tier="ci",
            rationale="rationale",
            curator="Jane",
        )

    parsed = yaml.safe_load(manifest_path.read_text())
    target = next(c for c in parsed["claims"] if c["id"] == "test-claim-one")
    assert target["tier"] == "research", (
        "manifest must stay at tier:research when the sidecar append fails"
    )


def test_promote_distinct_curators_get_distinct_event_ids(tmp_path: Path):
    """Codex F-CURATOR-CR3 (P2): same claim, same tiers, same
    second, same sha — but different curators — must produce
    distinct event_ids. Each is an independent audit record."""
    (tmp_path / "a").mkdir()
    (tmp_path / "b").mkdir()
    manifest_path_a = _sample_manifest(tmp_path / "a")
    manifest_path_b = _sample_manifest(tmp_path / "b")
    r1 = promote_claim(
        manifest_path=manifest_path_a,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="rationale",
        curator="Alice",
        timestamp="2026-05-15T10:00:00Z",
    )
    r2 = promote_claim(
        manifest_path=manifest_path_b,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="rationale",
        curator="Bob",
        timestamp="2026-05-15T10:00:00Z",
    )
    assert r1.event_id != r2.event_id


def test_promote_two_distinct_claims_produces_distinct_event_ids(
    tmp_path: Path,
):
    manifest_path = _sample_manifest(tmp_path)
    r1 = promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="rationale one",
        curator="Jane",
        timestamp="2026-09-15T10:00:00Z",
    )
    r2 = promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-two",
        to_tier="ci",
        rationale="rationale two",
        curator="Jane",
        timestamp="2026-09-15T10:00:00Z",
    )
    assert r1.event_id != r2.event_id


# ---------------------------------------------------------------------
# drop_claim
# ---------------------------------------------------------------------


def test_drop_removes_claim_from_manifest(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    result = drop_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
    )
    assert result.remaining_claim_ids == ["test-claim-two"]
    parsed = yaml.safe_load(manifest_path.read_text())
    assert [c["id"] for c in parsed["claims"]] == ["test-claim-two"]


def test_drop_rejects_unknown_claim_id(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    with pytest.raises(CuratorError):
        drop_claim(
            manifest_path=manifest_path,
            claim_id="does-not-exist",
        )


def test_drop_does_not_write_sidecar(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)
    drop_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
    )
    assert not (tmp_path / "review_events.json").exists()


# ---------------------------------------------------------------------
# Cross-language integration with typed-trust PR3 validator
# ---------------------------------------------------------------------


def _typed_trust_binary() -> Path | None:
    repo_root = Path(__file__).resolve().parents[2]
    for p in (
        repo_root / "typed-trust" / "target" / "debug" / "typed-trust",
        repo_root / "typed-trust" / "target" / "release" / "typed-trust",
    ):
        if p.is_file():
            return p
    on_path = shutil.which("typed-trust")
    return Path(on_path) if on_path else None


@pytest.mark.skipif(
    _typed_trust_binary() is None,
    reason="typed-trust binary not built; run `cargo build` in typed-trust/",
)
def test_promoted_manifest_satisfies_typed_trust_validator(tmp_path: Path):
    """End-to-end: a manifest promoted via the curator tool must
    pass typed-trust's PR3 promotion validator at tier:ci. Proves
    the Python sidecar format + typed-trust translation are
    byte-compatible."""
    manifest_path = _sample_manifest(tmp_path)
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="Verified the bound is stated.",
        curator="Jane Doe <orcid:0000-0001-2345-6789>",
    )
    sidecar_path = tmp_path / "review_events.json"
    binary = _typed_trust_binary()
    assert binary is not None
    result = subprocess.run(
        [
            str(binary),
            str(manifest_path),
            "--review-events-sidecar",
            str(sidecar_path),
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, (
        f"typed-trust rejected the promoted manifest.\n"
        f"stderr:\n{result.stderr}\n"
        f"stdout:\n{result.stdout[:500]}"
    )


@pytest.mark.skipif(
    _typed_trust_binary() is None,
    reason="typed-trust binary not built",
)
def test_multi_step_promoted_manifest_satisfies_typed_trust_validator(
    tmp_path: Path,
):
    """End-to-end: research -> ci -> release manifest with two
    sidecar events must satisfy typed-trust's multi-step validator.
    Proves the Python curator tooling and the Rust chain validator
    are byte-compatible across two promotions."""
    manifest_path = _sample_manifest(tmp_path)
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="leg 1",
        curator="Jane Doe",
    )
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="release",
        rationale="leg 2",
        curator="Jane Doe",
    )
    sidecar_path = tmp_path / "review_events.json"
    binary = _typed_trust_binary()
    assert binary is not None
    result = subprocess.run(
        [
            str(binary),
            str(manifest_path),
            "--review-events-sidecar",
            str(sidecar_path),
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, (
        f"typed-trust rejected the multi-step promoted manifest.\n"
        f"stderr:\n{result.stderr}\n"
    )


@pytest.mark.skipif(
    _typed_trust_binary() is None,
    reason="typed-trust binary not built",
)
def test_release_tier_with_only_one_promotion_event_is_rejected(
    tmp_path: Path,
):
    """Codex note: the Python curator's ladder enforcement
    prevents a curator from CREATING this state, but the Rust
    validator must also reject it if the state appears on disk
    (hand-edited manifest, byzantine curator, etc.). Promote
    research → ci normally, then hand-edit the manifest to
    tier:release WITHOUT writing the second sidecar event.
    """
    manifest_path = _sample_manifest(tmp_path)
    promote_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        to_tier="ci",
        rationale="leg 1",
        curator="Jane",
    )
    # Hand-edit the manifest to claim tier:release without writing
    # the second sidecar event.
    body = manifest_path.read_text()
    body = body.replace("tier: ci", "tier: release", 1)
    manifest_path.write_text(body)

    binary = _typed_trust_binary()
    assert binary is not None
    result = subprocess.run(
        [
            str(binary),
            str(manifest_path),
            "--review-events-sidecar",
            str(tmp_path / "review_events.json"),
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode != 0
    assert "ci" in result.stderr and "release" in result.stderr, (
        f"expected error naming the missing ci→release leg, got: "
        f"{result.stderr}"
    )


@pytest.mark.skipif(
    _typed_trust_binary() is None,
    reason="typed-trust binary not built",
)
def test_unpromoted_extracted_claim_at_ci_tier_is_rejected(tmp_path: Path):
    """Sanity check that the gate fires: hand-edit tier:research →
    tier:ci on the extracted manifest WITHOUT writing a promotion
    event. Typed-trust must reject."""
    manifest_path = _sample_manifest(tmp_path)
    body = manifest_path.read_text()
    body = body.replace("tier: research", "tier: ci", 1)
    manifest_path.write_text(body)
    binary = _typed_trust_binary()
    assert binary is not None
    result = subprocess.run(
        [str(binary), str(manifest_path)],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode != 0
    assert "promote_from_extracted" in result.stderr or "extracted" in result.stderr
