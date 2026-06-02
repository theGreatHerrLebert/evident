"""Phase 5 PR4: output writer for an extracted/<artifact-id>/ tree.

After the model emits ``submit_extracted_claims`` and the validator
approves / rejects each tolerance, this module writes the three
output files the curator reviews:

- ``evident.yaml`` — the draft manifest in the schema typed-trust
  already understands (with the Phase 5 PR1–3 additions:
  ``replay_status``, structured ``provenance``, etc.).
- ``source/cited.md`` — the per-claim source_span citations, so the
  curator can audit each tolerance against the source.
- ``EXTRACTION.md`` — human-readable summary of accepted + rejected
  candidates, with structured rejection reasons. The rejections
  list is just as important as the claims — the curator needs to
  see what the extractor decided NOT to commit to.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable

import yaml


# ---------------------------------------------------------------------
# Result types — the extractor produces these; render consumes them
# ---------------------------------------------------------------------


@dataclass
class ExtractedClaim:
    """One claim that passed the validator and will appear in the
    draft manifest."""

    id: str
    title: str
    claim: str
    subject_aliases: list[str]
    tolerances: list[dict]


@dataclass
class RejectedCandidate:
    """A candidate the extractor (or validator) rejected. Goes into
    EXTRACTION.md for the curator's review."""

    candidate_text: str
    locator: str
    reason: str
    rationale: str


@dataclass
class ExtractionResult:
    """Bundle of everything an extraction run produces."""

    source_id: str
    source_sha: str
    extractor_model: str
    extracted_at: str
    claims: list[ExtractedClaim] = field(default_factory=list)
    rejections: list[RejectedCandidate] = field(default_factory=list)


# ---------------------------------------------------------------------
# Writers
# ---------------------------------------------------------------------


def build_manifest_dict(
    result: ExtractionResult,
    project: str,
) -> dict:
    """Build the draft manifest dict in the schema typed-trust
    understands. tier:research is always used at extraction time;
    promotion requires a PromoteFromExtracted event later.
    """
    return {
        "version": "0.1",
        "project": project,
        "claims": [_claim_block(c, result) for c in result.claims],
    }


def _claim_block(claim: ExtractedClaim, result: ExtractionResult) -> dict:
    return {
        "id": claim.id,
        "title": claim.title,
        "kind": "measurement",
        "tier": "research",
        "source": "source/cited.md",
        "case": f"source/cited.md#{claim.id}",
        "claim": claim.claim,
        "tolerances": [_tolerance_block(t) for t in claim.tolerances],
        "evidence": {
            "oracle": ["Paper-Authority"],
            "command": "no-replay-path",
            "artifact": f"source/cited.md#{claim.id}",
            # Phase 5 PR1 fields — extracted claims default to
            # unavailable_artifacts; the curator can change this if
            # the source IS replayable.
            "replay_status": "unavailable_artifacts",
            "replay_reason": "code_private",
        },
        "provenance": {
            "kind": "extracted-from-paper",
            "source_id": result.source_id,
            "source_sha": result.source_sha,
            # source_context is set by the walker (PR5/PR6) when it
            # detects copied marketing text; default unknown here so
            # the curator notices.
            "source_context": "unknown",
            "extractor": {
                "model": result.extractor_model,
                "extracted_at": result.extracted_at,
            },
            "curator": None,
        },
        "last_verified": {
            "commit": None,
            "date": None,
            "value": None,
            "corpus_sha": None,
        },
    }


def _tolerance_block(t: dict) -> dict:
    """Lift the model's tolerance dict into the manifest schema. The
    extractor-side fields (``source_span``, ``subject_aliases``)
    travel as auxiliary text in ``prose`` so the curator sees them
    without needing extractor-internal fields in the manifest.
    """
    prose_parts = []
    if t.get("prose"):
        prose_parts.append(t["prose"].strip())
    if t.get("source_span"):
        prose_parts.append(f"source_span: {t['source_span'].strip()!r}")
    return {
        "metric": t["metric"],
        "op": t["op"],
        "value": t["value"],
        "prose": "\n".join(prose_parts),
    }


def render_cited_md(result: ExtractionResult) -> str:
    """Render the ``cited.md`` file: per-claim source_span quotes the
    curator can audit. Anchors (``#<claim-id>``) make each claim's
    citation linkable from the manifest's ``case`` field.
    """
    lines: list[str] = ["# Extracted-claim citations\n"]
    lines.append(
        f"Source: `{result.source_id}` (sha256: `{result.source_sha}`).\n"
    )
    lines.append(
        f"Extracted by `{result.extractor_model}` at "
        f"`{result.extracted_at}`.\n"
    )
    if not result.claims:
        lines.append("\n_No claims extracted from this source._\n")
        return "\n".join(lines)
    for claim in result.claims:
        lines.append(f"\n## <a id=\"{claim.id}\"></a>{claim.id}\n")
        lines.append(f"**{claim.title}**\n")
        lines.append(claim.claim.strip())
        for i, t in enumerate(claim.tolerances):
            lines.append(
                f"\n### Tolerance {i}: `{t['metric']} {t['op']} {t['value']}`\n"
            )
            lines.append(
                f"> {t.get('source_span', '').strip()}"
            )
            if t.get("prose"):
                lines.append(f"\n_Prose:_ {t['prose'].strip()}")
    return "\n".join(lines) + "\n"


def render_extraction_md(result: ExtractionResult) -> str:
    """Render the human-readable summary. Accepted + rejected
    candidates with structured reasons.
    """
    lines: list[str] = ["# Extraction summary\n"]
    lines.append(
        f"Source: `{result.source_id}`  \n"
        f"Source sha256: `{result.source_sha}`  \n"
        f"Extractor: `{result.extractor_model}` "
        f"({result.extracted_at})  \n\n"
    )
    lines.append(f"## Accepted claims ({len(result.claims)})\n")
    if result.claims:
        for c in result.claims:
            lines.append(
                f"- **{c.id}** — {c.title}  \n"
                f"  Tolerances: {len(c.tolerances)}; "
                f"subject aliases: {c.subject_aliases}"
            )
    else:
        lines.append(
            "_No claims extracted. Default-deny framing rejected "
            "all candidates._"
        )

    lines.append(f"\n## Rejected candidates ({len(result.rejections)})\n")
    if result.rejections:
        # Group by reason so the curator can scan failure modes.
        by_reason: dict[str, list[RejectedCandidate]] = {}
        for r in result.rejections:
            by_reason.setdefault(r.reason, []).append(r)
        for reason in sorted(by_reason.keys()):
            entries = by_reason[reason]
            lines.append(f"\n### `{reason}` ({len(entries)})\n")
            for r in entries:
                lines.append(
                    f"- {r.locator}: {r.candidate_text!r}  \n"
                    f"  _Reason:_ {r.rationale}"
                )
    else:
        lines.append("_No rejected candidates recorded._")
    return "\n".join(lines) + "\n"


def write_outputs(
    result: ExtractionResult,
    output_dir: Path,
    project: str,
) -> None:
    """Write the three output files into ``output_dir``.

    Creates ``output_dir`` and ``output_dir/source/`` if needed.
    Overwrites existing files (the extractor is idempotent against
    the same source).
    """
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "source").mkdir(parents=True, exist_ok=True)

    manifest = build_manifest_dict(result, project=project)
    (output_dir / "evident.yaml").write_text(
        yaml.safe_dump(manifest, sort_keys=False, default_flow_style=False),
        encoding="utf-8",
    )
    (output_dir / "source" / "cited.md").write_text(
        render_cited_md(result), encoding="utf-8"
    )
    (output_dir / "EXTRACTION.md").write_text(
        render_extraction_md(result), encoding="utf-8"
    )


def now_utc_isoformat() -> str:
    """Return the current UTC time in ISO-8601 with Z suffix.

    Used as a default for ``extracted_at`` when the caller does not
    pin a specific timestamp.
    """
    return (
        datetime.now(tz=timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )
