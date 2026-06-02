"""Curator review walkthrough.

Interactive walk through each extracted claim with timing,
accept/drop/skip prompts, and a curation-log writer. Composes
with ``curator.promote_claim`` and ``curator.drop_claim``.

Design:

- Each claim's pre/post timestamps are captured; the curation
  log records ``minutes_spent`` per claim. This is the
  load-bearing input to the Phase 5 extraction-rate experiment's
  curator-cost metric.
- The walkthrough is idempotent: claims at ``tier`` other than
  ``research`` (already promoted) and claims missing from the
  manifest (already dropped) are skipped. A curator can run
  ``review`` again to finish a partial session.
- ``--non-interactive`` produces a dry-run summary of what
  WOULD be displayed without prompting. Used for tests and for
  curator preview before committing time.

The curation log shape matches
``experiments/phase5-extraction-rate/curation/_template.yaml``.
"""

from __future__ import annotations

import datetime
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Optional, TextIO

import click
import yaml

from . import curator as curator_mod


# ---------------------------------------------------------------------
# Decision types
# ---------------------------------------------------------------------


@dataclass
class ClaimReviewRecord:
    """One claim's outcome from a walkthrough session."""

    extracted_id: str
    decision: str  # "accept" | "drop" | "skip" | "already_curated"
    to_tier: Optional[str] = None
    rationale: Optional[str] = None
    started_at: str = ""
    ended_at: str = ""
    minutes_spent: float = 0.0
    matched_ground_truth_id: Optional[str] = None
    notes: Optional[str] = None


@dataclass
class WalkthroughResult:
    artifact_id: str
    curator: str
    extraction_started_at: str
    walkthrough_started_at: str
    walkthrough_ended_at: str
    minutes_total: float
    records: list[ClaimReviewRecord] = field(default_factory=list)
    quit_early: bool = False


# ---------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------


def _now_utc() -> datetime.datetime:
    return datetime.datetime.now(tz=datetime.timezone.utc).replace(
        microsecond=0,
    )


def _iso(dt: datetime.datetime) -> str:
    return dt.isoformat().replace("+00:00", "Z")


def _read_manifest(path: Path) -> dict:
    return yaml.safe_load(path.read_text(encoding="utf-8")) or {}


def _read_cited_md_anchor(
    cited_md_path: Path, claim_id: str
) -> Optional[str]:
    """Return the section of cited.md anchored by the given claim
    id. None if the file or section is missing."""
    if not cited_md_path.is_file():
        return None
    text = cited_md_path.read_text(encoding="utf-8")
    marker = f'id="{claim_id}"'
    idx = text.find(marker)
    if idx == -1:
        return None
    # Take from the heading line that contains the anchor through
    # the next `## ` heading (or EOF).
    line_start = text.rfind("\n", 0, idx) + 1
    next_section = text.find("\n## ", line_start + 1)
    end = next_section if next_section != -1 else len(text)
    return text[line_start:end].strip()


def _format_claim_for_display(
    claim: dict, cited_md_anchor: Optional[str]
) -> str:
    """Build a human-readable rendering for the curator."""
    lines = [
        "",
        "=" * 72,
        f"Claim id:    {claim.get('id', '<missing>')}",
        f"Tier:        {claim.get('tier', '<missing>')}",
        f"Title:       {claim.get('title', '')}",
        f"Claim:       {(claim.get('claim') or '').strip()}",
        "",
        "Tolerances:",
    ]
    for i, t in enumerate(claim.get("tolerances") or []):
        lines.append(
            f"  [{i}] {t.get('metric', '')} "
            f"{t.get('op', '')} {t.get('value', '')}"
        )
        prose = (t.get("prose") or "").strip()
        if prose:
            lines.append(f"      {prose}")
    if cited_md_anchor:
        lines.append("")
        lines.append("Cited source (from source/cited.md):")
        for line in cited_md_anchor.splitlines():
            lines.append(f"  | {line}")
    lines.append("=" * 72)
    lines.append("")
    return "\n".join(lines)


# ---------------------------------------------------------------------
# Prompt protocols (so tests can stub them)
# ---------------------------------------------------------------------


PromptDecision = Callable[[str], str]
"""Given the display block, return one of accept/drop/skip/quit."""

PromptTier = Callable[[], str]
"""Return 'ci' or 'release'."""

PromptText = Callable[[str], str]
"""Return free text for rationale."""


def _click_prompt_decision(display: str) -> str:
    click.echo(display)
    while True:
        ans = click.prompt(
            "[a]ccept / [d]rop / [s]kip / [q]uit",
            type=str,
        ).strip().lower()
        if ans in ("a", "accept"):
            return "accept"
        if ans in ("d", "drop"):
            return "drop"
        if ans in ("s", "skip"):
            return "skip"
        if ans in ("q", "quit"):
            return "quit"
        click.echo("  ? choose a/d/s/q")


def _click_prompt_tier() -> str:
    return click.prompt(
        "promote to which tier?",
        type=click.Choice(["ci", "release"]),
    )


def _click_prompt_text(prompt: str) -> str:
    return click.prompt(prompt, type=str).strip()


# ---------------------------------------------------------------------
# Walkthrough core
# ---------------------------------------------------------------------


def walk_manifest(
    *,
    manifest_path: Path,
    curator: str,
    artifact_id: Optional[str] = None,
    cited_md_path: Optional[Path] = None,
    sidecar_path: Optional[Path] = None,
    prompt_decision: PromptDecision = _click_prompt_decision,
    prompt_tier: PromptTier = _click_prompt_tier,
    prompt_text: PromptText = _click_prompt_text,
    out: Optional[TextIO] = None,
) -> WalkthroughResult:
    """Walk each claim in ``manifest_path`` and handle the curator's
    decision per-claim. Returns the per-claim record list with
    timestamps.

    ``cited_md_path`` defaults to ``manifest_path.parent / source / cited.md``.
    ``sidecar_path`` defaults to ``manifest_path.parent / review_events.json``.
    """
    manifest = _read_manifest(manifest_path)
    claims = manifest.get("claims") or []
    artifact_id = artifact_id or manifest.get("project") or manifest_path.parent.name

    cited_md = (
        cited_md_path
        if cited_md_path is not None
        else manifest_path.parent / "source" / "cited.md"
    )
    sidecar = (
        sidecar_path
        if sidecar_path is not None
        else manifest_path.parent / "review_events.json"
    )

    walk_started = _now_utc()
    records: list[ClaimReviewRecord] = []
    quit_early = False

    for claim in claims:
        cid = claim.get("id", "<missing-id>")
        tier = claim.get("tier", "")

        if tier != "research":
            # Already curated in a prior run; record as such.
            records.append(
                ClaimReviewRecord(
                    extracted_id=cid,
                    decision="already_curated",
                    to_tier=tier,
                )
            )
            continue

        anchor = _read_cited_md_anchor(cited_md, cid)
        display = _format_claim_for_display(claim, anchor)

        started = _now_utc()
        choice = prompt_decision(display)
        if choice == "quit":
            quit_early = True
            records.append(
                ClaimReviewRecord(
                    extracted_id=cid,
                    decision="skip",
                    started_at=_iso(started),
                    ended_at=_iso(_now_utc()),
                    notes="walkthrough quit early",
                )
            )
            break

        if choice == "skip":
            ended = _now_utc()
            records.append(
                ClaimReviewRecord(
                    extracted_id=cid,
                    decision="skip",
                    started_at=_iso(started),
                    ended_at=_iso(ended),
                    minutes_spent=_minutes(started, ended),
                )
            )
            continue

        if choice == "drop":
            curator_mod.drop_claim(
                manifest_path=manifest_path,
                claim_id=cid,
            )
            ended = _now_utc()
            records.append(
                ClaimReviewRecord(
                    extracted_id=cid,
                    decision="drop",
                    started_at=_iso(started),
                    ended_at=_iso(ended),
                    minutes_spent=_minutes(started, ended),
                )
            )
            continue

        if choice == "accept":
            to_tier = prompt_tier()
            rationale = prompt_text("rationale (required)")
            try:
                curator_mod.promote_claim(
                    manifest_path=manifest_path,
                    claim_id=cid,
                    to_tier=to_tier,
                    rationale=rationale,
                    curator=curator,
                    sidecar_path=sidecar,
                )
            except curator_mod.CuratorError as exc:
                ended = _now_utc()
                records.append(
                    ClaimReviewRecord(
                        extracted_id=cid,
                        decision="skip",
                        started_at=_iso(started),
                        ended_at=_iso(ended),
                        minutes_spent=_minutes(started, ended),
                        notes=f"promotion failed: {exc}",
                    )
                )
                continue
            ended = _now_utc()
            records.append(
                ClaimReviewRecord(
                    extracted_id=cid,
                    decision="accept",
                    to_tier=to_tier,
                    rationale=rationale,
                    started_at=_iso(started),
                    ended_at=_iso(ended),
                    minutes_spent=_minutes(started, ended),
                )
            )
            continue

    walk_ended = _now_utc()
    return WalkthroughResult(
        artifact_id=str(artifact_id),
        curator=curator,
        extraction_started_at=(
            (manifest.get("claims") or [{}])[0]
            .get("provenance", {})
            .get("extractor", {})
            .get("extracted_at", "")
        ),
        walkthrough_started_at=_iso(walk_started),
        walkthrough_ended_at=_iso(walk_ended),
        minutes_total=_minutes(walk_started, walk_ended),
        records=records,
        quit_early=quit_early,
    )


def _minutes(start: datetime.datetime, end: datetime.datetime) -> float:
    return round((end - start).total_seconds() / 60.0, 4)


# ---------------------------------------------------------------------
# Curation log writer (matches experiments template)
# ---------------------------------------------------------------------


def render_curation_log(result: WalkthroughResult) -> dict:
    """Build the curation log dict matching
    experiments/phase5-extraction-rate/curation/_template.yaml.
    """
    n_accept = sum(1 for r in result.records if r.decision == "accept")
    n_drop = sum(1 for r in result.records if r.decision == "drop")
    n_skip = sum(
        1 for r in result.records
        if r.decision in ("skip",)
    )
    return {
        "artifact_id": result.artifact_id,
        "curator": result.curator,
        "extraction": {
            "extracted_at": result.extraction_started_at or None,
            "claims_processed": len(result.records),
            "accepted": n_accept,
            "dropped": n_drop,
            "skipped": n_skip,
        },
        "curation": {
            "started_at": result.walkthrough_started_at,
            "ended_at": result.walkthrough_ended_at,
            "minutes_total": result.minutes_total,
            "quit_early": result.quit_early,
        },
        "extracted_claims": [
            {
                "extracted_id": r.extracted_id,
                "matched_ground_truth_id": r.matched_ground_truth_id,
                "decision": r.decision,
                "to_tier": r.to_tier,
                "rationale": r.rationale,
                "minutes_spent": r.minutes_spent,
                "started_at": r.started_at,
                "ended_at": r.ended_at,
                "notes": r.notes,
            }
            for r in result.records
        ],
    }


def write_curation_log(result: WalkthroughResult, path: Path) -> None:
    payload = render_curation_log(result)
    path.write_text(
        yaml.safe_dump(payload, sort_keys=False, default_flow_style=False),
        encoding="utf-8",
    )
