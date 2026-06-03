"""PR5g: tests for the concordance comparator + last_concorded sidecar.

Covers each of the five primitives' pass / fail / not_assessed
outcomes, plus the sidecar's read / write / merge cycle.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from evident_agent import concordance, last_concorded


# ---------------------------------------------------------------------
# numeric_band
# ---------------------------------------------------------------------


def _band_block(prior=1.5, epsilon=0.5, metric_path="fragpipe.hla_10k.fdr_pct"):
    return {
        "pattern": {
            "pattern_kind": "numeric_band",
            "metric_path": metric_path,
            "prior_value": prior,
            "epsilon": epsilon,
        },
        "paper_locator": "src.md",
        "prior_binding": {
            "prior_unit": "percentage_points",
            "prior_metric_definition": "Empirical true FDR.",
            "locator": "Meier 2024 Table 3",
            "prior_extraction_note": "Curator verified",
            "source_id": "doi:test",
        },
    }


def _artifact_band(value, unit="percentage_points"):
    return {
        "artifact_schema_version": "1",
        "artifact_provenance": {
            "image_digest": "sha256:abc",
            "produced_at": "2026-06-04T10:00:00Z",
        },
        "fragpipe": {
            "hla_10k": {"fdr_pct": value, "_unit": unit},
        },
    }


def test_numeric_band_pass_within_epsilon():
    result = concordance.evaluate(_artifact_band(1.6), _band_block())
    assert result.comparison_status == "pass"
    assert result.observed_value == 1.6
    assert result.observed_unit == "percentage_points"
    assert result.diagnostics["within_band"] is True
    # Provenance copied through.
    assert result.image_digest == "sha256:abc"


def test_numeric_band_fail_outside_epsilon():
    result = concordance.evaluate(_artifact_band(2.5), _band_block())
    assert result.comparison_status == "fail"
    assert result.diagnostics["within_band"] is False


def test_numeric_band_not_assessed_when_metric_path_missing():
    art = {"artifact_schema_version": "1", "fragpipe": {"hla_10k": {}}}
    result = concordance.evaluate(art, _band_block())
    assert result.comparison_status == "not_assessed"
    assert "not present" in result.diagnostics["reason"]


def test_numeric_band_not_assessed_on_unit_mismatch():
    result = concordance.evaluate(
        _artifact_band(1.6, unit="fraction_unit_interval"), _band_block(),
    )
    assert result.comparison_status == "not_assessed"
    assert result.diagnostics["reason"] == "unit_mismatch"


def test_numeric_band_passes_when_observed_unit_absent_curator_trusted():
    art = {
        "artifact_schema_version": "1",
        "fragpipe": {"hla_10k": {"fdr_pct": 1.6}},  # no _unit
    }
    result = concordance.evaluate(art, _band_block())
    assert result.comparison_status == "pass"


# ---------------------------------------------------------------------
# relative_band
# ---------------------------------------------------------------------


def _relband_block(prior=100.0, ratio=2.0, metric_path="runtime.ms"):
    return {
        "pattern": {
            "pattern_kind": "relative_band",
            "metric_path": metric_path,
            "prior_value": prior,
            "ratio": ratio,
        },
        "paper_locator": "src.md",
        "prior_binding": {
            "prior_unit": "ms",
            "prior_metric_definition": "Total runtime in ms.",
            "locator": "Prior fig 4",
            "prior_extraction_note": "x",
            "source_id": "doi:test",
        },
    }


def test_relative_band_pass_inside_factor_2():
    art = {"runtime": {"ms": 180.0, "_unit": "ms"}}
    result = concordance.evaluate(art, _relband_block())
    assert result.comparison_status == "pass"
    assert result.diagnostics["lower_bound"] == 50.0
    assert result.diagnostics["upper_bound"] == 200.0


def test_relative_band_fail_outside_factor_2():
    art = {"runtime": {"ms": 250.0, "_unit": "ms"}}
    result = concordance.evaluate(art, _relband_block())
    assert result.comparison_status == "fail"


# ---------------------------------------------------------------------
# same_order_of_magnitude
# ---------------------------------------------------------------------


def _oom_block(prior=1500.0, zero_policy="not_assessed"):
    return {
        "pattern": {
            "pattern_kind": "same_order_of_magnitude",
            "metric_path": "count.total",
            "prior_value": prior,
            "zero_policy": zero_policy,
        },
        "paper_locator": "src.md",
        "prior_binding": {
            "prior_unit": "count",
            "prior_metric_definition": "Identified peptides",
            "locator": "x",
            "prior_extraction_note": "x",
            "source_id": "doi:test",
        },
    }


def test_same_order_pass_when_oom_matches():
    art = {"count": {"total": 2500.0, "_unit": "count"}}
    result = concordance.evaluate(art, _oom_block())
    assert result.comparison_status == "pass"
    assert result.diagnostics["observed_oom"] == 3
    assert result.diagnostics["prior_oom"] == 3


def test_same_order_fail_when_oom_differs():
    art = {"count": {"total": 250.0, "_unit": "count"}}
    result = concordance.evaluate(art, _oom_block())
    assert result.comparison_status == "fail"


def test_same_order_not_assessed_on_non_positive_with_default_policy():
    art = {"count": {"total": 0.0, "_unit": "count"}}
    result = concordance.evaluate(art, _oom_block())
    assert result.comparison_status == "not_assessed"
    assert result.diagnostics["reason"] == "observed_non_positive"


def test_same_order_fail_on_non_positive_with_reject_policy():
    art = {"count": {"total": -10.0, "_unit": "count"}}
    result = concordance.evaluate(art, _oom_block(zero_policy="reject"))
    assert result.comparison_status == "fail"


# ---------------------------------------------------------------------
# ordinal_match
# ---------------------------------------------------------------------


def _ordinal_block(direction="lower_is_better", tie_policy="strict"):
    return {
        "pattern": {
            "pattern_kind": "ordinal_match",
            "entity_to_path": {
                "FragPipe_v22": "fragpipe.hla_10k.fdr_pct",
                "PEAKS_XPro": "peaks_xpro.hla_10k.fdr_pct",
                "MaxQuant": "maxquant.hla_10k.fdr_pct",
            },
            "direction": direction,
            "tie_policy": tie_policy,
            "prior_value": {
                "FragPipe_v22": 1.5,
                "PEAKS_XPro": 1.8,
                "MaxQuant": 2.4,
            },
        },
        "paper_locator": "src.md",
        "prior_binding": {
            "prior_unit": "percentage_points",
            "prior_metric_definition": "FDR per Meier 2024",
            "locator": "Table 3",
            "prior_extraction_note": "Curator confirmed",
            "source_id": "doi:test",
        },
    }


def _ordinal_artifact(fragpipe, peaks, maxquant):
    return {
        "fragpipe": {"hla_10k": {"fdr_pct": fragpipe}},
        "peaks_xpro": {"hla_10k": {"fdr_pct": peaks}},
        "maxquant": {"hla_10k": {"fdr_pct": maxquant}},
    }


def test_ordinal_match_pass_when_ordering_identical():
    art = _ordinal_artifact(1.4, 1.9, 2.3)
    result = concordance.evaluate(art, _ordinal_block())
    assert result.comparison_status == "pass"
    assert result.observed_ordering == ["FragPipe_v22", "PEAKS_XPro", "MaxQuant"]


def test_ordinal_match_fail_when_ordering_reversed():
    art = _ordinal_artifact(2.4, 1.8, 1.5)
    result = concordance.evaluate(art, _ordinal_block())
    assert result.comparison_status == "fail"
    assert result.observed_ordering == ["MaxQuant", "PEAKS_XPro", "FragPipe_v22"]


def test_ordinal_match_pass_under_adjacent_swap_policy():
    # FragPipe and PEAKS adjacent-swap.
    art = _ordinal_artifact(1.9, 1.5, 2.4)
    result = concordance.evaluate(
        art, _ordinal_block(tie_policy="adjacent_swap_ok"),
    )
    assert result.comparison_status == "pass"


def test_ordinal_match_higher_is_better_direction():
    # Identification rate — higher is better.
    block = _ordinal_block(direction="higher_is_better")
    block["pattern"]["prior_value"] = {
        "ToolA": 50.0, "ToolB": 40.0, "ToolC": 30.0,
    }
    block["pattern"]["entity_to_path"] = {
        "ToolA": "a.rate",
        "ToolB": "b.rate",
        "ToolC": "c.rate",
    }
    art = {
        "a": {"rate": 52.0}, "b": {"rate": 42.0}, "c": {"rate": 33.0},
    }
    result = concordance.evaluate(art, block)
    assert result.comparison_status == "pass"
    # Highest first under higher_is_better.
    assert result.observed_ordering == ["ToolA", "ToolB", "ToolC"]


def test_ordinal_match_keyset_mismatch_raises():
    block = _ordinal_block()
    block["pattern"]["prior_value"]["NewTool"] = 5.0
    art = _ordinal_artifact(1.4, 1.9, 2.3)
    with pytest.raises(concordance.ConcordanceError) as exc:
        concordance.evaluate(art, block)
    assert "keyset" in str(exc.value)


# ---------------------------------------------------------------------
# monotone_with
# ---------------------------------------------------------------------


def _monotone_block(direction="decreasing"):
    return {
        "pattern": {
            "pattern_kind": "monotone_with",
            "metric_path": "fdr.series",
            "parameter_path": "complexity.series",
            "direction": direction,
        },
        "paper_locator": "src.md",
        "prior_binding": {
            "prior_unit": "percentage_points",
            "prior_metric_definition": "FDR across complexity levels.",
            "locator": "Fig 2",
            "prior_extraction_note": "x",
            "source_id": "doi:test",
        },
    }


def test_monotone_with_decreasing_pass():
    art = {
        "fdr": {"series": [2.5, 2.0, 1.5, 1.0]},
        "complexity": {"series": [1, 2, 3, 4]},
    }
    result = concordance.evaluate(art, _monotone_block())
    assert result.comparison_status == "pass"
    assert result.observed_series == [2.5, 2.0, 1.5, 1.0]


def test_monotone_with_decreasing_fail_when_increases():
    art = {
        "fdr": {"series": [1.0, 1.5, 1.2, 2.0]},
        "complexity": {"series": [1, 2, 3, 4]},
    }
    result = concordance.evaluate(art, _monotone_block())
    assert result.comparison_status == "fail"


def test_monotone_with_handles_unsorted_parameter():
    # Parameter in shuffled order; comparator must sort by parameter first.
    art = {
        "fdr": {"series": [1.5, 2.5, 1.0, 2.0]},
        "complexity": {"series": [3, 1, 4, 2]},
    }
    result = concordance.evaluate(art, _monotone_block(direction="decreasing"))
    # After sorting by complexity: complexity=[1,2,3,4], fdr=[2.5,2.0,1.5,1.0]
    assert result.observed_series == [2.5, 2.0, 1.5, 1.0]
    assert result.comparison_status == "pass"


def test_monotone_with_not_assessed_on_length_mismatch():
    art = {
        "fdr": {"series": [2.5, 2.0]},
        "complexity": {"series": [1, 2, 3]},
    }
    result = concordance.evaluate(art, _monotone_block())
    assert result.comparison_status == "not_assessed"
    assert "length mismatch" in result.diagnostics["reason"]


# ---------------------------------------------------------------------
# dispatch / error paths
# ---------------------------------------------------------------------


def test_unknown_pattern_kind_raises():
    block = {
        "pattern": {"pattern_kind": "unicycle_match"},
        "prior_binding": {},
        "paper_locator": "src.md",
    }
    with pytest.raises(concordance.ConcordanceError):
        concordance.evaluate({}, block)


def test_missing_pattern_kind_raises():
    with pytest.raises(concordance.ConcordanceError):
        concordance.evaluate({}, {"pattern": {}, "prior_binding": {}})


# ---------------------------------------------------------------------
# last_concorded.json sidecar
# ---------------------------------------------------------------------


def test_last_concorded_round_trip(tmp_path: Path):
    path = tmp_path / "last_concorded.json"
    entries = {
        "claim-a": last_concorded.LastConcordedEntry(
            comparison_status="pass",
            observed_value=1.6,
            observed_unit="percentage_points",
            image_digest="sha256:abc",
            produced_at="2026-06-04T10:00:00Z",
            diagnostics={"delta_from_prior": 0.1, "within_band": True},
        ),
        "claim-b": last_concorded.LastConcordedEntry(
            comparison_status="fail",
            observed_ordering=["MaxQuant", "PEAKS_XPro", "FragPipe_v22"],
            prior_ordering=["FragPipe_v22", "PEAKS_XPro", "MaxQuant"],
        ),
    }
    last_concorded.write(path, entries)
    loaded = last_concorded.read(path)
    assert loaded["claim-a"].comparison_status == "pass"
    assert loaded["claim-a"].observed_value == 1.6
    assert loaded["claim-a"].diagnostics["within_band"] is True
    assert loaded["claim-b"].observed_ordering == [
        "MaxQuant", "PEAKS_XPro", "FragPipe_v22",
    ]


def test_last_concorded_merge_new_wins(tmp_path: Path):
    existing = {
        "claim-a": last_concorded.LastConcordedEntry(comparison_status="fail"),
    }
    new = {
        "claim-a": last_concorded.LastConcordedEntry(comparison_status="pass"),
        "claim-b": last_concorded.LastConcordedEntry(
            comparison_status="not_assessed",
        ),
    }
    merged = last_concorded.merge(existing, new)
    assert merged["claim-a"].comparison_status == "pass"
    assert merged["claim-b"].comparison_status == "not_assessed"


def test_last_concorded_read_missing_returns_empty(tmp_path: Path):
    path = tmp_path / "does_not_exist.json"
    assert last_concorded.read(path) == {}


def test_evaluate_to_sidecar_round_trip(tmp_path: Path):
    """End-to-end: evaluate a concordance claim, write the result to
    the sidecar, read it back, check the comparison_status."""
    result = concordance.evaluate(_artifact_band(1.6), _band_block())
    path = tmp_path / "last_concorded.json"
    entries = {
        "rustims-fragpipe-fdr-10k-concords-meier": last_concorded.LastConcordedEntry(
            comparison_status=result.comparison_status,
            observed_value=result.observed_value,
            observed_unit=result.observed_unit,
            image_digest=result.image_digest,
            produced_at=result.produced_at,
            diagnostics=result.diagnostics,
        ),
    }
    last_concorded.write(path, entries)
    loaded = last_concorded.read(path)
    assert loaded["rustims-fragpipe-fdr-10k-concords-meier"].comparison_status == "pass"
    assert loaded["rustims-fragpipe-fdr-10k-concords-meier"].observed_value == 1.6
