"""Deterministic backing-claim construction (Phase 2b).

The model does **not** author backing-claim YAML directly. It reports
a *violation tuple* — `{target_criterion_id, metric, observed_value,
bound, comparator, citation}` — and the agent constructs the backing
claim from that tuple plus the target claim's manifest. This collapses
the entire class of trivial-pass bypasses (codex F-2B-2/4/5): the
model has no degree of freedom over the tolerance logic the validator
later checks.

Two pure functions:

- :func:`validate_contradiction` enforces that the violation actually
  contradicts the target claim's tolerance — same metric, same bound,
  same comparator, finite numeric observed_value that violates the
  bound, non-empty citation.

- :func:`build_backing_claim` synthesizes the backing claim dict from
  the validated violation. Tolerance is the *logical inverse* of the
  target's comparator (e.g., target `<` becomes backing `>=`); bound
  is the target's bound; `last_verified.value` is the violation's
  observed_value. Backing id is
  ``<target_id>-counter-<sha256[:8] of canonical violation>``.

Both functions are pure and side-effect-free so they can be tested
without spinning up the CLI or the API.
"""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import math
from typing import Any, Optional

# Comparators we accept on a target tolerance and that we know how to
# invert when building the backing claim's pass condition. Float
# equality (``=`` / ``!=``) is rejected for Phase 2b challenges — the
# typed model has no notion of approximate equality, so reflex against
# `=` would require new tolerance semantics. Defer to Phase 2c.
SUPPORTED_COMPARATORS = ("<", "<=", ">", ">=")

_LOGICAL_INVERSE: dict[str, str] = {
    "<": ">=",
    "<=": ">",
    ">": "<=",
    ">=": "<",
}


class ViolationRejected(Exception):
    """Raised by :func:`validate_contradiction` when the violation
    doesn't actually contradict the target tolerance. The CLI logs
    the message on stderr and skips the sidecar entry — never
    retries (this is semantic failure, not transport).
    """


def validate_contradiction(
    target_claim: dict[str, Any],
    target_criterion_id: str,
    violation: dict[str, Any],
) -> dict[str, Any]:
    """Confirm that ``violation`` contradicts ``target_claim``'s
    tolerance for ``target_criterion_id``. Returns the matched target
    tolerance dict on success; raises :class:`ViolationRejected`
    otherwise.

    Rules enforced (codex F-2B-2/F-2B-4):

    - ``target_criterion_id`` must match an actual tolerance in the
      target's ``tolerances`` list (no inventing criteria).
    - violation ``metric`` equals the target tolerance's metric
      (no metric drift).
    - violation ``comparator`` is the target tolerance's comparator
      (the agent inverts when synthesizing; the model reports the
      target's own op).
    - violation ``bound`` is numerically equal to the target
      tolerance's value (no threshold drift; trivial predicates like
      ``observed > 0`` against ``< 0.02`` rejected).
    - violation ``observed_value`` is a finite number, not NaN, inf,
      or a string.
    - violation ``observed_value`` *actually* violates the target
      tolerance (e.g., for comparator ``<`` with bound 0.02,
      observed must satisfy ``observed >= 0.02`` — i.e., NOT < 0.02).
    - violation ``citation`` is non-empty.
    - ``comparator`` is in :data:`SUPPORTED_COMPARATORS`.
    """
    # Find the matched tolerance entry.
    tolerances = target_claim.get("tolerances") or []
    matched: Optional[dict[str, Any]] = None
    for t in tolerances:
        if isinstance(t, dict) and t.get("metric") == target_criterion_id:
            matched = t
            break
    if matched is None:
        raise ViolationRejected(
            f"target_criterion_id {target_criterion_id!r} not found in target tolerances"
        )

    # Tolerance must be structured (op + value present, not prose-only).
    target_metric = matched.get("metric")
    target_op = matched.get("op")
    target_value = matched.get("value")
    if not (target_metric and target_op is not None and target_value is not None):
        raise ViolationRejected(
            f"target criterion {target_criterion_id!r} has no structured "
            f"tolerance (metric/op/value); cannot challenge"
        )
    if target_op not in SUPPORTED_COMPARATORS:
        raise ViolationRejected(
            f"target tolerance comparator {target_op!r} not in supported set "
            f"{SUPPORTED_COMPARATORS}; float-equality reflex deferred to 2c"
        )

    # Violation tuple fields.
    v_metric = violation.get("metric")
    v_observed = violation.get("observed_value")
    v_bound = violation.get("bound")
    v_comparator = violation.get("comparator")
    v_citation = violation.get("citation")

    if v_metric != target_metric:
        raise ViolationRejected(
            f"violation.metric {v_metric!r} does not match target tolerance "
            f"metric {target_metric!r} (metric drift)"
        )
    if v_comparator != target_op:
        raise ViolationRejected(
            f"violation.comparator {v_comparator!r} does not match target "
            f"tolerance op {target_op!r} (the agent inverts; report the target's own op)"
        )
    if not _numbers_equal(v_bound, target_value):
        raise ViolationRejected(
            f"violation.bound {v_bound!r} does not equal target tolerance value "
            f"{target_value!r} (threshold drift; trivial bounds rejected)"
        )

    # Observed value must be a finite number, not stringified, NaN, inf.
    if isinstance(v_observed, bool) or not isinstance(v_observed, (int, float)):
        raise ViolationRejected(
            f"violation.observed_value {v_observed!r} must be a numeric type, "
            f"not {type(v_observed).__name__}"
        )
    observed_float = float(v_observed)
    if not math.isfinite(observed_float):
        raise ViolationRejected(
            f"violation.observed_value {v_observed!r} must be finite (no NaN, inf, -inf)"
        )

    # Observed must actually violate the target tolerance.
    if not _observed_violates(observed_float, target_op, float(target_value)):
        raise ViolationRejected(
            f"violation.observed_value {observed_float} satisfies the target "
            f"tolerance ({observed_float} {target_op} {target_value}); no real "
            f"contradiction"
        )

    if not isinstance(v_citation, str) or not v_citation.strip():
        raise ViolationRejected("violation.citation must be a non-empty string")

    return matched


def build_backing_claim(
    target_claim: dict[str, Any],
    target_criterion_id: str,
    violation: dict[str, Any],
    *,
    timestamp: Optional[str] = None,
) -> dict[str, Any]:
    """Construct a backing claim YAML from the target + violation.

    Caller MUST run :func:`validate_contradiction` first; this
    function trusts its input. The backing claim:

    - Inherits ``source`` from the target.
    - Has a deterministic id ``<target_id>-counter-<short-hash>``
      where ``short-hash`` is the first 8 hex of sha256 over the
      canonical violation tuple.
    - Carries a single structured tolerance: target's metric,
      *inverted* comparator (e.g., target's ``<`` becomes backing
      ``>=``), and target's bound. With the violation's
      observed_value as ``last_verified.value``, the backing's
      criterion synthesizes to Pass (this is what makes the backing
      report sustain).
    - Reuses the target's ``evidence`` block — same artifact,
      command, oracle. The Challenge cites a row of the same
      artifact, not a different one.
    - Includes ``last_verified.date`` (today's date in UTC) AND
      ``last_verified.value``. Both fields are required for
      typed-trust's ``translate_last_verified`` to bind the
      observation to the criterion.
    - Is structurally a leaf: no ``review_events`` field, no
      ``challenge`` field (typed-trust rejects depth > 1).
    """
    metric = violation["metric"]
    observed = float(violation["observed_value"])
    bound = float(violation["bound"])
    comparator = violation["comparator"]
    citation = violation["citation"]

    target_id = target_claim["id"]
    inverse_op = _LOGICAL_INVERSE[comparator]
    short_hash = _violation_short_hash(target_id, target_criterion_id, violation)
    backing_id = f"{target_id}-counter-{short_hash}"

    date = timestamp[:10] if timestamp else dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d")

    backing: dict[str, Any] = {
        "id": backing_id,
        "title": f"Counter-evidence: {metric} violates {comparator} {bound}",
        "kind": "measurement",
        "tier": target_claim.get("tier", "ci"),
        "source": target_claim.get("source", "."),
        "claim": (
            f"Counter-evidence to {target_id}: {citation} reports "
            f"{metric} = {observed}, which {_violates_phrase(observed, comparator, bound)}."
        ),
        "tolerances": [
            {
                "metric": metric,
                "op": inverse_op,
                "value": bound,
                "prose": (
                    f"Counter-claim: the cited observed value {observed} satisfies "
                    f"{metric} {inverse_op} {bound}, i.e., violates the target's "
                    f"{metric} {comparator} {bound} tolerance."
                ),
            }
        ],
        "evidence": _backing_evidence_block(target_claim.get("evidence") or {}),
        "last_verified": {
            "date": date,
            "value": observed,
        },
    }
    return backing


# ---------- helpers ----------

def _numbers_equal(a: Any, b: Any) -> bool:
    """Numeric equality with int/float coercion. ``True``/``False`` are
    rejected as numeric here because Python's ``True == 1`` would
    otherwise let bool-typed bounds slip through."""
    if isinstance(a, bool) or isinstance(b, bool):
        return False
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return float(a) == float(b)
    return False


def _observed_violates(observed: float, op: str, bound: float) -> bool:
    """True iff ``observed`` does NOT satisfy ``op bound``. A
    contradicting observation by definition fails the target's
    tolerance.
    """
    if op == "<":
        return observed >= bound
    if op == "<=":
        return observed > bound
    if op == ">":
        return observed <= bound
    if op == ">=":
        return observed < bound
    return False  # unreachable; SUPPORTED_COMPARATORS restricts ops


def _violation_short_hash(
    target_id: str, target_criterion_id: str, violation: dict[str, Any]
) -> str:
    """First 8 hex chars of sha256 over the canonical-JSON violation
    tuple. Deterministic and collision-resistant for the backing-id
    namespace under a single target.
    """
    payload = {
        "target_id": target_id,
        "target_criterion_id": target_criterion_id,
        "metric": violation["metric"],
        "observed_value": float(violation["observed_value"]),
        "bound": float(violation["bound"]),
        "comparator": violation["comparator"],
        "citation": violation["citation"],
    }
    encoded = json.dumps(payload, sort_keys=False, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()[:8]


def _violates_phrase(observed: float, op: str, bound: float) -> str:
    """Render a human-readable phrase describing the violation, for
    the backing claim's ``claim`` text."""
    return (
        f"violates the target tolerance ({op} {bound}) because "
        f"{observed} is not {op} {bound}"
    )


def _backing_evidence_block(target_evidence: dict[str, Any]) -> dict[str, Any]:
    """Backing claim's evidence is the target's evidence — same
    artifact, command, oracle. The Challenge cites a row of that same
    artifact, so the backing claim is verifiable from the same source.
    """
    return {
        "oracle": list(target_evidence.get("oracle") or []),
        "command": target_evidence.get("command", ""),
        "artifact": target_evidence.get("artifact", ""),
    }


__all__ = [
    "SUPPORTED_COMPARATORS",
    "ViolationRejected",
    "validate_contradiction",
    "build_backing_claim",
]
