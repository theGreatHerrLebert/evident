"""Phase 5 PR5: dry-run source audit output.

When ``--dry-run`` is passed to ``evident-agent extract``, the
walker runs but the model is NOT called. This module writes a
human-readable ``EXTRACTION.md`` and a structured ``dry_run.json``
into the output dir so a curator can audit:

- which files were included / skipped (and why)
- which external citations were detected and redacted
- the estimated input size that WOULD have been sent to the model
- explicit "no model call was made" language so the output is
  never mistaken for a real negative extraction

Codex v1 review of PR5 v1 made this load-bearing: a dry-run that
silently writes a normal-looking manifest with zero claims is a
false-confidence trap.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

from .repo import WalkedSource, assemble_for_model


def _section_summary(walked: WalkedSource) -> list[dict]:
    """Per-section audit fields.

    Codex F-PR5-CR5 (P2): also include a sha256 of the RAW
    (pre-redaction) section text so a curator running a later
    live-extraction can detect whether the source changed in
    spans that the dry-run audit hid via redaction.
    """
    return [
        {
            "path": s.path,
            "kind": s.kind,
            "raw_bytes": len(s.text_raw.encode("utf-8")),
            "raw_sha256": hashlib.sha256(
                s.text_raw.encode("utf-8")
            ).hexdigest(),
            "post_redaction_bytes": len(s.text.encode("utf-8")),
            "post_redaction_sha256": hashlib.sha256(
                s.text.encode("utf-8")
            ).hexdigest(),
            "truncated": s.truncated,
        }
        for s in walked.sections
    ]


def _redaction_summary(walked: WalkedSource) -> list[dict]:
    return [
        {
            "section": r.section_path,
            "span_start": r.span_start,
            "span_end": r.span_end,
            "reason": r.reason,
            "original_preview": r.original[:200],
        }
        for r in walked.redactions
    ]


def build_dry_run_payload(walked: WalkedSource) -> dict:
    """Build the structured dry_run.json payload."""
    assembled = assemble_for_model(walked)
    return {
        "mode": "dry_run",
        "no_model_call_was_made": True,
        "source": {
            "source_id": walked.source_id,
            "source_sha": walked.source_sha,
        },
        "sections_included": _section_summary(walked),
        "files_skipped": [
            {
                "path": s.path,
                "reason": s.reason,
                "size_bytes": s.size_bytes,
            }
            for s in walked.skipped
        ],
        "redactions": _redaction_summary(walked),
        "notes": walked.notes,
        "assembled_text_for_model": {
            "size_bytes": len(assembled.encode("utf-8")),
            "sha256": hashlib.sha256(
                assembled.encode("utf-8")
            ).hexdigest(),
        },
    }


_DRY_RUN_NOTICE = (
    "No model call was made; no claims were extracted or validated."
)


def render_audit_markdown(walked: WalkedSource) -> str:
    """Human-readable ``EXTRACTION.md`` for dry-run mode."""
    lines: list[str] = ["# Extraction source audit (dry run)\n"]
    lines.append(f"**{_DRY_RUN_NOTICE}**\n")
    lines.append(
        f"Source id: `{walked.source_id}`  \n"
        f"Source sha: `{walked.source_sha}`  \n"
    )

    lines.append("\n## Files included\n")
    if walked.sections:
        for s in walked.sections:
            note = " *(truncated)*" if s.truncated else ""
            raw_kb = len(s.text_raw.encode("utf-8")) / 1024
            lines.append(
                f"- `{s.path}` ({s.kind}, "
                f"{raw_kb:.1f} KiB raw){note}"
            )
    else:
        lines.append(
            "_No allow-listed text files found in this repo._"
        )

    lines.append("\n## Files skipped\n")
    if walked.skipped:
        # Group by reason so the audit reads at a glance.
        by_reason: dict[str, list] = {}
        for s in walked.skipped:
            by_reason.setdefault(s.reason, []).append(s)
        for reason in sorted(by_reason):
            lines.append(f"\n### `{reason}` ({len(by_reason[reason])})\n")
            for s in by_reason[reason]:
                size = (
                    f" ({s.size_bytes} bytes)" if s.size_bytes else ""
                )
                lines.append(f"- `{s.path}`{size}")
    else:
        lines.append("_No files were skipped._")

    lines.append("\n## External citations redacted\n")
    if walked.redactions:
        by_kind: dict[str, list] = {}
        for r in walked.redactions:
            by_kind.setdefault(r.reason, []).append(r)
        for kind in sorted(by_kind):
            lines.append(f"\n### `{kind}` ({len(by_kind[kind])})\n")
            for r in by_kind[kind]:
                preview = r.original.strip().replace("\n", " ")
                if len(preview) > 120:
                    preview = preview[:117] + "..."
                lines.append(f"- `{r.section_path}`: `{preview}`")
    else:
        lines.append(
            "_No external citations detected in the included files._"
        )

    if walked.notes:
        lines.append("\n## Notes\n")
        for n in walked.notes:
            lines.append(f"- {n}")

    lines.append("")
    lines.append(
        "---\n\n"
        f"_{_DRY_RUN_NOTICE}_  \n"
        "_To run a real extraction, omit `--dry-run`._\n"
    )
    return "\n".join(lines) + "\n"


def write_dry_run_outputs(
    walked: WalkedSource,
    output_dir: Path,
) -> None:
    """Write the audit-mode outputs into ``output_dir``.

    Crucially does NOT write ``evident.yaml`` — a dry-run output
    directory cannot be confused for a real extraction.
    """
    output_dir.mkdir(parents=True, exist_ok=True)
    payload = build_dry_run_payload(walked)
    (output_dir / "dry_run.json").write_text(
        json.dumps(payload, indent=2, sort_keys=False),
        encoding="utf-8",
    )
    (output_dir / "EXTRACTION.md").write_text(
        render_audit_markdown(walked),
        encoding="utf-8",
    )
