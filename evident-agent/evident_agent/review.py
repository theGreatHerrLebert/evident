"""Anthropic API call + response parsing + validation + retry.

This is the only module in evident-agent that talks to a model. The
rest of Phase 2a is plumbing around its output.

Defensible properties:

- **Validation rejects, never repairs.** If the model emits a valid
  schema response that fails our semantic rules (Endorse with a
  failing check, hallucinated criterion name, sub-50-char rationale),
  we discard the response and surface stderr. We do NOT prompt for a
  revised answer — that turns the validator into a back-channel for
  prompt-engineering.

- **One retry on transport.** Network blips and the rare "model didn't
  call the forced tool" failure both warrant exactly one retry with
  the same prompt. Both attempts are logged; no semantic repair.

- **No API call unless an SDK call is provided.** Caller may inject a
  fake ``api_client`` for tests / ``--no-api`` replay. This module
  doesn't import ``anthropic`` at module load time — the import is
  lazy in ``default_api_client`` so ``--no-api`` runs work in
  environments without the SDK.
"""

from __future__ import annotations

import dataclasses
import datetime as dt
import logging
from dataclasses import dataclass, field
from typing import Any, Callable, Optional

from .prompt import (
    PROCEDURAL_CATEGORIES,
    SUBSTANTIVE_CATEGORIES,
    TOOL_DEFINITION,
    build_request,
)

LOGGER = logging.getLogger("evident_agent.review")

MIN_RATIONALE_CHARS = 50
CHECK_KEYS = ("metric_present", "within_tolerance", "outliers_checked", "reproducible_chain")
VERDICT_VALUES = ("endorse", "dissent", "challenge")
CHECK_VALUES = ("pass", "fail", "unknown")


# ---------- Result + error types ----------

@dataclass
class ReviewVerdict:
    """A validated submit_review tool input. The agent serializes this
    into a ``ReviewEventEntry`` for the sidecar."""

    verdict: str  # "endorse" | "dissent" | "challenge"
    checks: dict[str, str]  # CHECK_KEYS → CHECK_VALUES
    rationale: str
    observed_value: Optional[str] = None
    tolerance: Optional[str] = None
    failure_reason: Optional[str] = None
    # Phase 2b: only populated when verdict == "challenge".
    challenge_category: Optional[str] = None
    challenge_target_criterion_id: Optional[str] = None
    challenge_violation: Optional[dict[str, Any]] = None
    # Provenance fields populated by ``call_review``.
    model: str = ""
    request_id: str = ""
    timestamp: str = field(default_factory=lambda: _utc_now_iso())


class ReviewRejected(Exception):
    """The model's response was schema-valid but failed semantic
    validation. The caller skips the sidecar entry and logs the
    reason; we do NOT retry."""


class ReviewTransportError(Exception):
    """The response was malformed at the SDK or schema level — the
    model didn't call the forced tool, or the tool input wasn't
    decodable. One retry is allowed; if the retry also fails we
    surface this."""


# ---------- Type protocol for the API client ----------

ApiResponse = Any  # opaque; we read .content + .id off it


# ---------- Public entry point ----------

def call_review(
    *,
    model: str,
    claim_yaml: str,
    digest_rendered: str,
    api_client: Optional[Any] = None,
    max_tokens: int = 1024,
) -> ReviewVerdict:
    """Run one submit_review call.

    On transport failure (tool not called, malformed tool input),
    retries exactly once with the same prompt. On semantic failure
    (Endorse-with-failing-check, hallucinated criterion, short
    rationale), raises ``ReviewRejected`` immediately — no retry.

    ``api_client`` must expose ``.messages.create(**kwargs)`` (the
    Anthropic SDK shape). If None, the default lazy-imported Anthropic
    client is used.
    """
    if api_client is None:
        api_client = _default_api_client()

    request = build_request(
        model=model,
        claim_yaml=claim_yaml,
        digest_rendered=digest_rendered,
        max_tokens=max_tokens,
    )

    response: ApiResponse
    tool_input: Optional[dict[str, Any]] = None
    last_transport_error: Optional[BaseException] = None
    for attempt in (1, 2):
        try:
            response = api_client.messages.create(**request)
            tool_input = _extract_tool_input(response)
            break
        except ReviewTransportError as exc:
            last_transport_error = exc
            LOGGER.warning(
                "review attempt %d failed (schema/tool): %s", attempt, exc
            )
        except _sdk_transport_exception_types() as exc:
            # Real SDK transport faults (timeouts, connection errors,
            # transient rate limits). The plan's retry contract covers
            # these; map them onto ReviewTransportError so the caller
            # sees one consistent type.
            last_transport_error = ReviewTransportError(
                f"{type(exc).__name__}: {exc}"
            )
            LOGGER.warning(
                "review attempt %d failed (sdk): %s", attempt, exc
            )
        if attempt == 2:
            assert last_transport_error is not None
            raise last_transport_error
    assert tool_input is not None  # loop only exits with tool_input set

    verdict = _validate_tool_input(tool_input)
    verdict.model = model
    verdict.request_id = getattr(response, "id", "") or ""
    return verdict


# ---------- Response extraction ----------

def _extract_tool_input(response: ApiResponse) -> dict[str, Any]:
    """Pull the submit_review tool's input dict out of an Anthropic
    Message response. Tolerates both the SDK's object shape and a
    plain-dict fixture replay.
    """
    content = getattr(response, "content", None)
    if content is None and isinstance(response, dict):
        content = response.get("content")
    if not content:
        raise ReviewTransportError("response has no content")

    for block in content:
        block_type = getattr(block, "type", None)
        if block_type is None and isinstance(block, dict):
            block_type = block.get("type")
        if block_type != "tool_use":
            continue
        name = getattr(block, "name", None) or (block.get("name") if isinstance(block, dict) else None)
        if name != "submit_review":
            continue
        tool_input = getattr(block, "input", None)
        if tool_input is None and isinstance(block, dict):
            tool_input = block.get("input")
        if not isinstance(tool_input, dict):
            raise ReviewTransportError("submit_review tool_use block has non-dict input")
        return tool_input
    raise ReviewTransportError("response did not call the submit_review tool")


# ---------- Semantic validation ----------

def _validate_tool_input(tool_input: dict[str, Any]) -> ReviewVerdict:
    """Apply all semantic rules from the Phase 2a plan.

    Validation failure → ``ReviewRejected`` with a stderr-friendly
    message. We do not repair, do not retry.
    """
    # Schema-required keys.
    verdict = tool_input.get("verdict")
    checks = tool_input.get("checks")
    rationale = tool_input.get("rationale")

    if verdict not in VERDICT_VALUES:
        raise ReviewRejected(f"verdict {verdict!r} not in {VERDICT_VALUES}")
    if not isinstance(checks, dict):
        raise ReviewRejected("checks must be an object with all four check keys")
    for key in CHECK_KEYS:
        if key not in checks:
            raise ReviewRejected(f"checks missing required key {key!r}")
        if checks[key] not in CHECK_VALUES:
            raise ReviewRejected(
                f"checks.{key} = {checks[key]!r} not in {CHECK_VALUES}"
            )
    if not isinstance(rationale, str) or len(rationale.strip()) < MIN_RATIONALE_CHARS:
        raise ReviewRejected(
            f"rationale must be non-empty and >= {MIN_RATIONALE_CHARS} characters"
        )

    # Endorse-with-failing-check rule.
    any_non_pass = any(checks[k] != "pass" for k in CHECK_KEYS)
    if verdict == "endorse" and any_non_pass:
        bad = [k for k in CHECK_KEYS if checks[k] != "pass"]
        raise ReviewRejected(
            f"verdict=endorse contradicts checks: {bad} are not 'pass'"
        )

    observed_value = tool_input.get("observed_value")
    tolerance = tool_input.get("tolerance")
    failure_reason = tool_input.get("failure_reason")

    # Endorse must cite an observed_value AND tolerance.
    if verdict == "endorse":
        if not observed_value or not tolerance:
            raise ReviewRejected(
                "verdict=endorse requires non-null observed_value and tolerance"
            )

    # Phase 2b: Challenge verdict requires a challenge block.
    challenge_category: Optional[str] = None
    challenge_target_criterion_id: Optional[str] = None
    challenge_violation: Optional[dict[str, Any]] = None

    if verdict == "challenge":
        challenge_block = tool_input.get("challenge")
        if not isinstance(challenge_block, dict):
            raise ReviewRejected(
                "verdict=challenge requires a `challenge` block with category"
            )

        category = challenge_block.get("category")
        if category not in PROCEDURAL_CATEGORIES and category not in SUBSTANTIVE_CATEGORIES:
            raise ReviewRejected(
                f"challenge.category {category!r} is not a known category"
            )
        challenge_category = category

        if category in SUBSTANTIVE_CATEGORIES:
            # Substantive Challenge needs target_criterion_id +
            # violation. The deeper contradiction check happens in
            # violation.validate_contradiction (called from the CLI
            # where the target claim is in scope).
            tcid = challenge_block.get("target_criterion_id")
            if not isinstance(tcid, str) or not tcid:
                raise ReviewRejected(
                    f"substantive challenge category {category!r} requires "
                    f"a non-empty target_criterion_id"
                )
            violation = challenge_block.get("violation")
            if not isinstance(violation, dict):
                raise ReviewRejected(
                    f"substantive challenge category {category!r} requires "
                    f"a violation tuple"
                )
            # Schema-level shape check; semantic contradiction logic
            # lives in violation.validate_contradiction.
            for req in ("metric", "observed_value", "bound", "comparator", "citation"):
                if req not in violation:
                    raise ReviewRejected(
                        f"challenge.violation missing required field {req!r}"
                    )
            challenge_target_criterion_id = tcid
            challenge_violation = dict(violation)
        else:
            # Procedural — must NOT carry violation (typed-trust
            # rejects that, but catch it agent-side too).
            if challenge_block.get("violation") is not None:
                raise ReviewRejected(
                    f"procedural challenge category {category!r} must not "
                    f"carry a violation tuple"
                )

    return ReviewVerdict(
        verdict=verdict,
        checks=dict(checks),
        rationale=rationale.strip(),
        observed_value=observed_value if observed_value else None,
        tolerance=tolerance if tolerance else None,
        failure_reason=failure_reason if failure_reason else None,
        challenge_category=challenge_category,
        challenge_target_criterion_id=challenge_target_criterion_id,
        challenge_violation=challenge_violation,
    )


def reject_if_truncated_endorse_lacks_evidence(
    verdict: ReviewVerdict, digest_body: str, digest_truncated: bool
) -> None:
    """Raise ``ReviewRejected`` if the model Endorses a claim while
    the digest was truncated AND the cited ``observed_value`` does not
    appear in the digest text. The model would be working blind.

    Phase 2a plan rule F9 (codex review): "If the digest had
    truncated: true and the digest does not contain the cited
    observed_value, reject as Endorse-without-evidence."
    """
    if verdict.verdict != "endorse":
        return
    if not digest_truncated:
        return
    if verdict.observed_value is None:
        return
    if verdict.observed_value in digest_body:
        return
    raise ReviewRejected(
        f"verdict=endorse with digest truncated and observed_value "
        f"{verdict.observed_value!r} not present in digest body"
    )


def reject_if_hallucinated_criterion(
    verdict: ReviewVerdict, claim_criteria: list[str]
) -> None:
    """Raise ``ReviewRejected`` if the failure_reason references a
    criterion name that isn't in the claim's criteria list.

    Approach:
    1. Extract identifier-shaped tokens from the failure_reason — only
       the syntactically-marked ones (backticked or ``criterion:``-
       prefixed). Bare prose is allowed.
    2. Each extracted token must appear in ``claim_criteria``.
       Otherwise the model invented a criterion id, which is a clear
       hallucination signal.

    Free-form prose ("value 0.021 exceeds the bound 0.02") is allowed —
    the model may describe a defect without naming a criterion at all.
    """
    if verdict.failure_reason is None or not claim_criteria:
        return
    fr = verdict.failure_reason

    import re as _re

    flagged = _re.findall(r"`([a-zA-Z][\w.-]+)`", fr) + _re.findall(
        r"criterion[:=]?\s*`?([a-zA-Z][\w.-]+)`?", fr, flags=_re.IGNORECASE
    )
    # Drop the literal token "criterion" itself if the regex captured it.
    flagged = [tok for tok in flagged if tok.lower() != "criterion"]
    # Identifier-shaped means containing an underscore, dot, or dash —
    # criterion ids in this codebase are snake_case (metric names like
    # `median_relative_error`) or kebab-case ids. Backticked single
    # English words (e.g., `unsupported`, `failure`) are prose
    # emphasis, not criterion references; filter them out to avoid
    # false-positive rejections of otherwise valid dissents.
    flagged = [tok for tok in flagged if _re.search(r"[_.\-]", tok)]

    claim_set = set(claim_criteria)
    bogus = [tok for tok in flagged if tok not in claim_set]
    if bogus:
        raise ReviewRejected(
            f"failure_reason references criterion(s) not in claim: {bogus}"
        )


# ---------- Helpers ----------

def _utc_now_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _default_api_client() -> Any:
    """Lazy-import the Anthropic SDK and return a default client.

    Lazy because (a) the SDK may not be installed in ``--no-api``
    environments, and (b) importing at module load time would pin
    `anthropic` as a hard runtime dep on every import path through
    evident-agent.
    """
    try:
        import anthropic  # type: ignore[import-not-found]
    except ImportError as exc:  # pragma: no cover — env-specific
        raise ReviewTransportError(
            "anthropic SDK not installed; pip install anthropic, or pass --no-api"
        ) from exc
    return anthropic.Anthropic()


def _sdk_transport_exception_types() -> tuple[type[BaseException], ...]:
    """Return the SDK exception classes that should trigger the
    transport-retry path. Lazy-imported because the SDK may not be
    installed in tests / ``--no-api`` environments. When the SDK is
    absent, return an empty tuple so the except clause matches nothing
    (only ReviewTransportError catches in that case).
    """
    try:
        import anthropic  # type: ignore[import-not-found]
    except ImportError:
        return ()
    base = getattr(anthropic, "APIError", None)
    if base is None:
        return ()
    return (base,)


# ---------- Conversion for the sidecar ----------

def verdict_to_sidecar_entry(
    verdict: ReviewVerdict,
    *,
    claim_id: str,
    author_name: str,
    author_version: str,
    author_context: Optional[str] = None,
    target_claim: Optional[dict[str, Any]] = None,
) -> "ReviewEventEntry":  # noqa: F821 - forward ref to avoid cycle
    """Materialize the verdict into a sidecar entry suitable for
    ``review_sidecar.append_events``.

    For Endorse/Dissent: straight projection.

    For Challenge: build the challenge block (and, for substantive
    categories, the agent-constructed backing claim). ``target_claim``
    is REQUIRED for substantive Challenges — the backing claim is
    derived from it. The caller MUST run
    ``violation.validate_contradiction`` first; this function trusts
    its input.
    """
    from .review_sidecar import ReviewAuthor, ReviewEventEntry
    from .violation import build_backing_claim

    challenge_block: Optional[dict[str, Any]] = None
    if verdict.verdict == "challenge":
        if verdict.challenge_category is None:
            raise ReviewRejected("challenge verdict missing category")
        block: dict[str, Any] = {"category": verdict.challenge_category}
        if verdict.challenge_category in SUBSTANTIVE_CATEGORIES:
            if (
                target_claim is None
                or verdict.challenge_target_criterion_id is None
                or verdict.challenge_violation is None
            ):
                raise ReviewRejected(
                    "substantive challenge requires target_claim, "
                    "target_criterion_id, and violation"
                )
            block["target_criterion_id"] = verdict.challenge_target_criterion_id
            block["violation"] = dict(verdict.challenge_violation)
            # Phase 2c: pass the author into the backing-claim
            # construction so multi-model panels with identical
            # violation tuples produce distinct backing ids
            # (codex F-2C-1).
            author_for_hash = {
                "kind": "model",
                "name": author_name,
                "version": author_version,
                "context": author_context,
            }
            block["backing_claim"] = build_backing_claim(
                target_claim,
                verdict.challenge_target_criterion_id,
                verdict.challenge_violation,
                timestamp=verdict.timestamp,
                author=author_for_hash,
            )
        challenge_block = block

    return ReviewEventEntry(
        claim_id=claim_id,
        kind=verdict.verdict,
        author=ReviewAuthor(
            kind="model",
            name=author_name,
            version=author_version,
            context=author_context,
        ),
        rationale=verdict.rationale,
        timestamp=verdict.timestamp,
        checks=dict(verdict.checks),
        observed_value=verdict.observed_value,
        tolerance=verdict.tolerance,
        failure_reason=verdict.failure_reason,
        challenge=challenge_block,
    )


# Public re-exports for convenience.
__all__ = [
    "ReviewVerdict",
    "ReviewRejected",
    "ReviewTransportError",
    "MIN_RATIONALE_CHARS",
    "CHECK_KEYS",
    "VERDICT_VALUES",
    "CHECK_VALUES",
    "call_review",
    "reject_if_hallucinated_criterion",
    "reject_if_truncated_endorse_lacks_evidence",
    "verdict_to_sidecar_entry",
]

# Keep dataclasses module imported for type checkers / IDEs (dataclass
# decorator is used above but the symbol must remain importable).
_ = dataclasses
