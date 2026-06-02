"""Phase 5 PR4: tests for the extract validator.

The validator is the load-bearing piece of Phase 5 (per codex's two
reviews): it kills silent threshold invention by enforcing that the
metric, comparator, bound, AND subject all co-occur in the same local
context (sentence / table cell / row) of the source span.

A model can write a YAML tolerance that LOOKS rigorous; the validator
is what prevents it from smuggling in a bound the source never
claimed for the claimed subject.

See EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md v3, section
"Default-deny framing (v3: source-span enforcement)".
"""

from __future__ import annotations

import pytest

from evident_agent.extract.validator import (
    ValidationError,
    validate_tolerance,
)


# ---------------------------------------------------------------------
# Happy-path: a clean source span with all four elements co-occurring
# ---------------------------------------------------------------------


def test_clean_tolerance_with_explicit_bound_passes():
    """The base happy path: source span quotes a bound, comparator,
    metric, and subject all in the same sentence."""
    tolerance = {
        "metric": "median_rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": (
            "we achieve median RMSD less than 0.5 Å across the "
            "BPTI test suite"
        ),
    }
    subject_aliases = ["we", "our", "ours", "the proposed method"]
    validate_tolerance(tolerance, subject_aliases=subject_aliases)


def test_clean_tolerance_with_le_unicode_passes():
    tolerance = {
        "metric": "error",
        "op": "<=",
        "value": 0.01,
        "source_span": "our method achieves error ≤ 0.01 on all 100 structures",
    }
    validate_tolerance(tolerance, subject_aliases=["our", "we", "ours"])


def test_clean_tolerance_with_greater_than_passes():
    tolerance = {
        "metric": "throughput",
        "op": ">",
        "value": 1000.0,
        "source_span": (
            "we sustain throughput greater than 1000 requests per "
            "second on the production cluster"
        ),
    }
    validate_tolerance(tolerance, subject_aliases=["we", "our"])


# ---------------------------------------------------------------------
# Load-bearing rejection: local-binding failure
# ---------------------------------------------------------------------


def test_wrong_subject_binding_is_rejected():
    """The codex-flagged bug: span says 'baseline error is below 0.5;
    our method reports 0.42'. The naive validator sees `below`, `0.5`,
    `error`, `our method` all present and approves `ours < 0.5`. The
    local-binding rule rejects because `below 0.5` is bound to the
    *baseline* subject, not the claimed subject.
    """
    tolerance = {
        "metric": "error",
        "op": "<",
        "value": 0.5,
        "source_span": (
            "baseline error is below 0.5; our method reports 0.42"
        ),
    }
    with pytest.raises(ValidationError) as exc:
        validate_tolerance(
            tolerance, subject_aliases=["our method", "we", "ours"]
        )
    assert exc.value.kind == "comparator_bound_to_wrong_subject"


def test_prose_says_better_with_number_in_other_sentence_is_rejected():
    """The other codex-flagged case: prose says 'outperforms' (no
    bound), number lives only in a separate sentence/table. The
    validator must NOT accept inventing `< 0.5` because the bound
    `0.5` never appears as a bound in the span.

    The discriminator is missing_value: the bound `0.5` itself
    never appears (the table reports `0.42`). That's the right
    rejection — the extractor cannot invent a bound from a
    reported value.
    """
    tolerance = {
        "metric": "rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": (
            "Our method outperforms the baseline on the BPTI suite. "
            "Table 3 reports rmsd = 0.42."
        ),
    }
    with pytest.raises(ValidationError) as exc:
        validate_tolerance(
            tolerance, subject_aliases=["our", "we", "ours"]
        )
    assert exc.value.kind in (
        "comparator_bound_to_wrong_subject",
        "missing_comparator",
        "missing_value",
    )


# ---------------------------------------------------------------------
# Per-element rejection: missing metric, comparator, value, or subject
# ---------------------------------------------------------------------


def test_missing_metric_token_is_rejected():
    tolerance = {
        "metric": "median_rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": "we achieve a value less than 0.5",  # no 'rmsd'
    }
    with pytest.raises(ValidationError) as exc:
        validate_tolerance(tolerance, subject_aliases=["we", "our"])
    assert exc.value.kind == "missing_metric"


def test_missing_comparator_is_rejected():
    """Span contains the bound value but no comparator word."""
    tolerance = {
        "metric": "rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": "our method's rmsd is 0.5 on the suite",
    }
    with pytest.raises(ValidationError) as exc:
        validate_tolerance(
            tolerance, subject_aliases=["our method", "we"]
        )
    assert exc.value.kind in (
        "missing_comparator",
        "comparator_bound_to_wrong_subject",
    )


def test_missing_value_is_rejected():
    tolerance = {
        "metric": "rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": "our rmsd is less than half an angstrom",
    }
    with pytest.raises(ValidationError) as exc:
        validate_tolerance(
            tolerance, subject_aliases=["our", "we"]
        )
    assert exc.value.kind in (
        "missing_value",
        "comparator_bound_to_wrong_subject",
    )


def test_missing_subject_is_rejected():
    """If the source span never mentions the claimed subject, the
    tolerance is rejected. Otherwise the extractor could attribute
    someone else's claim to 'ours'."""
    tolerance = {
        "metric": "rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": "The benchmark's rmsd is less than 0.5",
    }
    with pytest.raises(ValidationError) as exc:
        validate_tolerance(
            tolerance,
            subject_aliases=["our method", "our system", "we propose"],
        )
    assert exc.value.kind == "missing_subject"


# ---------------------------------------------------------------------
# Comparator coverage: all 25+ phrasings the v3 plan calls out
# ---------------------------------------------------------------------


@pytest.mark.parametrize(
    "phrasing",
    [
        # `<` / `<=` family (from EVIDENT_PHASE5 v3)
        "less than 0.5",
        "less than or equal to 0.5",
        "less or equal 0.5",
        "below 0.5",
        "under 0.5",
        "lower than 0.5",
        "smaller than 0.5",
        "fewer than 0.5",
        "at most 0.5",
        "at or below 0.5",
        "no more than 0.5",
        "no greater than 0.5",
        "not greater than 0.5",
        "does not exceed 0.5",
        "up to 0.5",
        "capped at 0.5",
        "maximum of 0.5",
        "max. 0.5",
        "bounded by 0.5",
        "bounded above by 0.5",
        "max. 0.5",
        "≤ 0.5",
        "<= 0.5",
        "< 0.5",
        # LaTeX variants
        "\\leq 0.5",
        "\\le 0.5",
        # "max." is in the v3 plan but the trailing period gets
        # normalised away by sentence splitting; "max" alone works.
        "max 0.5",
    ],
)
def test_each_lt_comparator_phrasing_is_accepted(phrasing):
    tolerance = {
        "metric": "rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": f"our method's rmsd is {phrasing} across the suite",
    }
    # Should NOT raise. The bound, comparator, and subject are all in
    # the same sentence with the metric token present.
    validate_tolerance(tolerance, subject_aliases=["our method", "we"])


@pytest.mark.parametrize(
    "phrasing",
    [
        # `>` / `>=` family
        "greater than 1000",
        "greater than or equal to 1000",
        "more than 1000",
        "above 1000",
        "higher than 1000",
        "larger than 1000",
        "exceeds 1000",
        "at least 1000",
        "at or above 1000",
        "no less than 1000",
        "not below 1000",
        "minimum of 1000",
        "bounded below by 1000",
        "≥ 1000",
        ">= 1000",
        "> 1000",
        "\\geq 1000",
        "\\ge 1000",
    ],
)
def test_each_gt_comparator_phrasing_is_accepted(phrasing):
    tolerance = {
        "metric": "throughput",
        "op": ">",
        "value": 1000,
        "source_span": f"our system's throughput is {phrasing} req/sec",
    }
    validate_tolerance(
        tolerance, subject_aliases=["our system", "we", "our"]
    )


@pytest.mark.parametrize(
    "near_match",
    [
        # Phrasings that LOOK like bounds but aren't comparators
        "approximately 0.5",
        "around 0.5",
        "close to 0.5",
        "about 0.5",
        "roughly 0.5",
        # Reports of measured values, not bounds
        "is 0.5",
        "of 0.5",
        "= 0.5",
    ],
)
def test_near_match_phrasings_are_rejected(near_match):
    """Phrasings like 'approximately', 'around', 'close to' look bound-
    like but aren't comparators; the validator must reject them. The
    extractor errs on the side of zero claims over an invented bound.
    """
    tolerance = {
        "metric": "rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": f"our method's rmsd is {near_match}",
    }
    with pytest.raises(ValidationError):
        validate_tolerance(
            tolerance, subject_aliases=["our method", "we"]
        )


# ---------------------------------------------------------------------
# Op direction must match the comparator in the span
# ---------------------------------------------------------------------


def test_op_lt_with_gt_comparator_in_span_is_rejected():
    """Tolerance says op:`<` but the cited span uses `greater than`.
    Direction mismatch — reject."""
    tolerance = {
        "metric": "rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": "our rmsd is greater than 0.5 on this suite",
    }
    with pytest.raises(ValidationError):
        validate_tolerance(
            tolerance, subject_aliases=["our", "we"]
        )


# ---------------------------------------------------------------------
# Sentence/cell splitting: validator should accept multiple sentences
# if the bound + subject co-occur in ONE of them
# ---------------------------------------------------------------------


def test_multi_sentence_span_with_clean_sentence_passes():
    """Spans often quote multiple sentences. As long as ONE sentence
    has all four (metric, comparator, value, subject), the tolerance
    is valid."""
    tolerance = {
        "metric": "rmsd",
        "op": "<",
        "value": 0.5,
        "source_span": (
            "The baseline is uninteresting. Our method achieves rmsd "
            "less than 0.5 on the BPTI suite. We also evaluated on "
            "an ablation."
        ),
    }
    validate_tolerance(
        tolerance, subject_aliases=["our method", "we", "ours"]
    )
