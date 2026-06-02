"""Curator review walkthrough.

Interactive walk through each extracted claim with timing,
accept/drop/skip prompts, and a cumulative curation-log writer.
Composes with ``curator.promote_claim`` and ``curator.drop_claim``.

Design:

- Each claim's pre/post timestamps are captured; the curation
  log records ``minutes_spent`` per claim. This is the
  load-bearing input to the Phase 5 extraction-rate experiment's
  curator-cost metric.

- **Cumulative log discipline (codex F-WALK-CR1 P1).** Reruns
  do NOT overwrite a prior log. Before walking, the writer
  loads the existing curation log (if any), preserves its
  ``accept``/``drop`` records, and walks only the claims that
  still need a decision (tier:research + no prior accept/drop).
  Prior ``drop`` records are retained even though the claims
  themselves are no longer in the manifest — they are part of
  the curator's cumulative decision history.

- **Quit writes ``unreviewed`` records (codex F-WALK-CR3 P2)**
  for remaining-in-manifest claims so the aggregator's
  ``len(extracted_claims)`` denominator stays correct.

- The walkthrough is idempotent and resumable: a curator can
  ``quit`` and run again; the resumed run picks up where the
  prior left off.

- Prompt callbacks are dependency-injected so tests don't need a
  real terminal.

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
    """One claim's outcome from a walkthrough session.

    Decision values:
    - ``accept`` — curator promoted the claim
    - ``drop`` — curator removed the claim from the manifest
    - ``skip`` — curator declined to decide (claim stays
      tier:research, will re-prompt on re-run)
    - ``unreviewed`` — walkthrough quit before reaching this
      claim (codex F-WALK-CR3 P2)
    """

    extracted_id: str
    decision: str
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
    extractor_model: Optional[str]
    extracted_claims_count: int
    walkthrough_started_at: str
    walkthrough_ended_at: str
    minutes_total: float
    records: list[ClaimReviewRecord] = field(default_factory=list)
    quit_early: bool = False


# ---------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------


def _now_utc() -> datetime.datetime:
    # Codex F-WALK-CR-timing-precision: keep microseconds during
    # measurement so fast decisions don't collapse to 0.0. Only
    # round at serialization.
    return datetime.datetime.now(tz=datetime.timezone.utc)


def _iso(dt: datetime.datetime) -> str:
    # Serialize to second precision for the audit log.
    return (
        dt.replace(microsecond=0).isoformat().replace("+00:00", "Z")
    )


def _read_manifest(path: Path) -> dict:
    return yaml.safe_load(path.read_text(encoding="utf-8")) or {}


def _read_cited_md_anchor(
    cited_md_path: Path, claim_id: str
) -> Optional[str]:
    """Return the section of cited.md anchored by the given claim
    id. None if the file or section is missing.

    Codex F-WALK-CR4 P2: match only an actual ``<a id="<claim>">``
    anchor element, not a quoted anchor-looking string that might
    appear in another section's prose.
    """
    import html
    import re

    if not cited_md_path.is_file():
        return None
    text = cited_md_path.read_text(encoding="utf-8")
    escaped = html.escape(claim_id, quote=True)
    pat = re.compile(
        r'<a\s+id="' + re.escape(escaped) + r'"\s*>',
        re.IGNORECASE,
    )
    m = pat.search(text)
    if m is None:
        return None
    idx = m.start()
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

PromptTier = Callable[[list[str], str], str]
"""Given (valid_targets, current_tier), return the chosen target.

The walkthrough computes ``valid_targets`` from the promotion ladder
(`curator._adjacent_promotion_target`). Today that's at most one
element — a research-tier claim advances to ci, a ci-tier claim
advances to release. The prompt presents the choice; if only one
target is valid, it asks for explicit confirmation rather than
auto-selecting.
"""

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


def _click_prompt_tier(valid_targets: list[str], current_tier: str) -> str:
    """Prompt the curator for the promotion target.

    Multi-step ladder awareness: the walkthrough passes only the
    next-rung target. If there's exactly one valid target, ask for
    confirmation; otherwise present the choice.
    """
    if not valid_targets:
        raise click.UsageError(
            f"claim is at tier {current_tier!r} which is not a valid "
            "promotion source"
        )
    if len(valid_targets) == 1:
        target = valid_targets[0]
        if click.confirm(
            f"promote {current_tier} -> {target}?",
            default=False,
        ):
            return target
        # Treat decline as a skip-from-accept-branch — the walkthrough
        # records this via the CuratorError path. Returning the same
        # tier triggers the ladder rejection downstream which the
        # walkthrough re-maps to skip.
        return current_tier
    return click.prompt(
        f"promote {current_tier} -> which tier?",
        type=click.Choice(valid_targets),
    )


def _click_prompt_text(prompt: str) -> str:
    return click.prompt(prompt, type=str).strip()


# ---------------------------------------------------------------------
# Walkthrough core
# ---------------------------------------------------------------------


def _load_prior_records(
    log_path: Path,
) -> dict[str, ClaimReviewRecord]:
    """Read prior records from an existing curation log. Returns
    an empty map if the file is missing or malformed."""
    if not log_path.is_file():
        return {}
    try:
        payload = yaml.safe_load(log_path.read_text(encoding="utf-8")) or {}
    except yaml.YAMLError:
        return {}
    out: dict[str, ClaimReviewRecord] = {}
    for raw in payload.get("extracted_claims") or []:
        if not isinstance(raw, dict):
            continue
        cid = raw.get("extracted_id")
        if not cid:
            continue
        out[cid] = ClaimReviewRecord(
            extracted_id=cid,
            decision=raw.get("decision", "skip"),
            to_tier=raw.get("to_tier"),
            rationale=raw.get("rationale"),
            started_at=raw.get("started_at", ""),
            ended_at=raw.get("ended_at", ""),
            minutes_spent=float(raw.get("minutes_spent") or 0.0),
            matched_ground_truth_id=raw.get("matched_ground_truth_id"),
            notes=raw.get("notes"),
        )
    return out


def _extractor_model_from_manifest(manifest: dict) -> Optional[str]:
    for c in manifest.get("claims") or []:
        prov = c.get("provenance") or {}
        ext = prov.get("extractor") or {}
        model = ext.get("model")
        if model:
            return model
    return None


def _extracted_at_from_manifest(manifest: dict) -> Optional[str]:
    """Return the extracted_at if all claims agree on it, else None.
    Codex F-WALK-CR-extracted_at: first-claim-wins is misleading when
    claims come from different runs."""
    seen: set[str] = set()
    for c in manifest.get("claims") or []:
        prov = c.get("provenance") or {}
        ext = prov.get("extractor") or {}
        v = ext.get("extracted_at")
        if v:
            seen.add(v)
    if len(seen) == 1:
        return next(iter(seen))
    return None


def walk_manifest(
    *,
    manifest_path: Path,
    curator: str,
    curation_log_path: Optional[Path] = None,
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

    ``curation_log_path`` (optional): if given, prior accept/drop
    records from that file are preserved into the new result —
    the cumulative-log discipline that prevents reruns from
    overwriting prior decisions (codex F-WALK-CR1 P1).
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
    prior_records = (
        _load_prior_records(curation_log_path)
        if curation_log_path is not None
        else {}
    )

    walk_started = _now_utc()
    records: list[ClaimReviewRecord] = []
    seen_cids: set[str] = set()
    quit_early = False

    for i, claim in enumerate(claims):
        cid = claim.get("id", "<missing-id>")
        seen_cids.add(cid)
        tier = claim.get("tier", "")

        prior = prior_records.get(cid)
        # If a prior accept exists, the claim is already curated;
        # carry the prior record forward as the authoritative one.
        if prior is not None and prior.decision == "accept":
            records.append(prior)
            continue
        # If the manifest reports tier > research but we have no
        # prior accept record, treat as an out-of-band promotion —
        # surface as already_curated for the curator's awareness.
        if tier != "research":
            if prior is not None:
                records.append(prior)
            else:
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
            # The current claim becomes a skip; remaining claims in
            # the manifest become 'unreviewed' so the aggregator's
            # denominator stays correct (codex F-WALK-CR3 P2).
            records.append(
                ClaimReviewRecord(
                    extracted_id=cid,
                    decision="skip",
                    started_at=_iso(started),
                    ended_at=_iso(_now_utc()),
                    notes="walkthrough quit early",
                )
            )
            for later in claims[i + 1:]:
                later_id = later.get("id", "<missing-id>")
                seen_cids.add(later_id)
                later_prior = prior_records.get(later_id)
                if later_prior is not None and later_prior.decision in (
                    "accept", "drop",
                ):
                    records.append(later_prior)
                else:
                    records.append(
                        ClaimReviewRecord(
                            extracted_id=later_id,
                            decision="unreviewed",
                            notes="walkthrough quit early before reaching this claim",
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
            # Compute the next-rung target from the promotion
            # ladder. Today this returns at most one option per
            # claim (research→ci or ci→release). The prompt
            # presents it explicitly so the curator can confirm.
            next_rung = curator_mod._adjacent_promotion_target(tier)
            valid_targets = [next_rung] if next_rung is not None else []
            to_tier = prompt_tier(valid_targets, tier)
            if not valid_targets or to_tier == tier:
                # Curator declined the confirm prompt, or no valid
                # target exists. Record as skip.
                ended = _now_utc()
                records.append(
                    ClaimReviewRecord(
                        extracted_id=cid,
                        decision="skip",
                        started_at=_iso(started),
                        ended_at=_iso(ended),
                        minutes_spent=_minutes(started, ended),
                        notes="curator declined promotion",
                    )
                )
                continue
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

    # Preserve prior drop records for claims no longer in the
    # manifest. Without this, the cumulative drop count would be
    # lost across reruns.
    for cid, prior in prior_records.items():
        if cid in seen_cids:
            continue
        if prior.decision in ("drop", "accept"):
            records.append(prior)

    walk_ended = _now_utc()
    return WalkthroughResult(
        artifact_id=str(artifact_id),
        curator=curator,
        extraction_started_at=_extracted_at_from_manifest(manifest) or "",
        extractor_model=_extractor_model_from_manifest(manifest),
        extracted_claims_count=len(records),
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
    ``experiments/phase5-extraction-rate/curation/_template.yaml``
    field names (codex F-WALK-CR2 P2).
    """
    n_accept = sum(1 for r in result.records if r.decision == "accept")
    n_drop = sum(1 for r in result.records if r.decision == "drop")
    n_skip = sum(1 for r in result.records if r.decision == "skip")
    n_unreviewed = sum(
        1 for r in result.records if r.decision == "unreviewed"
    )
    return {
        "artifact_id": result.artifact_id,
        "curator": result.curator,
        "extraction": {
            # Template field names: run_at, extractor_model,
            # extracted_claims_count, extracted_rejections_count.
            "run_at": result.extraction_started_at or None,
            "extractor_model": result.extractor_model,
            "extracted_claims_count": result.extracted_claims_count,
            "extracted_rejections_count": None,  # surfaced via EXTRACTION.md
        },
        "curation": {
            "started_at": result.walkthrough_started_at,
            "ended_at": result.walkthrough_ended_at,
            "minutes_total": round(result.minutes_total, 4),
            "quit_early": result.quit_early,
            "accepted_count": n_accept,
            "dropped_count": n_drop,
            "skipped_count": n_skip,
            "unreviewed_count": n_unreviewed,
        },
        "extracted_claims": [
            {
                "extracted_id": r.extracted_id,
                "matched_ground_truth_id": r.matched_ground_truth_id,
                "decision": r.decision,
                "to_tier": r.to_tier,
                "rationale": r.rationale,
                "minutes_spent": round(r.minutes_spent, 4),
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
