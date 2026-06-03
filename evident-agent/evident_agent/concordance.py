"""Concordance comparator primitives.

Per the v4 design (``EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md``),
the framework owns the comparator vocabulary. The author writes a
declaration ("this is a ``numeric_band`` concordance with these
parameters") and the docker artifact produces a JSON file at
``evidence.artifact``. This module reads the artifact, dispatches
on the declared ``pattern_kind``, and emits a ``ConcordanceResult``.

The five primitives:

- ``numeric_band``: measured value within ±epsilon of prior_value
- ``relative_band``: measured value within [prior/ratio, prior*ratio]
- ``same_order_of_magnitude``: floor(log10(measured)) == floor(log10(prior))
- ``ordinal_match``: per-entity ranking matches prior's ranking
- ``monotone_with``: measured series, when sorted by parameter
  series, is monotone in ``direction``

Each comparator returns ``Pass / Fail / NotAssessed`` plus an
auditable diagnostics block. Unit-mismatch (artifact's ``_unit`` vs
manifest's ``prior_unit``) is a ``NotAssessed`` outcome with a
specific diagnostic so the curator sees what went wrong.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Any, Optional


ComparisonStatus = str  # "pass" | "fail" | "not_assessed"


@dataclass
class ConcordanceResult:
    """Result of running a comparator on a docker artifact.

    Mirrors the ``last_concorded.json`` sidecar shape that this
    object is later serialized to. The dataclass is intentionally
    permissive — fields populated depend on the pattern_kind:

    - ``numeric_band`` / ``relative_band`` / ``same_order_of_magnitude``
      populate ``observed_value`` (scalar).
    - ``ordinal_match`` populates ``observed_ordering`` /
      ``prior_ordering``.
    - ``monotone_with`` populates ``observed_series`` /
      ``parameter_series``.

    All comparators populate ``comparison_status`` and
    ``diagnostics``.
    """

    comparison_status: ComparisonStatus
    diagnostics: dict = field(default_factory=dict)
    # Scalar primitives.
    observed_value: Optional[float] = None
    observed_unit: Optional[str] = None
    # ordinal_match.
    observed_ordering: Optional[list[str]] = None
    prior_ordering: Optional[list[str]] = None
    # monotone_with.
    observed_series: Optional[list[float]] = None
    parameter_series: Optional[list[float]] = None
    # Provenance.
    image_digest: Optional[str] = None
    produced_at: Optional[str] = None

    def to_dict(self) -> dict:
        out: dict = {"comparison_status": self.comparison_status}
        if self.observed_value is not None:
            out["observed_value"] = self.observed_value
        if self.observed_unit is not None:
            out["observed_unit"] = self.observed_unit
        if self.observed_ordering is not None:
            out["observed_ordering"] = list(self.observed_ordering)
        if self.prior_ordering is not None:
            out["prior_ordering"] = list(self.prior_ordering)
        if self.observed_series is not None:
            out["observed_series"] = list(self.observed_series)
        if self.parameter_series is not None:
            out["parameter_series"] = list(self.parameter_series)
        if self.image_digest is not None:
            out["image_digest"] = self.image_digest
        if self.produced_at is not None:
            out["produced_at"] = self.produced_at
        if self.diagnostics:
            out["diagnostics"] = dict(self.diagnostics)
        return out


class ConcordanceError(Exception):
    """The comparator cannot proceed (malformed manifest or
    unrecognized pattern_kind).

    Distinct from ``ConcordanceResult`` with ``comparison_status:
    not_assessed`` — that's the comparator's normal way to say
    "the inputs are well-formed but the check can't be made"
    (missing metric_path, unit mismatch). ``ConcordanceError`` is
    for "the manifest itself is bad."
    """


# ---------------------------------------------------------------------
# Public dispatch
# ---------------------------------------------------------------------


def evaluate(artifact: dict, concordance_block: dict) -> ConcordanceResult:
    """Run the concordance comparator against the docker artifact.

    ``concordance_block`` is the manifest's ``concordance:`` block
    after YAML parsing (a plain dict). ``artifact`` is the JSON the
    docker image wrote at ``evidence.artifact``. Provenance fields
    on the artifact (``artifact_provenance.image_digest``,
    ``artifact_provenance.produced_at``) are copied through into
    the result for the sidecar.

    Dispatches on ``concordance.pattern.pattern_kind`` (the
    discriminator the typed-trust translator parses).
    """
    pattern = concordance_block.get("pattern") or {}
    pattern_kind = pattern.get("pattern_kind")
    if pattern_kind is None:
        raise ConcordanceError("concordance.pattern.pattern_kind missing")

    prior_binding = concordance_block.get("prior_binding") or {}
    prior_unit = prior_binding.get("prior_unit")

    if pattern_kind == "numeric_band":
        result = _evaluate_numeric_band(artifact, pattern, prior_unit)
    elif pattern_kind == "relative_band":
        result = _evaluate_relative_band(artifact, pattern, prior_unit)
    elif pattern_kind == "same_order_of_magnitude":
        result = _evaluate_same_order_of_magnitude(artifact, pattern, prior_unit)
    elif pattern_kind == "ordinal_match":
        result = _evaluate_ordinal_match(artifact, pattern)
    elif pattern_kind == "monotone_with":
        result = _evaluate_monotone_with(artifact, pattern)
    else:
        raise ConcordanceError(
            f"unknown pattern_kind {pattern_kind!r}"
        )

    # Propagate artifact provenance into every result.
    prov = artifact.get("artifact_provenance") or {}
    result.image_digest = prov.get("image_digest")
    result.produced_at = prov.get("produced_at")
    return result


# ---------------------------------------------------------------------
# Per-primitive comparators
# ---------------------------------------------------------------------


def _evaluate_numeric_band(
    artifact: dict, pattern: dict, prior_unit: Optional[str],
) -> ConcordanceResult:
    metric_path = pattern.get("metric_path")
    prior_value = pattern.get("prior_value")
    epsilon = pattern.get("epsilon")
    if metric_path is None or prior_value is None or epsilon is None:
        raise ConcordanceError(
            "numeric_band requires metric_path, prior_value, epsilon"
        )
    leaf, observed_unit, missing_reason = _resolve_leaf(artifact, metric_path)
    if missing_reason is not None:
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={"reason": missing_reason, "metric_path": metric_path},
        )
    if not isinstance(leaf, (int, float)):
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={
                "reason": "metric_path resolved to non-numeric value",
                "metric_path": metric_path,
                "resolved_type": type(leaf).__name__,
            },
        )
    if not _units_match(prior_unit, observed_unit):
        return ConcordanceResult(
            comparison_status="not_assessed",
            observed_value=float(leaf),
            observed_unit=observed_unit,
            diagnostics={
                "reason": "unit_mismatch",
                "prior_unit": prior_unit,
                "observed_unit": observed_unit,
            },
        )
    delta = float(leaf) - float(prior_value)
    within = abs(delta) <= epsilon
    return ConcordanceResult(
        comparison_status="pass" if within else "fail",
        observed_value=float(leaf),
        observed_unit=observed_unit,
        diagnostics={
            "delta_from_prior": delta,
            "epsilon": epsilon,
            "within_band": within,
        },
    )


def _evaluate_relative_band(
    artifact: dict, pattern: dict, prior_unit: Optional[str],
) -> ConcordanceResult:
    metric_path = pattern.get("metric_path")
    prior_value = pattern.get("prior_value")
    ratio = pattern.get("ratio")
    if metric_path is None or prior_value is None or ratio is None:
        raise ConcordanceError(
            "relative_band requires metric_path, prior_value, ratio"
        )
    leaf, observed_unit, missing_reason = _resolve_leaf(artifact, metric_path)
    if missing_reason is not None:
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={"reason": missing_reason, "metric_path": metric_path},
        )
    if not isinstance(leaf, (int, float)):
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={
                "reason": "metric_path resolved to non-numeric value",
                "metric_path": metric_path,
            },
        )
    if not _units_match(prior_unit, observed_unit):
        return ConcordanceResult(
            comparison_status="not_assessed",
            observed_value=float(leaf),
            observed_unit=observed_unit,
            diagnostics={
                "reason": "unit_mismatch",
                "prior_unit": prior_unit,
                "observed_unit": observed_unit,
            },
        )
    lower = float(prior_value) / float(ratio)
    upper = float(prior_value) * float(ratio)
    within = lower <= float(leaf) <= upper
    return ConcordanceResult(
        comparison_status="pass" if within else "fail",
        observed_value=float(leaf),
        observed_unit=observed_unit,
        diagnostics={
            "ratio": ratio,
            "lower_bound": lower,
            "upper_bound": upper,
            "within_band": within,
        },
    )


def _evaluate_same_order_of_magnitude(
    artifact: dict, pattern: dict, prior_unit: Optional[str],
) -> ConcordanceResult:
    metric_path = pattern.get("metric_path")
    prior_value = pattern.get("prior_value")
    zero_policy = pattern.get("zero_policy", "not_assessed")
    if metric_path is None or prior_value is None:
        raise ConcordanceError(
            "same_order_of_magnitude requires metric_path, prior_value"
        )
    leaf, observed_unit, missing_reason = _resolve_leaf(artifact, metric_path)
    if missing_reason is not None:
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={"reason": missing_reason, "metric_path": metric_path},
        )
    if not isinstance(leaf, (int, float)):
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={
                "reason": "metric_path resolved to non-numeric value",
                "metric_path": metric_path,
            },
        )
    if leaf <= 0:
        if zero_policy == "reject":
            return ConcordanceResult(
                comparison_status="fail",
                observed_value=float(leaf),
                observed_unit=observed_unit,
                diagnostics={"reason": "observed_non_positive", "policy": "reject"},
            )
        return ConcordanceResult(
            comparison_status="not_assessed",
            observed_value=float(leaf),
            observed_unit=observed_unit,
            diagnostics={"reason": "observed_non_positive", "policy": zero_policy},
        )
    if not _units_match(prior_unit, observed_unit):
        return ConcordanceResult(
            comparison_status="not_assessed",
            observed_value=float(leaf),
            observed_unit=observed_unit,
            diagnostics={
                "reason": "unit_mismatch",
                "prior_unit": prior_unit,
                "observed_unit": observed_unit,
            },
        )
    observed_oom = math.floor(math.log10(float(leaf)))
    prior_oom = math.floor(math.log10(float(prior_value)))
    match = observed_oom == prior_oom
    return ConcordanceResult(
        comparison_status="pass" if match else "fail",
        observed_value=float(leaf),
        observed_unit=observed_unit,
        diagnostics={
            "observed_oom": observed_oom,
            "prior_oom": prior_oom,
            "match": match,
        },
    )


def _evaluate_ordinal_match(artifact: dict, pattern: dict) -> ConcordanceResult:
    entity_to_path = pattern.get("entity_to_path") or {}
    direction = pattern.get("direction")
    tie_policy = pattern.get("tie_policy", "strict")
    prior_value = pattern.get("prior_value") or {}
    if not entity_to_path or direction is None:
        raise ConcordanceError(
            "ordinal_match requires entity_to_path, direction"
        )
    if set(entity_to_path.keys()) != set(prior_value.keys()):
        # The translator already enforces this at translate time,
        # but defend at runtime in case a manifest fell through
        # (e.g., agent constructed the block in-memory).
        raise ConcordanceError(
            "entity_to_path keyset != prior_value keyset"
        )

    # Resolve each entity's measured value from the artifact.
    measured: dict[str, float] = {}
    for entity, path in entity_to_path.items():
        leaf, _, missing_reason = _resolve_leaf(artifact, path)
        if missing_reason is not None or not isinstance(leaf, (int, float)):
            return ConcordanceResult(
                comparison_status="not_assessed",
                diagnostics={
                    "reason": "ordinal_match metric_path unresolved",
                    "entity": entity,
                    "metric_path": path,
                    "inner_reason": missing_reason
                    or "non-numeric value",
                },
            )
        measured[entity] = float(leaf)

    # The "expected" ordering is the prior_value entries sorted by
    # their value under `direction`; the observed ordering is the
    # measured entries sorted the same way. Then compare.
    reverse = direction == "higher_is_better"
    prior_ordering = sorted(prior_value.keys(), key=lambda k: prior_value[k], reverse=reverse)
    observed_ordering = sorted(measured.keys(), key=lambda k: measured[k], reverse=reverse)

    if observed_ordering == prior_ordering:
        verdict = "pass"
    elif tie_policy == "adjacent_swap_ok" and _differs_by_one_adjacent_swap(
        observed_ordering, prior_ordering
    ):
        verdict = "pass"
    else:
        verdict = "fail"

    return ConcordanceResult(
        comparison_status=verdict,
        observed_ordering=observed_ordering,
        prior_ordering=prior_ordering,
        diagnostics={
            "direction": direction,
            "tie_policy": tie_policy,
            "measured_values": measured,
            "prior_values": dict(prior_value),
        },
    )


def _evaluate_monotone_with(artifact: dict, pattern: dict) -> ConcordanceResult:
    metric_path = pattern.get("metric_path")
    parameter_path = pattern.get("parameter_path")
    direction = pattern.get("direction")
    if metric_path is None or parameter_path is None or direction is None:
        raise ConcordanceError(
            "monotone_with requires metric_path, parameter_path, direction"
        )
    metric_leaf, _, m_miss = _resolve_leaf(artifact, metric_path)
    parameter_leaf, _, p_miss = _resolve_leaf(artifact, parameter_path)
    if m_miss is not None or p_miss is not None:
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={
                "reason": "monotone_with series unresolved",
                "metric_path_resolved": m_miss is None,
                "parameter_path_resolved": p_miss is None,
            },
        )
    if not isinstance(metric_leaf, list) or not isinstance(parameter_leaf, list):
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={
                "reason": "monotone_with requires both paths to resolve to lists",
                "metric_type": type(metric_leaf).__name__,
                "parameter_type": type(parameter_leaf).__name__,
            },
        )
    if len(metric_leaf) != len(parameter_leaf):
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={
                "reason": "monotone_with: series length mismatch",
                "metric_len": len(metric_leaf),
                "parameter_len": len(parameter_leaf),
            },
        )
    if any(not isinstance(v, (int, float)) for v in metric_leaf):
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={"reason": "metric series contains non-numeric values"},
        )
    if any(not isinstance(v, (int, float)) for v in parameter_leaf):
        return ConcordanceResult(
            comparison_status="not_assessed",
            diagnostics={"reason": "parameter series contains non-numeric values"},
        )
    # Sort metric by parameter; check monotone in `direction`.
    paired = sorted(
        zip([float(p) for p in parameter_leaf], [float(m) for m in metric_leaf]),
        key=lambda pm: pm[0],
    )
    sorted_metric = [m for _, m in paired]
    sorted_param = [p for p, _ in paired]
    monotone = _is_monotone(sorted_metric, direction)
    return ConcordanceResult(
        comparison_status="pass" if monotone else "fail",
        observed_series=sorted_metric,
        parameter_series=sorted_param,
        diagnostics={"direction": direction, "monotone": monotone},
    )


# ---------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------


def _resolve_leaf(
    artifact: dict, dotted_path: str,
) -> tuple[Any, Optional[str], Optional[str]]:
    """Walk ``dotted_path`` into ``artifact`` and return
    ``(leaf_value, unit, missing_reason)``.

    ``unit`` is the sibling ``_unit`` field at the leaf's parent
    dict (the artifact contract recommends carrying ``_unit`` per
    leaf so unit-mismatches can be caught). ``missing_reason`` is
    ``None`` on success or a short string when the path is absent.
    """
    cur: Any = artifact
    parts = dotted_path.split(".") if dotted_path else []
    for i, part in enumerate(parts):
        if not isinstance(cur, dict):
            return None, None, f"path component {parts[i - 1]!r} did not resolve to a dict"
        if part not in cur:
            return None, None, f"path component {part!r} not present in artifact"
        cur = cur[part]
    # Try to read the unit off the parent dict at the leaf.
    unit: Optional[str] = None
    parent: Any = artifact
    for part in parts[:-1]:
        parent = parent.get(part) if isinstance(parent, dict) else None
    if isinstance(parent, dict):
        unit = parent.get("_unit") if isinstance(parent.get("_unit"), str) else None
    return cur, unit, None


def _units_match(prior_unit: Optional[str], observed_unit: Optional[str]) -> bool:
    """Treat a missing observed unit as "trust the curator" (the
    artifact contract says ``_unit`` SHOULD be present but doesn't
    require it). If both are present, they MUST match exactly —
    no implicit conversions."""
    if observed_unit is None:
        return True
    if prior_unit is None:
        return True
    return prior_unit == observed_unit


def _differs_by_one_adjacent_swap(a: list, b: list) -> bool:
    """True iff ``a`` and ``b`` differ by exactly one adjacent
    swap. Both must be the same length and same multiset."""
    if len(a) != len(b) or sorted(a) != sorted(b):
        return False
    diffs = [i for i in range(len(a)) if a[i] != b[i]]
    if len(diffs) != 2:
        return False
    i, j = diffs
    return j == i + 1 and a[i] == b[j] and a[j] == b[i]


def _is_monotone(series: list[float], direction: str) -> bool:
    if direction == "increasing":
        return all(series[i] <= series[i + 1] for i in range(len(series) - 1))
    if direction == "decreasing":
        return all(series[i] >= series[i + 1] for i in range(len(series) - 1))
    return False
