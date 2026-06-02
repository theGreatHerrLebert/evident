"""Tests for the deterministic backing-claim construction (Phase 2b).

Covers both halves of the contract:

- :func:`validate_contradiction` rejects every bypass class the codex
  review enumerated (F-2B-2, F-2B-4): metric drift, threshold drift,
  trivial bounds, non-numeric / stringified / NaN / inf
  observed_values, comparator unsupported, criterion not in target,
  observed value that doesn't actually contradict.

- :func:`build_backing_claim` produces a backing claim whose tolerance
  is the logical inverse of the target's, whose last_verified.value
  satisfies the inverse, and whose id is a deterministic hash of the
  violation.
"""

from __future__ import annotations

import math

import pytest

from evident_agent.violation import (
    SUPPORTED_COMPARATORS,
    ViolationRejected,
    build_backing_claim,
    validate_contradiction,
)


def _target_claim() -> dict:
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
                "prose": "stay under 2% relative error",
            }
        ],
        "evidence": {
            "oracle": ["BALL"],
            "command": "pytest tests/test_ball.py::test_electrostatic -v",
            "artifact": "bench/electrostatic_results.csv",
        },
    }


def _violation_valid() -> dict:
    return {
        "metric": "electrostatic_error",
        "observed_value": 0.025,
        "bound": 0.02,
        "comparator": "<",
        "citation": "row 47 of bench/electrostatic_results.csv",
    }


# ---------- happy path ----------

def test_validate_contradiction_passes_on_real_violation() -> None:
    matched = validate_contradiction(
        _target_claim(), "electrostatic_error", _violation_valid()
    )
    assert matched["metric"] == "electrostatic_error"
    assert matched["op"] == "<"
    assert matched["value"] == 0.02


def test_build_backing_claim_produces_passing_tolerance() -> None:
    backing = build_backing_claim(
        _target_claim(), "electrostatic_error", _violation_valid()
    )
    tol = backing["tolerances"][0]
    # Logical inverse of `<` is `>=`.
    assert tol["op"] == ">="
    assert tol["value"] == 0.02
    assert tol["metric"] == "electrostatic_error"
    # last_verified.value = observed; satisfies the inverse tolerance.
    assert backing["last_verified"]["value"] == 0.025
    assert backing["last_verified"]["value"] >= tol["value"]
    # Date is present so typed-trust's translate_last_verified binds.
    assert backing["last_verified"]["date"]
    # Inherits source + evidence from target.
    assert backing["source"] == "."
    assert backing["evidence"]["artifact"] == "bench/electrostatic_results.csv"


def test_backing_claim_id_is_deterministic_over_violation() -> None:
    target = _target_claim()
    v = _violation_valid()
    a = build_backing_claim(target, "electrostatic_error", v)
    b = build_backing_claim(target, "electrostatic_error", v)
    assert a["id"] == b["id"]
    assert a["id"].startswith("ball-electrostatic-ci-counter-")


def test_backing_claim_id_changes_when_violation_changes() -> None:
    target = _target_claim()
    a = build_backing_claim(target, "electrostatic_error", _violation_valid())
    other = _violation_valid()
    other["observed_value"] = 0.030
    b = build_backing_claim(target, "electrostatic_error", other)
    assert a["id"] != b["id"]


def test_backing_claim_is_structural_leaf() -> None:
    """Phase 2b backing claims must not carry review_events or a
    nested challenge — typed-trust rejects depth > 1.
    """
    backing = build_backing_claim(
        _target_claim(), "electrostatic_error", _violation_valid()
    )
    assert "review_events" not in backing
    assert "challenge" not in backing


# ---------- target_criterion_id checks ----------

def test_rejects_unknown_target_criterion_id() -> None:
    with pytest.raises(ViolationRejected, match="not found in target tolerances"):
        validate_contradiction(
            _target_claim(), "bogus_metric", _violation_valid()
        )


def test_rejects_prose_only_target_tolerance() -> None:
    target = _target_claim()
    target["tolerances"][0]["op"] = None
    target["tolerances"][0]["value"] = None
    with pytest.raises(ViolationRejected, match="no structured tolerance"):
        validate_contradiction(target, "electrostatic_error", _violation_valid())


# ---------- metric drift ----------

def test_rejects_metric_drift() -> None:
    v = _violation_valid()
    v["metric"] = "rmsd"
    with pytest.raises(ViolationRejected, match="metric drift"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


# ---------- comparator drift ----------

def test_rejects_comparator_drift() -> None:
    """Model must report the target's own comparator, not its inverse."""
    v = _violation_valid()
    v["comparator"] = ">="  # the inverse — wrong, must report target's `<`
    with pytest.raises(ViolationRejected, match="does not match target tolerance op"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


def test_rejects_unsupported_target_comparator() -> None:
    """Float-equality reflex is out of scope for 2b (F-2B-4)."""
    target = _target_claim()
    target["tolerances"][0]["op"] = "="
    v = _violation_valid()
    v["comparator"] = "="
    with pytest.raises(ViolationRejected, match="not in supported set"):
        validate_contradiction(target, "electrostatic_error", v)


# ---------- threshold drift ----------

def test_rejects_threshold_drift_loose() -> None:
    """Trivial bound like `> 0` against target `< 0.02` is rejected
    (F-2B-2 — the load-bearing fix)."""
    v = _violation_valid()
    v["bound"] = 0.0
    with pytest.raises(ViolationRejected, match="threshold drift"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


def test_rejects_threshold_drift_tight() -> None:
    v = _violation_valid()
    v["bound"] = 0.01
    with pytest.raises(ViolationRejected, match="threshold drift"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


# ---------- observed_value type / value ----------

def test_rejects_stringified_observed_value() -> None:
    v = _violation_valid()
    v["observed_value"] = "0.025"
    with pytest.raises(ViolationRejected, match="must be a numeric type"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


def test_rejects_bool_as_observed_value() -> None:
    """Python ``True == 1`` would otherwise let a bool slip through."""
    v = _violation_valid()
    v["observed_value"] = True
    with pytest.raises(ViolationRejected, match="must be a numeric type"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


def test_rejects_nan_observed_value() -> None:
    v = _violation_valid()
    v["observed_value"] = float("nan")
    with pytest.raises(ViolationRejected, match="must be finite"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


def test_rejects_inf_observed_value() -> None:
    v = _violation_valid()
    v["observed_value"] = float("inf")
    with pytest.raises(ViolationRejected, match="must be finite"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


# ---------- contradiction logic ----------

def test_rejects_observed_that_satisfies_target() -> None:
    """Model reports `observed = 0.008` against `< 0.02`. That's
    NOT a contradiction — observed satisfies the target tolerance."""
    v = _violation_valid()
    v["observed_value"] = 0.008
    with pytest.raises(ViolationRejected, match="no real"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


def test_observed_equal_to_bound_violates_strict_less_than() -> None:
    """Boundary case: `op: <` with `observed == bound` IS a
    contradiction (`0.02` does not satisfy `< 0.02`)."""
    v = _violation_valid()
    v["observed_value"] = 0.02
    matched = validate_contradiction(_target_claim(), "electrostatic_error", v)
    assert matched["op"] == "<"


def test_observed_equal_to_bound_does_not_violate_less_or_equal() -> None:
    """Boundary case: `op: <=` with `observed == bound` does NOT
    violate (0.02 satisfies <= 0.02)."""
    target = _target_claim()
    target["tolerances"][0]["op"] = "<="
    v = _violation_valid()
    v["comparator"] = "<="
    v["observed_value"] = 0.02
    with pytest.raises(ViolationRejected, match="no real"):
        validate_contradiction(target, "electrostatic_error", v)


# ---------- citation ----------

def test_rejects_empty_citation() -> None:
    v = _violation_valid()
    v["citation"] = ""
    with pytest.raises(ViolationRejected, match="non-empty"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


def test_rejects_non_string_citation() -> None:
    v = _violation_valid()
    v["citation"] = 42
    with pytest.raises(ViolationRejected, match="non-empty"):
        validate_contradiction(_target_claim(), "electrostatic_error", v)


# ---------- inverse-comparator semantics across all four ops ----------

@pytest.mark.parametrize(
    "target_op,inverse_op,observed,bound",
    [
        ("<", ">=", 0.025, 0.02),  # 0.025 violates < 0.02
        ("<=", ">", 0.03, 0.02),  # 0.03 violates <= 0.02
        (">", "<=", 0.005, 0.02),  # 0.005 violates > 0.02
        (">=", "<", 0.005, 0.02),  # 0.005 violates >= 0.02
    ],
)
def test_inverse_comparator_for_each_supported_op(
    target_op: str, inverse_op: str, observed: float, bound: float
) -> None:
    target = _target_claim()
    target["tolerances"][0]["op"] = target_op
    target["tolerances"][0]["value"] = bound

    v = _violation_valid()
    v["comparator"] = target_op
    v["bound"] = bound
    v["observed_value"] = observed

    validate_contradiction(target, "electrostatic_error", v)
    backing = build_backing_claim(target, "electrostatic_error", v)
    assert backing["tolerances"][0]["op"] == inverse_op
    assert backing["last_verified"]["value"] == observed
