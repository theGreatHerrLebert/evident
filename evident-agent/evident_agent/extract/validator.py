"""Phase 5 PR4: load-bearing source-span validator.

The single highest-risk failure mode (codex's call across two reviews
of the Phase 5 plan): **silent threshold invention**. A model reading
a reported value ``0.42`` and emitting a tolerance ``< 0.5`` produces
YAML that looks rigorous but smuggles in a bound the source never
claimed for the claimed subject.

This module enforces a four-layer defence per the v3 plan:

1. Every tolerance carries a ``source_span``: a verbatim quoted
   substring of the source.
2. The span must contain the metric token, a comparator equivalent,
   the bound value, AND at least one claimed-subject alias.
3. (Local-binding rule.) Those four elements must co-occur in the same
   *local context* — same sentence, same table row, same table cell.
   A comparator that appears next to a *different* number or
   subject in the same span does NOT satisfy the rule.
4. The comparator's direction must match the tolerance's ``op``
   field. Tolerance says ``<`` and the cited phrasing is "greater
   than" → reject.

The validator is the load-bearing layer. The prompt alone is not
enough (codex was explicit), so this module is what actually gates
the corpus's honesty guarantee.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Iterable


# ---------------------------------------------------------------------
# Comparator coverage (from EVIDENT_PHASE5 v3 §"Default-deny framing
# with source-span enforcement"). Each entry is a token or phrase
# that's an acceptable surface form of `<` / `<=` / `>` / `>=`.
# ---------------------------------------------------------------------

# `<` and `<=` family. Distinction between strict and non-strict is
# not made here — the validator checks DIRECTION; whether the bound is
# strict or non-strict is the tolerance's `op` field, not the
# phrasing. (A paper saying "less than 0.5" with op:`<=` would still
# pass; the prompt asks the extractor to match the direction.)
LT_PHRASES = (
    # ASCII operators
    "<=",
    "<",
    # Unicode
    "≤",
    # LaTeX
    "\\leq",
    "\\le",
    # English (longer phrases must come before shorter ones so the
    # regex match prefers them — see _comparator_regex)
    "less than or equal to",
    "less or equal",
    "less than",
    "no more than",
    "no greater than",
    "not greater than",
    "does not exceed",
    "lower than",
    "smaller than",
    "fewer than",
    "at or below",
    "at most",
    "bounded above by",
    "bounded by",
    "below",
    "under",
    "up to",
    "capped at",
    "maximum of",
    # "max." appears in the v3 plan as an abbreviation, but the
    # sentence splitter normalises the trailing period away. Use
    # plain "max" so a span like "max. 0.5" still matches after
    # tokenisation.
    "max",
)

# `>` and `>=` family.
GT_PHRASES = (
    ">=",
    ">",
    "≥",
    "\\geq",
    "\\ge",
    "greater than or equal to",
    "greater than",
    "no less than",
    "not below",
    "at or above",
    "at least",
    "bounded below by",
    "more than",
    "higher than",
    "larger than",
    "exceeds",
    "above",
    "minimum of",
)


# ---------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------


@dataclass(frozen=True)
class ValidationError(Exception):
    """Raised by ``validate_tolerance`` on a rejected tolerance.

    The ``kind`` field is a stable discriminator the extractor uses to
    populate ``EXTRACTION.md`` with structured rejection reasons.
    """

    kind: str
    message: str

    def __str__(self) -> str:  # pragma: no cover — trivial
        return f"{self.kind}: {self.message}"


# Stable discriminators (kept in one place so the extractor's
# EXTRACTION.md writer can reference them).
KIND_MISSING_METRIC = "missing_metric"
KIND_MISSING_COMPARATOR = "missing_comparator"
KIND_MISSING_VALUE = "missing_value"
KIND_MISSING_SUBJECT = "missing_subject"
KIND_WRONG_DIRECTION = "comparator_direction_mismatch"
KIND_WRONG_BINDING = "comparator_bound_to_wrong_subject"


# ---------------------------------------------------------------------
# Sentence splitting (the unit of "local context")
# ---------------------------------------------------------------------


# Sentence boundary: any of `.;!?\n`, 2+ whitespace, OR a markdown
# table pipe `|`. The pipe matters for the codex-flagged
# wrong-subject case (F-PR4-CR1b): a markdown table row like
# `| baseline | rmsd | < 0.5 | ours | rmsd | 0.42 |` would
# otherwise pass for `ours < 0.5` because all four tokens co-occur
# in the unsplit row. Treating `|` as a cell boundary forces the
# comparator and bound to be in the same cell as the subject.
#
# A `.` is only a boundary if it's NOT between digits (protect
# `0.5`) and NOT followed by whitespace+digit (protect "max. 0.5",
# "Fig. 3", "Eq. 4").
_SENTENCE_BOUNDARIES = re.compile(
    r"(?<!\d)\.(?!\d|\s+\d)"
    r"|[;!?\n|]+"
    r"|\s{2,}"
)


def _split_into_local_contexts(span: str) -> list[str]:
    """Split a source span into local contexts.

    A "local context" is the unit within which metric, comparator,
    bound, and subject must co-occur. For prose this is roughly a
    sentence (split on ``.``, ``;``, ``!``, ``?``, newline, or
    multiple consecutive whitespace — the last so a table-row span
    rendered as cells separated by ``  `` is split correctly).

    The ``.`` boundary is skipped when between digits so decimal
    numbers like ``0.5`` stay intact.
    """
    chunks = _SENTENCE_BOUNDARIES.split(span)
    return [c.strip() for c in chunks if c.strip()]


# ---------------------------------------------------------------------
# Comparator matching
# ---------------------------------------------------------------------


# Sort all phrases by length descending so longest-match wins. This
# is critical for phrases that contain opposite-direction words:
# "no more than" must match as `lt` before "more than" can match as
# `gt`. We tag each phrase with its direction so the scanner can
# return the right verdict.
_PHRASES_BY_LENGTH: list[tuple[str, str]] = sorted(
    [(p, "lt") for p in LT_PHRASES] + [(p, "gt") for p in GT_PHRASES],
    key=lambda x: (-len(x[0]), x[0]),
)


def _direction_for_op(op: str) -> str:
    if op in ("<", "<="):
        return "lt"
    if op in (">", ">="):
        return "gt"
    raise ValueError(f"unsupported op {op!r}; expected <, <=, >, or >=")


def _opposite_direction(direction: str) -> str:
    return "gt" if direction == "lt" else "lt"


def _is_symbolic_phrase(phrase: str) -> bool:
    """A phrase is "symbolic" if it's a mathematical operator
    (``<``, ``≤``, ``\\le``) rather than English prose. Symbolic
    phrases are matched as raw substrings — they don't have
    word-character neighbours to worry about. English phrases
    require word boundaries so ``under`` doesn't match ``thunder``.
    """
    return not any(c.isalpha() for c in phrase)


def _find_comparators_in(text: str) -> list[tuple[int, str, str]]:
    """Scan `text` for comparator phrases via longest-match-wins.

    Returns a list of ``(start_offset, matched_phrase, direction)``
    where direction is ``'lt'`` or ``'gt'``. Non-overlapping —
    once a region is matched by the longest phrase, shorter
    candidates that overlap that region are skipped.

    Codex F-PR4-CR2a fix: English-prose phrases (``under``,
    ``above``, ``max``, ``at least``, etc.) are matched with word
    boundaries so they don't fire inside ``thunder``,
    ``aboveboard``, ``maximal``. Symbolic operators (``<``, ``≤``,
    ``\\le``) keep raw substring matching since they don't have
    word-character neighbours.

    This is also the cure for the "no more than" / "more than"
    ambiguity (codex F-CR-PR4-comparator-coverage): by checking
    longest phrases first, "no more than" consumes the substring
    before "more than" can match.
    """
    text_l = text.lower()
    consumed = [False] * len(text_l)
    found: list[tuple[int, str, str]] = []
    for phrase, direction in _PHRASES_BY_LENGTH:
        phrase_l = phrase.lower()
        symbolic = _is_symbolic_phrase(phrase)
        pos = 0
        while True:
            idx = text_l.find(phrase_l, pos)
            if idx == -1:
                break
            end = idx + len(phrase_l)
            if any(consumed[idx:end]):
                pos = idx + 1
                continue
            # For English phrases, require word boundaries on both
            # sides so "under" doesn't fire inside "thunder".
            if not symbolic:
                before_ok = idx == 0 or not text_l[idx - 1].isalnum()
                after_ok = end == len(text_l) or not text_l[end].isalnum()
                if not (before_ok and after_ok):
                    pos = idx + 1
                    continue
            for i in range(idx, end):
                consumed[i] = True
            found.append((idx, phrase, direction))
            pos = end
    return found


def _contains_comparator_direction(text: str, direction: str) -> bool:
    """True if a non-overlapping longest-match comparator with this
    direction appears in `text`."""
    for _, _, d in _find_comparators_in(text):
        if d == direction:
            return True
    return False


# Regex matching a numeric token that could be a bound (signed integer
# or float, optional decimals, optional scientific notation, optional
# percent sign).
_NUM_RE = re.compile(
    r"(?<![A-Za-z0-9_])(-?\d+(?:\.\d+)?(?:[eE]-?\d+)?)\s*%?"
)


def _value_appears_in(text: str, value: float) -> bool:
    """True if `value` appears as a numeric token in `text`.

    Matches the value with a small tolerance for representation
    (``0.5`` vs ``0.50``, ``1000`` vs ``1000.0``, etc.).
    """
    for match in _NUM_RE.finditer(text):
        try:
            n = float(match.group(1))
        except ValueError:
            continue
        # Use absolute tolerance for integer-ish values, relative for
        # decimals. The simple equality check via formatted-string
        # avoids floating-point surprises ("0.42" vs 0.4200000001).
        if abs(n - float(value)) < 1e-9:
            return True
        # 1000 should match a span saying "1000"; 0.5 a span saying
        # "0.5" or "0.50" but NOT "0.4".
        formatted = (
            f"{float(value):.10g}"
        )  # 10 significant digits avoids spurious matches
        if match.group(1) == formatted:
            return True
    return False


# ---------------------------------------------------------------------
# Subject matching
# ---------------------------------------------------------------------


def _subject_appears_in(text: str, aliases: Iterable[str]) -> bool:
    """True if any subject alias appears in `text` (case-insensitive,
    word-boundary safe).

    Codex F-PR4-CR2b fix: uses ``(?<!\\w)`` / ``(?!\\w)`` instead
    of ``\\b`` so aliases ending or starting with non-word characters
    (e.g. ``ABRA-2.0``) still anchor correctly.
    """
    haystack = text.lower()
    for alias in aliases:
        alias_l = alias.lower().strip()
        if not alias_l:
            continue
        # Boundary that works even for aliases ending/starting with
        # non-word characters. ``\b`` switches direction based on the
        # adjacent character; the explicit non-word lookarounds are
        # independent of the alias's last/first character class.
        pat = r"(?<!\w)" + re.escape(alias_l) + r"(?!\w)"
        if re.search(pat, haystack):
            return True
    return False


def _metric_token_in(text: str, metric: str) -> bool:
    """True if the metric token appears in `text` as a complete word.

    Codex F-PR4-CR1a fix: this used to be an unanchored substring
    match, so ``metric: mse`` would match ``rmse`` and
    ``median_rmsd`` would match ``medianrmsd``. Now requires:

    - non-word character on both sides of the matched range, so
      ``mse`` does NOT match inside ``rmse``;
    - inter-word separator allows ``_``, whitespace, or ``-`` (a
      paper might write ``median-rmsd`` or ``median RMSD``);
    - the separator must be at least one character (it's not
      ``\\s*`` anymore), so ``medianrmsd`` does NOT match
      ``median_rmsd``.
    """
    norm_parts = [
        p for p in re.split(r"[_\s-]+", metric.lower()) if p
    ]
    if not norm_parts:
        return False
    sep = r"[_\s-]+"
    pat = (
        r"(?<!\w)"
        + sep.join(re.escape(p) for p in norm_parts)
        + r"(?!\w)"
    )
    return re.search(pat, text.lower()) is not None


# ---------------------------------------------------------------------
# The validator
# ---------------------------------------------------------------------


def validate_tolerance(
    tolerance: dict,
    subject_aliases: Iterable[str],
) -> None:
    """Validate a single tolerance dict against its ``source_span``.

    Raises :class:`ValidationError` on failure. The error's ``kind``
    is one of the ``KIND_*`` discriminators.

    Required fields on ``tolerance``:

    - ``metric``: the metric name (e.g. ``"median_rmsd"``)
    - ``op``: one of ``<``, ``<=``, ``>``, ``>=``
    - ``value``: numeric bound
    - ``source_span``: verbatim quoted substring of the source

    ``subject_aliases``: list of strings that refer to the claimed
    subject (``["our method", "we", "ours"]`` etc.). Provided by the
    extractor based on the source's own subject-identifying phrasings.
    """
    span = tolerance.get("source_span") or ""
    metric = tolerance.get("metric") or ""
    op = tolerance.get("op") or ""
    value = tolerance.get("value")

    if not span:
        raise ValidationError(
            kind="missing_source_span",
            message="tolerance has no source_span",
        )
    if value is None:
        raise ValidationError(
            kind=KIND_MISSING_VALUE,
            message="tolerance has no value",
        )

    # Layer 1: span must contain the metric token at all.
    if not _metric_token_in(span, metric):
        raise ValidationError(
            kind=KIND_MISSING_METRIC,
            message=(
                f"metric token {metric!r} not present in source_span"
            ),
        )

    # Layer 2/3: direction + local-binding. For each local context,
    # look for ALL FOUR: metric, value, same-direction comparator,
    # subject alias. Use the longest-match comparator scanner so
    # phrases like "no more than" win over "more than".
    target_direction = _direction_for_op(op)
    opposite_direction = _opposite_direction(target_direction)

    found_local_match = False
    metric_present_somewhere = False
    value_present_somewhere = False
    comparator_present_somewhere = False
    subject_present_somewhere = False
    opposite_comparator_present_somewhere = False
    aliases = list(subject_aliases)
    for sentence in _split_into_local_contexts(span):
        has_metric = _metric_token_in(sentence, metric)
        has_value = _value_appears_in(sentence, float(value))
        has_comparator = _contains_comparator_direction(
            sentence, target_direction
        )
        has_opposite = _contains_comparator_direction(
            sentence, opposite_direction
        )
        has_subject = _subject_appears_in(sentence, aliases)
        metric_present_somewhere |= has_metric
        value_present_somewhere |= has_value
        comparator_present_somewhere |= has_comparator
        subject_present_somewhere |= has_subject
        opposite_comparator_present_somewhere |= has_opposite
        if (
            has_metric
            and has_value
            and has_comparator
            and has_subject
        ):
            # An opposite-direction comparator in the SAME sentence
            # is a direction mismatch (e.g. "rmsd is greater than
            # 0.5" against op:`<`). Since we use longest-match
            # detection, the false-positive "no more than" doesn't
            # trigger this.
            if has_opposite:
                raise ValidationError(
                    kind=KIND_WRONG_DIRECTION,
                    message=(
                        f"source_span contains comparator with direction "
                        f"opposite to op {op!r}"
                    ),
                )
            found_local_match = True
            break

    if found_local_match:
        return

    # No local-binding match. Diagnose which element was missing
    # globally to give the rejection a precise discriminator.
    if not value_present_somewhere:
        raise ValidationError(
            kind=KIND_MISSING_VALUE,
            message=(
                f"value {value!r} not present in source_span"
            ),
        )
    if not comparator_present_somewhere:
        # If the opposite direction IS present somewhere, that's a
        # direction mismatch rather than a missing comparator.
        if opposite_comparator_present_somewhere:
            raise ValidationError(
                kind=KIND_WRONG_DIRECTION,
                message=(
                    "source_span uses comparator with opposite direction"
                ),
            )
        raise ValidationError(
            kind=KIND_MISSING_COMPARATOR,
            message=(
                f"no comparator equivalent of op {op!r} in source_span"
            ),
        )
    if not subject_present_somewhere:
        raise ValidationError(
            kind=KIND_MISSING_SUBJECT,
            message=(
                "none of the claimed subject's aliases appear in "
                "source_span"
            ),
        )

    # Each element appeared SOMEWHERE in the span, but not all in the
    # same local context. That's the codex-flagged silent-threshold-
    # invention failure mode.
    raise ValidationError(
        kind=KIND_WRONG_BINDING,
        message=(
            "metric, comparator, value, and subject appear in the "
            "span but not co-occurring in the same local context — "
            "the bound may be attached to a different subject in the "
            "source"
        ),
    )
