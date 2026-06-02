"""The six required fixtures from the Phase 2a plan.

Fixtures 1 and 2 (positive Endorse + negative BALL Dissent) require a
recorded API response — they're produced by the user kicking off the
real model run with ``--record``. The other four are pure-logic
fixtures and run in CI.

Mapping:

- 3. Unknown claim_id sidecar rejection — via the typed-trust binary.
- 4. Multi-criterion with one unsupported — exercises the prompt-side
     visibility of all criteria + the validation that accepts a
     Dissent naming a real criterion.
- 5. Truncated evidence missing relevant content — exercises the new
     ``reject_if_truncated_endorse_lacks_evidence`` validator.
- 6. Concurrent sidecar appends — already covered by
     ``test_review_sidecar.test_concurrent_appends_do_not_lose_entries``.

Fixtures 1 and 2 are tested via ``test_recorded_e2e`` (skipped when
no fixture file is present).
"""

from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from pathlib import Path
from textwrap import dedent
from typing import Any, Optional

import pytest

from evident_agent.review import (
    CHECK_KEYS,
    ReviewRejected,
    ReviewVerdict,
    call_review,
    reject_if_hallucinated_criterion,
    reject_if_truncated_endorse_lacks_evidence,
)
from evident_agent.review_sidecar import (
    ReviewAuthor,
    ReviewEventEntry,
    append_events,
)


@dataclass
class FakeBlock:
    type: str
    name: Optional[str] = None
    input: Optional[dict[str, Any]] = None


@dataclass
class FakeResponse:
    id: str
    content: list[FakeBlock]


class FakeMessages:
    def __init__(self, responses):
        self.responses = list(responses)

    def create(self, **_kwargs):
        item = self.responses.pop(0)
        if isinstance(item, Exception):
            raise item
        return item


class FakeClient:
    def __init__(self, responses):
        self.messages = FakeMessages(responses)


def _typed_trust_binary() -> Path:
    return (
        Path(__file__).resolve().parents[2]
        / "typed-trust"
        / "target"
        / "debug"
        / "typed-trust"
    )


# ============================================================
# Fixture 3 — Unknown claim_id sidecar rejection (end-to-end)
# ============================================================

def test_fixture3_unknown_claim_id_in_sidecar_exits_nonzero(tmp_path: Path) -> None:
    """Hand-crafted sidecar names a claim not in the manifest. typed-
    trust must exit 1 with the unknown id named in stderr.
    """
    binary = _typed_trust_binary()
    if not binary.is_file():
        pytest.skip(f"typed-trust binary not built at {binary}")

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
                title: t
                claim: c
                tolerances:
                  - metric: relative_error
                    op: "<"
                    value: 0.02
                    prose: stay under 2%
                evidence:
                  oracle: [Test]
                  command: "true"
                  artifact: out.json
            """
        ).strip()
        + "\n"
    )
    sidecar = tmp_path / "review_events.json"
    sidecar.write_text(
        json.dumps(
            {
                "events": [
                    {
                        "claim_id": "claim-NONEXISTENT",
                        "kind": "endorse",
                        "author": {
                            "kind": "model",
                            "name": "claude-opus-4-7",
                            "version": "20250101",
                        },
                        "rationale": "Rationale that's long enough to satisfy minimum length validation in 2a.",
                        "timestamp": "2026-06-02T10:31:44Z",
                    }
                ]
            }
        )
    )
    result = subprocess.run(
        [
            str(binary),
            "--format",
            "json",
            "--review-events-sidecar",
            str(sidecar),
            str(manifest),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode != 0
    assert "claim-NONEXISTENT" in result.stderr


# ============================================================
# Fixture 4 — Multi-criterion claim with one unsupported criterion
# ============================================================

def _multi_criterion_claim_raw() -> dict:
    return {
        "id": "claim-multi",
        "title": "Multi-criterion test claim",
        "kind": "measurement",
        "tier": "ci",
        "claim": "Both metrics must pass independently.",
        "tolerances": [
            {
                "metric": "metric_alpha",
                "op": "<",
                "value": 0.02,
                "prose": "alpha stays under 2%",
            },
            {
                "metric": "metric_beta",
                "op": "<",
                "value": 0.05,
                "prose": "beta stays under 5%",
            },
        ],
        "evidence": {
            "oracle": ["Test"],
            "command": "true",
            "artifact": "out.json",
        },
    }


def test_fixture4_dissent_naming_real_criterion_accepted(tmp_path: Path) -> None:
    """The model dissents and names ``metric_beta`` (a real criterion
    of the claim) as unsupported. Validation must accept it without
    flagging it as hallucinated."""
    verdict = ReviewVerdict(
        verdict="dissent",
        checks={
            "metric_present": "pass",
            "within_tolerance": "unknown",
            "outliers_checked": "pass",
            "reproducible_chain": "pass",
        },
        rationale=(
            "Digest contains metric_alpha = 0.01 but no metric_beta "
            "data. Cannot Endorse without coverage for every criterion."
        ),
        observed_value="0.01",
        tolerance="< 0.02",
        failure_reason="criterion `metric_beta` is not supported by the digest",
    )
    # No raise — metric_beta is real.
    reject_if_hallucinated_criterion(
        verdict, claim_criteria=["metric_alpha", "metric_beta"]
    )


def test_fixture4_dissent_naming_fake_criterion_rejected(tmp_path: Path) -> None:
    """The same prompt setup, but the model invents a criterion name.
    Validation rejects."""
    verdict = ReviewVerdict(
        verdict="dissent",
        checks={
            "metric_present": "unknown",
            "within_tolerance": "unknown",
            "outliers_checked": "pass",
            "reproducible_chain": "pass",
        },
        rationale="Cannot verify because the relevant data is missing for one of the criteria.",
        observed_value=None,
        tolerance=None,
        failure_reason="criterion `metric_gamma` not in digest",
    )
    with pytest.raises(ReviewRejected, match="metric_gamma"):
        reject_if_hallucinated_criterion(
            verdict, claim_criteria=["metric_alpha", "metric_beta"]
        )


# ============================================================
# Fixture 5 — Truncated evidence missing relevant content
# ============================================================

def test_fixture5_endorse_rejected_when_truncated_and_observation_absent() -> None:
    """Digest was truncated and the cited observed_value (``0.008``)
    is not in the digest body — the model was working blind."""
    verdict = ReviewVerdict(
        verdict="endorse",
        checks={k: "pass" for k in CHECK_KEYS},
        rationale=(
            "Digest shows the metric value within tolerance based on the "
            "headline number summary block in the artifact."
        ),
        observed_value="0.008",
        tolerance="< 0.02",
    )
    digest_body = "summary: rows=1000, errors=0\n...<truncated>...\n"
    with pytest.raises(ReviewRejected, match="not present in digest"):
        reject_if_truncated_endorse_lacks_evidence(
            verdict, digest_body, digest_truncated=True
        )


def test_fixture5_endorse_accepted_when_truncated_but_observation_present() -> None:
    """Truncated, but the cited value IS in the digest body — valid."""
    verdict = ReviewVerdict(
        verdict="endorse",
        checks={k: "pass" for k in CHECK_KEYS},
        rationale=(
            "The cited metric is visible at the boundary of the digest "
            "and stays within the stated tolerance band."
        ),
        observed_value="0.008",
        tolerance="< 0.02",
    )
    digest_body = "summary: median=0.008, errors=0\n...<truncated>...\n"
    # Must not raise.
    reject_if_truncated_endorse_lacks_evidence(
        verdict, digest_body, digest_truncated=True
    )


def test_fixture5_dissent_unaffected_by_truncation() -> None:
    """Dissent doesn't require an observed_value citation; the rule
    only constrains Endorse."""
    verdict = ReviewVerdict(
        verdict="dissent",
        checks={
            "metric_present": "unknown",
            "within_tolerance": "unknown",
            "outliers_checked": "unknown",
            "reproducible_chain": "unknown",
        },
        rationale="Cannot verify the cited metric in the truncated digest. Default to dissent per framing rules.",
        failure_reason="metric_present=unknown",
    )
    reject_if_truncated_endorse_lacks_evidence(verdict, "", digest_truncated=True)


# ============================================================
# Fixtures 1 + 2 — Recorded API responses (skip when absent)
# ============================================================

def _fixture_dir() -> Path:
    return Path(__file__).parent / "fixtures" / "review"


def test_recorded_endorse_e2e(tmp_path: Path) -> None:
    fixture = _fixture_dir() / "sasa_ci_endorse.json"
    if not fixture.is_file():
        pytest.skip(f"recorded endorse fixture not present at {fixture}")
    recorded = json.loads(fixture.read_text())
    response = FakeResponse(
        id=recorded.get("id", "msg_recorded"),
        content=[
            FakeBlock(
                type="tool_use",
                name="submit_review",
                input=recorded["tool_input"],
            )
        ],
    )
    client = FakeClient([response])
    verdict = call_review(
        model="claude-opus-4-7",
        claim_yaml="id: proteon-sasa-vs-biopython-release-1k-pdbs\n",
        digest_rendered=recorded.get("digest", "<digest></digest>"),
        api_client=client,
    )
    assert verdict.verdict == "endorse"
    # All four checks must read pass for an Endorse.
    assert all(verdict.checks[k] == "pass" for k in CHECK_KEYS)


def test_recorded_ball_dissent_e2e(tmp_path: Path) -> None:
    fixture = _fixture_dir() / "ball_electrostatic_dissent.json"
    if not fixture.is_file():
        pytest.skip(f"recorded BALL dissent fixture not present at {fixture}")
    recorded = json.loads(fixture.read_text())
    response = FakeResponse(
        id=recorded.get("id", "msg_recorded"),
        content=[
            FakeBlock(
                type="tool_use",
                name="submit_review",
                input=recorded["tool_input"],
            )
        ],
    )
    client = FakeClient([response])
    verdict = call_review(
        model="claude-opus-4-7",
        claim_yaml="id: ball-electrostatic-synthetic-dissent\n",
        digest_rendered=recorded.get("digest", "<digest></digest>"),
        api_client=client,
    )
    # The load-bearing 2a fixture: model must Dissent when the
    # digest is vague / insufficient — no specific contradicting
    # value to cite would warrant Challenge.
    assert verdict.verdict == "dissent"
    assert verdict.failure_reason
    assert any(verdict.checks[k] != "pass" for k in CHECK_KEYS)


def test_recorded_ball_challenge_e2e(tmp_path: Path) -> None:
    """Phase 2b's load-bearing recorded fixture: when the digest
    contains a specific observed value that violates the target
    tolerance, the model must escalate to Challenge with a violation
    tuple naming the exact metric / bound / comparator / observed
    value / citation.

    Skipped until the fixture is recorded via ``evident-agent review
    --record``.
    """
    fixture = _fixture_dir() / "ball_electrostatic_challenge.json"
    if not fixture.is_file():
        pytest.skip(f"recorded BALL challenge fixture not present at {fixture}")
    recorded = json.loads(fixture.read_text())
    response = FakeResponse(
        id=recorded.get("id", "msg_recorded"),
        content=[
            FakeBlock(
                type="tool_use",
                name="submit_review",
                input=recorded["tool_input"],
            )
        ],
    )
    client = FakeClient([response])
    verdict = call_review(
        model="claude-opus-4-7",
        claim_yaml="id: ball-electrostatic-synthetic-challenge\n",
        digest_rendered=recorded.get("digest", "<digest></digest>"),
        api_client=client,
    )
    assert verdict.verdict == "challenge", (
        f"expected challenge, got {verdict.verdict}; rationale: {verdict.rationale[:200]}"
    )
    # Substantive Challenge requires a violation tuple naming the
    # exact target metric and bound.
    assert verdict.challenge_target_criterion_id is not None
    assert verdict.challenge_violation is not None
    for required in ("metric", "observed_value", "bound", "comparator", "citation"):
        assert required in verdict.challenge_violation, (
            f"violation missing required field {required!r}"
        )

    # Phase 2b's load-bearing rule (codex F-2B-2): the violation MUST
    # actually contradict the target tolerance — same metric, same
    # bound, same comparator, finite numeric observed_value that
    # violates the bound. Without this assertion the fixture is just
    # a shape check; a regressed model that drifted on metric or bound
    # would pass CI.
    target_claim = _read_target_claim_for_fixture(
        "ball_challenge", "ball-electrostatic-synthetic-challenge"
    )
    validate_contradiction(
        target_claim,
        verdict.challenge_target_criterion_id,
        verdict.challenge_violation,
    )


def test_recorded_ball_challenge_e2e_catches_threshold_drift_codex_3_cr2(
    tmp_path: Path,
) -> None:
    """Codex F-CR3-2 regression: the strengthened Challenge fixture
    assertion must catch a model that drifts on the bound. Simulate a
    regression by mutating the fixture's reported bound; the test
    helper must now reject."""
    fixture = _fixture_dir() / "ball_electrostatic_challenge.json"
    if not fixture.is_file():
        pytest.skip(f"recorded BALL challenge fixture not present at {fixture}")
    recorded = json.loads(fixture.read_text())
    # Drift the bound to a value that doesn't match the target's 0.02.
    recorded["tool_input"]["challenge"]["violation"]["bound"] = 0.10
    target_claim = _read_target_claim_for_fixture(
        "ball_challenge", "ball-electrostatic-synthetic-challenge"
    )
    with pytest.raises(ViolationRejected, match="threshold drift"):
        validate_contradiction(
            target_claim,
            recorded["tool_input"]["challenge"]["target_criterion_id"],
            recorded["tool_input"]["challenge"]["violation"],
        )


def _read_target_claim_for_fixture(fixture_dir: str, claim_id: str) -> dict:
    """Load the target claim dict for a recorded fixture.

    Mirrors the agent's view of the adversarial fixture manifest so the
    CI assertion uses the same source of truth the model was reviewed
    against.
    """
    import yaml

    manifest_path = (
        Path(__file__).resolve().parent
        / "fixtures"
        / "adversarial"
        / fixture_dir
        / "evident.yaml"
    )
    parsed = yaml.safe_load(manifest_path.read_text())
    for c in parsed.get("claims", []):
        if c.get("id") == claim_id:
            return c
    raise AssertionError(
        f"target claim {claim_id!r} not found in {manifest_path}"
    )


# ============================================================
# Phase 2b — eight required fixtures
# ============================================================

from evident_agent.review import (
    ReviewRejected,
    ReviewVerdict,
    verdict_to_sidecar_entry,
)
from evident_agent.review_sidecar import append_events
from evident_agent.violation import (
    ViolationRejected,
    build_backing_claim,
    validate_contradiction,
)


def _target_claim_dict() -> dict:
    return {
        "id": "ball-electrostatic-ci",
        "title": "BALL electrostatic CI",
        "kind": "measurement",
        "tier": "ci",
        "source": ".",
        "claim": "electrostatic_error stays under tolerance",
        "tolerances": [
            {
                "metric": "electrostatic_error",
                "op": "<",
                "value": 0.02,
                "prose": "stay under 2%",
            }
        ],
        "evidence": {
            "oracle": ["BALL"],
            "command": "pytest",
            "artifact": "bench/electrostatic_results.csv",
        },
    }


# Fixture 2 — Trivial-pass violation rejected
def test_fixture2_trivial_pass_violation_rejected() -> None:
    """Codex F-2B-2 load-bearing case: `observed > 0` against target
    `< 0.02` is rejected at validate_contradiction."""
    bad = {
        "metric": "electrostatic_error",
        "observed_value": 1.0,
        "bound": 0.0,
        "comparator": "<",
        "citation": "x",
    }
    with pytest.raises(ViolationRejected, match="threshold drift"):
        validate_contradiction(_target_claim_dict(), "electrostatic_error", bad)


# Fixture 3 — Threshold-drift violation rejected
def test_fixture3_threshold_drift_violation_rejected() -> None:
    bad = {
        "metric": "electrostatic_error",
        "observed_value": 0.015,
        "bound": 0.01,
        "comparator": "<",
        "citation": "x",
    }
    with pytest.raises(ViolationRejected, match="threshold drift"):
        validate_contradiction(_target_claim_dict(), "electrostatic_error", bad)


# Fixture 4 — Metric-drift violation rejected
def test_fixture4_metric_drift_violation_rejected() -> None:
    bad = {
        "metric": "rmsd",
        "observed_value": 0.05,
        "bound": 0.02,
        "comparator": "<",
        "citation": "x",
    }
    with pytest.raises(ViolationRejected, match="metric drift"):
        validate_contradiction(_target_claim_dict(), "electrostatic_error", bad)


# Fixture 5 — Non-violating observation rejected
def test_fixture5_non_violating_observation_rejected() -> None:
    """observed_value satisfies the target tolerance, so no real
    contradiction exists."""
    bad = {
        "metric": "electrostatic_error",
        "observed_value": 0.008,
        "bound": 0.02,
        "comparator": "<",
        "citation": "row 1 of bench/electrostatic_results.csv",
    }
    with pytest.raises(ViolationRejected, match="no real"):
        validate_contradiction(_target_claim_dict(), "electrostatic_error", bad)


# Fixture 6 — Sustaining Challenge flips target to Contested (end-to-end)
def test_fixture6_sustaining_challenge_flips_target_to_contested(
    tmp_path: Path,
) -> None:
    """End-to-end through the typed-trust binary: hand-crafted sidecar
    with a valid violation, agent-built backing claim, target renders
    Contested with backing report Current + Pass."""
    binary = _typed_trust_binary()
    if not binary.is_file():
        pytest.skip(f"typed-trust binary not built at {binary}")

    target = _target_claim_dict()
    target["last_verified"] = {
        "commit": "abc",
        "date": "2026-05-01",
        "value": 0.008,
    }
    manifest = tmp_path / "evident.yaml"
    manifest.write_text(
        "version: 0.1\n"
        "project: test\n"
        "claims:\n"
        "  - id: ball-electrostatic-ci\n"
        "    kind: measurement\n"
        "    tier: ci\n"
        "    title: t\n"
        "    claim: c\n"
        "    tolerances:\n"
        "      - metric: electrostatic_error\n"
        "        op: \"<\"\n"
        "        value: 0.02\n"
        "        prose: stay under 2%\n"
        "    evidence:\n"
        "      oracle: [BALL]\n"
        "      command: \"true\"\n"
        "      artifact: bench/electrostatic_results.csv\n"
        "    last_verified:\n"
        "      commit: abc\n"
        "      date: 2026-05-01\n"
        "      value: 0.008\n"
    )

    verdict = ReviewVerdict(
        verdict="challenge",
        checks={
            "metric_present": "pass",
            "within_tolerance": "fail",
            "outliers_checked": "pass",
            "reproducible_chain": "pass",
        },
        rationale="Row 47 reports electrostatic_error 0.025, exceeding the 0.02 bound.",
        observed_value="0.025",
        tolerance="< 0.02",
        failure_reason="row 47 violates the upper bound on electrostatic_error",
        challenge_category="weak_statistics",
        challenge_target_criterion_id="electrostatic_error",
        challenge_violation={
            "metric": "electrostatic_error",
            "observed_value": 0.025,
            "bound": 0.02,
            "comparator": "<",
            "citation": "row 47 of bench/electrostatic_results.csv",
        },
        model="claude-opus-4-7",
    )
    entry = verdict_to_sidecar_entry(
        verdict,
        claim_id="ball-electrostatic-ci",
        author_name="claude-opus-4-7",
        author_version="20250101",
        target_claim=target,
    )
    sidecar = tmp_path / "review_events.json"
    append_events(sidecar, [entry])

    result = subprocess.run(
        [
            str(binary),
            "--format",
            "json",
            "--review-events-sidecar",
            str(sidecar),
            str(manifest),
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    bundle = json.loads(result.stdout)
    target_report = bundle["reports"][0]
    assert target_report["status"] == "contested", (
        f"expected target Contested, got {target_report['status']}; bundle: {bundle}"
    )
    backing_reports = target_report.get("_graph", {}).get("backing_reports", [])
    assert any(b["status"] == "current" for b in backing_reports), (
        "expected at least one backing report Current"
    )


# Fixture 7 — Procedural Challenge without backing
def test_fixture7_procedural_challenge_renders_without_backing(
    tmp_path: Path,
) -> None:
    binary = _typed_trust_binary()
    if not binary.is_file():
        pytest.skip(f"typed-trust binary not built at {binary}")

    manifest = tmp_path / "evident.yaml"
    manifest.write_text(
        "version: 0.1\n"
        "project: test\n"
        "claims:\n"
        "  - id: ball-electrostatic-ci\n"
        "    kind: measurement\n"
        "    tier: ci\n"
        "    title: t\n"
        "    claim: c\n"
        "    tolerances:\n"
        "      - metric: electrostatic_error\n"
        "        op: \"<\"\n"
        "        value: 0.02\n"
        "        prose: stay under 2%\n"
        "    evidence:\n"
        "      oracle: [BALL]\n"
        "      command: \"true\"\n"
        "      artifact: out.json\n"
    )
    sidecar_path = tmp_path / "review_events.json"
    sidecar_path.write_text(
        json.dumps(
            {
                "events": [
                    {
                        "claim_id": "ball-electrostatic-ci",
                        "kind": "challenge",
                        "author": {
                            "kind": "model",
                            "name": "claude-opus-4-7",
                            "version": "20250101",
                        },
                        "rationale": "Docker container fails to start; reproducibility blocked completely.",
                        "timestamp": "2026-06-02T10:31:44Z",
                        "challenge": {"category": "command_failure"},
                    }
                ]
            }
        )
    )
    result = subprocess.run(
        [
            str(binary),
            "--format",
            "json",
            "--review-events-sidecar",
            str(sidecar_path),
            str(manifest),
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    bundle = json.loads(result.stdout)
    target = bundle["reports"][0]
    # Procedural Challenge moves status to contested (without a
    # backing claim).
    assert target["status"] == "contested"
    # And there should be no backing reports.
    backing = target.get("_graph", {}).get("backing_reports", [])
    assert not backing, f"procedural challenge must not carry backing reports; got {backing}"


# Fixture 8 — Cycle / depth-2 backing rejected
def test_fixture8_backing_claim_id_matching_target_id_rejected(
    tmp_path: Path,
) -> None:
    binary = _typed_trust_binary()
    if not binary.is_file():
        pytest.skip(f"typed-trust binary not built at {binary}")

    manifest = tmp_path / "evident.yaml"
    manifest.write_text(
        "version: 0.1\n"
        "project: test\n"
        "claims:\n"
        "  - id: ball-electrostatic-ci\n"
        "    kind: measurement\n"
        "    tier: ci\n"
        "    title: t\n"
        "    claim: c\n"
        "    tolerances:\n"
        "      - metric: electrostatic_error\n"
        "        op: \"<\"\n"
        "        value: 0.02\n"
        "        prose: stay under 2%\n"
        "    evidence:\n"
        "      oracle: [BALL]\n"
        "      command: \"true\"\n"
        "      artifact: out.json\n"
    )
    # Hand-crafted sidecar with backing.id == target.id — one-step
    # cycle that typed-trust rejects at translation time.
    sidecar_path = tmp_path / "review_events.json"
    sidecar_path.write_text(
        json.dumps(
            {
                "events": [
                    {
                        "claim_id": "ball-electrostatic-ci",
                        "kind": "challenge",
                        "author": {
                            "kind": "model",
                            "name": "claude-opus-4-7",
                            "version": "20250101",
                        },
                        "rationale": "Row 47 reports 0.025 exceeding the 0.02 bound on electrostatic_error.",
                        "timestamp": "2026-06-02T10:31:44Z",
                        "challenge": {
                            "category": "weak_statistics",
                            "target_criterion_id": "electrostatic_error",
                            "violation": {
                                "metric": "electrostatic_error",
                                "observed_value": 0.025,
                                "bound": 0.02,
                                "comparator": "<",
                                "citation": "row 47",
                            },
                            "backing_claim": {
                                # Cycle: same id as the target.
                                "id": "ball-electrostatic-ci",
                                "title": "self-cycle",
                                "kind": "measurement",
                                "tier": "ci",
                                "source": ".",
                                "claim": "x",
                                "tolerances": [
                                    {
                                        "metric": "electrostatic_error",
                                        "op": ">=",
                                        "value": 0.02,
                                        "prose": "x",
                                    }
                                ],
                                "evidence": {
                                    "oracle": ["BALL"],
                                    "command": "true",
                                    "artifact": "x",
                                },
                                "last_verified": {
                                    "date": "2026-06-02",
                                    "value": 0.025,
                                },
                            },
                        },
                    }
                ]
            }
        )
    )
    result = subprocess.run(
        [
            str(binary),
            "--format",
            "json",
            "--review-events-sidecar",
            str(sidecar_path),
            str(manifest),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode != 0
    assert "matches the target" in result.stderr or "cycle" in result.stderr.lower()
