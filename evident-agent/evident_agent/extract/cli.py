"""Phase 5 PR5: CLI wiring for ``evident-agent extract``.

Composes the walker (``repo.py``) with the framing/validator/render
package from PR4. The bridge between the model's open-ended
output and the framework's structured corpus.

Two modes:

- ``--dry-run``: walker runs, model is NOT called; ``audit.py``
  writes the source-audit outputs.
- normal: walker → model call → response processing → validator
  → render writes ``extracted/<artifact-id>/`` directory.

The response processor is where the validator becomes
load-bearing: the model can return any tolerance, but only
validator-approved tolerances reach ``evident.yaml``.
"""

from __future__ import annotations

import logging
from dataclasses import asdict
from pathlib import Path
from typing import Any, Iterable

from . import audit, framing, paper, render, repo
from .render import ExtractedClaim, ExtractionResult, RejectedCandidate
from .validator import ValidationError, validate_tolerance


LOGGER = logging.getLogger(__name__)


# ---------------------------------------------------------------------
# Response extraction (same pattern as evident_agent.review)
# ---------------------------------------------------------------------


class ExtractTransportError(Exception):
    """The API response was missing the expected tool_use block."""


def _extract_tool_input(response: Any) -> dict:
    """Pull the ``submit_extracted_claims`` tool input out of an
    Anthropic Message response. Tolerates both SDK objects and
    plain-dict fixture replay (mirrors review.py).
    """
    content = getattr(response, "content", None)
    if content is None and isinstance(response, dict):
        content = response.get("content")
    if not content:
        raise ExtractTransportError("response has no content")

    for block in content:
        block_type = getattr(block, "type", None)
        if block_type is None and isinstance(block, dict):
            block_type = block.get("type")
        if block_type != "tool_use":
            continue
        name = (
            getattr(block, "name", None)
            or (block.get("name") if isinstance(block, dict) else None)
        )
        if name != framing.TOOL_DEFINITION["name"]:
            continue
        tool_input = getattr(block, "input", None)
        if tool_input is None and isinstance(block, dict):
            tool_input = block.get("input")
        if not isinstance(tool_input, dict):
            raise ExtractTransportError(
                f"tool_use block has non-dict input: {type(tool_input).__name__}"
            )
        return tool_input
    raise ExtractTransportError(
        f"no {framing.TOOL_DEFINITION['name']!r} tool_use block in response"
    )


# ---------------------------------------------------------------------
# Tool-response processor (the validator's hook)
# ---------------------------------------------------------------------


def process_tool_response(
    tool_input: dict,
    walked: repo.WalkedSource,
    *,
    extractor_model: str,
    extracted_at: str,
) -> ExtractionResult:
    """Walk the model's tool_input, drop validator-rejected
    tolerances, drop claims with zero remaining tolerances, and
    return a ready-to-render ``ExtractionResult``.
    """
    accepted_claims: list[ExtractedClaim] = []
    rejections: list[RejectedCandidate] = list(
        _model_rejections(tool_input)
    )

    for raw_claim in tool_input.get("claims", []):
        if not isinstance(raw_claim, dict):
            continue
        accepted_tolerances: list[dict] = []
        for raw_tol in raw_claim.get("tolerances", []):
            try:
                validate_tolerance(
                    raw_tol,
                    subject_aliases=raw_claim.get(
                        "subject_aliases", []
                    ),
                )
            except ValidationError as exc:
                rejections.append(
                    RejectedCandidate(
                        candidate_text=str(raw_tol.get("source_span", "")),
                        locator=str(raw_claim.get("id", "<unknown>")),
                        reason=_map_validator_kind_to_rejection_reason(
                            exc.kind
                        ),
                        rationale=(
                            f"validator rejected tolerance for "
                            f"{raw_claim.get('id')!r}: {exc.message}"
                        ),
                    )
                )
                continue
            accepted_tolerances.append(raw_tol)

        if not accepted_tolerances:
            # Codex v3 contract: claim with zero valid tolerances is
            # dropped entirely. Each tolerance rejection is already
            # in `rejections`.
            continue

        accepted_claims.append(
            ExtractedClaim(
                id=str(raw_claim.get("id", "")),
                title=str(raw_claim.get("title", "")),
                claim=str(raw_claim.get("claim", "")),
                subject_aliases=list(
                    raw_claim.get("subject_aliases", [])
                ),
                tolerances=accepted_tolerances,
            )
        )

    return ExtractionResult(
        source_id=walked.source_id,
        source_sha=walked.source_sha,
        extractor_model=extractor_model,
        extracted_at=extracted_at,
        claims=accepted_claims,
        rejections=rejections,
    )


def _model_rejections(tool_input: dict) -> Iterable[RejectedCandidate]:
    for raw in tool_input.get("rejections", []) or []:
        if not isinstance(raw, dict):
            continue
        yield RejectedCandidate(
            candidate_text=str(raw.get("candidate_text", "")),
            locator=str(raw.get("locator", "")),
            reason=str(raw.get("reason", "unspecified")),
            rationale=str(raw.get("rationale", "")),
        )


# Validator kind discriminators are stable but their names don't
# exactly match the model-rejection enum from framing.py. Codex
# F-PR5-CR4 (P2): keep the kinds DISTINCT so the curator can read
# the precise failure mode. Each validator KIND_* maps to a
# qualified reason string that preserves the original discriminator
# rather than collapsing several modes into `bound_not_stated`.
_VALIDATOR_TO_REJECTION_REASON = {
    "missing_source_span": "validator_missing_source_span",
    "missing_metric": "metric_not_named",
    "missing_comparator": "validator_missing_comparator",
    "missing_value": "validator_missing_value",
    "missing_subject": "validator_missing_subject",
    "comparator_direction_mismatch": "validator_comparator_direction_mismatch",
    "comparator_bound_to_wrong_subject": "comparator_bound_to_wrong_subject",
}


def _map_validator_kind_to_rejection_reason(kind: str) -> str:
    """Lift a validator KIND_* into a render-layer rejection reason.

    Validator-side rejections are prefixed `validator_*` (except the
    two that map onto model-side enum values, `metric_not_named`
    and `comparator_bound_to_wrong_subject`) so the curator can
    distinguish "model emitted a tolerance that the validator
    rejected" from "model said don't extract this candidate." The
    EXTRACTION.md writer groups by reason, so the prefix gives
    curators a clean axis to filter on.
    """
    return _VALIDATOR_TO_REJECTION_REASON.get(
        kind, f"validator_{kind}"
    )


# ---------------------------------------------------------------------
# Top-level run
# ---------------------------------------------------------------------


def run_extract_repo(
    *,
    repo_path: Path,
    output_dir: Path,
    project: str | None = None,
    model: str = "claude-opus-4-7",
    dry_run: bool = False,
    api_client: Any | None = None,
    max_tokens: int = 4096,
    extracted_at: str | None = None,
) -> ExtractionResult | None:
    """Top-level entry point. Returns the ``ExtractionResult`` on a
    normal run; ``None`` on dry-run (no manifest is produced).
    """
    walked = repo.walk_repo(repo_path)
    if dry_run:
        audit.write_dry_run_outputs(walked, output_dir)
        return None

    assembled = repo.assemble_for_model(walked)
    request = framing.build_request(
        source_text=assembled,
        source_id=walked.source_id,
        model=model,
        max_tokens=max_tokens,
    )
    if api_client is None:
        api_client = _default_api_client()
    response = api_client.messages.create(**request)
    tool_input = _extract_tool_input(response)
    result = process_tool_response(
        tool_input,
        walked,
        extractor_model=model,
        extracted_at=(
            extracted_at if extracted_at else render.now_utc_isoformat()
        ),
    )
    project_name = project or f"extracted/{_repo_id_for_project(walked.source_id)}"
    render.write_outputs(result, output_dir=output_dir, project=project_name)
    return result


class PaperExtractionSkipped(Exception):
    """Raised when the paper walker decides the source cannot be
    safely sent to the model (pdftotext missing / no form-feeds /
    extraction empty). Carries the WalkedSource so the CLI can
    write a diagnostic EXTRACTION.md."""

    def __init__(self, walked: repo.WalkedSource):
        super().__init__(
            f"paper walker skipped source {walked.source_id!r}"
        )
        self.walked = walked


def run_extract_paper(
    *,
    paper_path: Path,
    output_dir: Path,
    project: str | None = None,
    source_id: str | None = None,
    model: str = "claude-opus-4-7",
    dry_run: bool = False,
    api_client: Any | None = None,
    max_tokens: int = 4096,
    extracted_at: str | None = None,
) -> ExtractionResult | None:
    """Phase 5 PR6: top-level entry point for paper extraction.

    Returns the ``ExtractionResult`` on a normal run; ``None`` on
    dry-run. Raises ``PaperExtractionSkipped`` when the walker
    refused to produce a usable source (pdftotext missing, empty
    PDF text, no form-feeds, unsupported extension); the CLI maps
    that to a diagnostic audit and a non-zero exit.
    """
    result = paper.walk_paper(paper_path, source_id=source_id)
    walked = result.walked

    if dry_run:
        audit.write_dry_run_outputs(walked, output_dir)
        return None

    # PDF refusal modes — write a diagnostic audit and propagate.
    if not walked.sections:
        audit.write_dry_run_outputs(walked, output_dir)
        raise PaperExtractionSkipped(walked)

    assembled = paper.assemble_for_model(walked)
    request = framing.build_request(
        source_text=assembled,
        source_id=walked.source_id,
        model=model,
        max_tokens=max_tokens,
    )
    if api_client is None:
        api_client = _default_api_client()
    response = api_client.messages.create(**request)
    tool_input = _extract_tool_input(response)
    extraction = process_tool_response(
        tool_input,
        walked,
        extractor_model=model,
        extracted_at=(
            extracted_at if extracted_at else render.now_utc_isoformat()
        ),
    )
    project_name = project or f"extracted/{_repo_id_for_project(walked.source_id)}"
    render.write_outputs(
        extraction, output_dir=output_dir, project=project_name,
    )
    # PDF experimental warning travels through walked.notes; surface
    # it in EXTRACTION.md by appending after render's writer ran.
    if result.source_format == "pdf" and walked.notes:
        _append_experimental_pdf_banner(output_dir, walked.notes)
    return extraction


def _append_experimental_pdf_banner(
    output_dir: Path, notes: list[str]
) -> None:
    """Surface PR6 experimental-PDF banner on top of the render's
    EXTRACTION.md so a curator can't miss it."""
    md_path = output_dir / "EXTRACTION.md"
    if not md_path.is_file():
        return
    body = md_path.read_text(encoding="utf-8")
    banner_lines = [
        "> **Experimental PDF source.**",
        *(f"> {n}" for n in notes if n),
        "",
    ]
    md_path.write_text(
        "\n".join(banner_lines) + "\n" + body,
        encoding="utf-8",
    )


def _repo_id_for_project(source_id: str) -> str:
    """Map a `github:owner/repo@sha` or `local:name@sha` into a
    filesystem-friendly project slug."""
    # Drop the @sha tail.
    base = source_id.split("@", 1)[0]
    return base.replace(":", "-").replace("/", "-")


def _default_api_client() -> Any:
    """Lazy-import the Anthropic SDK and return a default client."""
    try:
        import anthropic
    except ImportError as exc:
        raise RuntimeError(
            "Anthropic SDK not installed; install `anthropic` or pass "
            "an explicit api_client to run_extract_repo()."
        ) from exc
    return anthropic.Anthropic()


# Re-export for tests
__all__ = [
    "ExtractTransportError",
    "process_tool_response",
    "run_extract_repo",
]
