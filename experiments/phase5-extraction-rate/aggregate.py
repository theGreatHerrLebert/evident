#!/usr/bin/env python3
"""Aggregate the per-artifact ground-truth + curation YAML into the
results.md decision table.

Usage:
    python3 aggregate.py [--write-results]

Reads:
    ground-truth/<id>.yaml
    curation/<id>.yaml

Computes per source-type (repo / paper) and overall:
    - recall: |extracted ∩ ground_truth| / |ground_truth|
    - precision: |curator_accepted| / |extracted_total|
    - claims_per_artifact_median
    - minutes_per_accepted_claim_median
    - replay_path_distribution
    - rejections_quality_distribution

Compares against the codex-recommended thresholds:
    - claims_per_artifact_median >= 2
    - minutes_per_accepted_claim_median:
        repo: <= 5
        paper: <= 10..15
        mixed: <= 10

Outputs either a pretty-printed report (default) or writes
results.md if --write-results is given.
"""

from __future__ import annotations

import argparse
import sys
from collections import defaultdict
from pathlib import Path
from statistics import median

try:
    import yaml
except ImportError:  # pragma: no cover
    sys.stderr.write(
        "PyYAML is required: pip install pyyaml\n"
    )
    sys.exit(2)


HERE = Path(__file__).resolve().parent


REPO_MINUTES_THRESHOLD = 5
PAPER_MINUTES_THRESHOLD_LOW = 10
PAPER_MINUTES_THRESHOLD_HIGH = 15
MIXED_MINUTES_THRESHOLD = 10
CLAIMS_PER_ARTIFACT_THRESHOLD = 2


def _load_yaml(p: Path) -> dict:
    with p.open(encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def _load_pair(artifact_id: str) -> tuple[dict, dict] | None:
    gt = HERE / "ground-truth" / f"{artifact_id}.yaml"
    cu = HERE / "curation" / f"{artifact_id}.yaml"
    if not gt.is_file() or not cu.is_file():
        return None
    return _load_yaml(gt), _load_yaml(cu)


def _format(artifact_id: str) -> str:
    return artifact_id


def _source_type(gt: dict) -> str:
    fmt = (gt.get("artifact_format") or "").strip()
    if fmt.startswith("repo"):
        return "repo"
    if fmt.startswith("paper"):
        return "paper"
    return "unknown"


def _recall(gt: dict, cu: dict) -> float | None:
    truth = gt.get("load_bearing_claims") or []
    if not truth:
        return None
    matched = {
        c.get("matched_ground_truth_id")
        for c in (cu.get("extracted_claims") or [])
        if c.get("matched_ground_truth_id")
    }
    return len(matched) / len(truth)


def _precision(cu: dict) -> float | None:
    extracted = cu.get("extracted_claims") or []
    if not extracted:
        return None
    accepted = [c for c in extracted if c.get("decision") == "accept"]
    return len(accepted) / len(extracted)


def _curator_minutes_per_accepted(cu: dict) -> float | None:
    total = (cu.get("curation") or {}).get("minutes_total")
    if total is None:
        return None
    accepted = [
        c for c in (cu.get("extracted_claims") or [])
        if c.get("decision") == "accept"
    ]
    if not accepted:
        return None
    return total / len(accepted)


def _approved_per_artifact(cu: dict) -> int:
    return sum(
        1 for c in (cu.get("extracted_claims") or [])
        if c.get("decision") == "accept"
    )


def aggregate(artifact_ids: list[str]) -> dict:
    by_type: dict[str, list] = defaultdict(list)
    overall: list = []
    rejection_quality: dict[str, int] = defaultdict(int)
    for aid in artifact_ids:
        pair = _load_pair(aid)
        if pair is None:
            continue
        gt, cu = pair
        st = _source_type(gt)
        row = {
            "artifact_id": aid,
            "source_type": st,
            "recall": _recall(gt, cu),
            "precision": _precision(cu),
            "approved": _approved_per_artifact(cu),
            "minutes_per_accepted": _curator_minutes_per_accepted(cu),
        }
        by_type[st].append(row)
        overall.append(row)
        for r in (cu.get("rejections_audit") or []):
            rejection_quality[r.get("rejection_quality") or "unknown"] += 1
    summaries = {}
    for st in list(by_type.keys()) + ["overall"]:
        rows = overall if st == "overall" else by_type[st]
        if not rows:
            continue
        accepted_counts = [r["approved"] for r in rows if r["approved"] is not None]
        minutes_per = [
            r["minutes_per_accepted"] for r in rows
            if r["minutes_per_accepted"] is not None
        ]
        recalls = [r["recall"] for r in rows if r["recall"] is not None]
        precisions = [r["precision"] for r in rows if r["precision"] is not None]
        summaries[st] = {
            "artifact_count": len(rows),
            "approved_per_artifact_median": (
                median(accepted_counts) if accepted_counts else None
            ),
            "minutes_per_accepted_median": (
                median(minutes_per) if minutes_per else None
            ),
            "recall_median": median(recalls) if recalls else None,
            "precision_median": (
                median(precisions) if precisions else None
            ),
        }
    return {
        "rows": overall,
        "summaries": summaries,
        "rejection_quality": dict(rejection_quality),
    }


def _decision(summaries: dict) -> dict:
    """Apply the codex-recommended stop/go thresholds."""
    decisions = {}
    for st, s in summaries.items():
        approved = s.get("approved_per_artifact_median")
        minutes = s.get("minutes_per_accepted_median")
        approved_ok = approved is not None and approved >= CLAIMS_PER_ARTIFACT_THRESHOLD
        if st == "repo":
            minutes_ok = minutes is not None and minutes <= REPO_MINUTES_THRESHOLD
        elif st == "paper":
            minutes_ok = (
                minutes is not None and minutes <= PAPER_MINUTES_THRESHOLD_HIGH
            )
        elif st == "overall":
            minutes_ok = (
                minutes is not None and minutes <= MIXED_MINUTES_THRESHOLD
            )
        else:
            minutes_ok = False
        decisions[st] = {
            "approved_ok": approved_ok,
            "minutes_ok": minutes_ok,
            "verdict": "go" if approved_ok and minutes_ok else "stop",
        }
    return decisions


def render(summary: dict) -> str:
    out = ["# Phase 5 extraction-rate experiment — results\n"]
    out.append(
        "Automatic aggregate. Source-of-truth is the per-artifact "
        "ground-truth/<id>.yaml + curation/<id>.yaml files.\n"
    )
    if not summary["rows"]:
        out.append(
            "_No artifacts processed yet. Hand-label ground truth, "
            "run the extractor, write curation files, then re-run.\n_"
        )
        return "\n".join(out)
    out.append("## Per-artifact rows\n")
    out.append(
        "| artifact | type | recall | precision | approved | min/claim |"
    )
    out.append(
        "|---|---|---|---|---|---|"
    )
    for r in summary["rows"]:
        def f(x, fmt="{:.2f}"):
            return fmt.format(x) if x is not None else "—"
        out.append(
            f"| `{r['artifact_id']}` | {r['source_type']} | "
            f"{f(r['recall'])} | {f(r['precision'])} | "
            f"{r['approved']} | {f(r['minutes_per_accepted'], '{:.1f}')} |"
        )
    out.append("")
    out.append("## Source-type aggregates\n")
    out.append(
        "| source | n | approved/artifact (median) | min/accepted (median) "
        "| recall (median) | precision (median) |"
    )
    out.append("|---|---|---|---|---|---|")
    for st, s in summary["summaries"].items():
        def f(x, fmt="{:.2f}"):
            return fmt.format(x) if x is not None else "—"
        out.append(
            f"| {st} | {s['artifact_count']} | "
            f"{f(s['approved_per_artifact_median'], '{:.1f}')} | "
            f"{f(s['minutes_per_accepted_median'], '{:.1f}')} | "
            f"{f(s['recall_median'])} | "
            f"{f(s['precision_median'])} |"
        )
    out.append("")
    out.append("## Codex stop/go decision\n")
    decisions = _decision(summary["summaries"])
    out.append(
        "| source | approved ≥ 2 | minutes ok | verdict |"
    )
    out.append("|---|---|---|---|")
    for st, d in decisions.items():
        out.append(
            f"| {st} | {'✓' if d['approved_ok'] else '✗'} | "
            f"{'✓' if d['minutes_ok'] else '✗'} | "
            f"**{d['verdict'].upper()}** |"
        )
    out.append("")
    if summary["rejection_quality"]:
        out.append("## Rejection-quality audit\n")
        for k, v in sorted(summary["rejection_quality"].items()):
            out.append(f"- **{k}**: {v}")
    return "\n".join(out) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write-results", action="store_true")
    args = ap.parse_args()
    ground_truth_dir = HERE / "ground-truth"
    ids = sorted(
        p.stem for p in ground_truth_dir.glob("*.yaml")
        if not p.stem.startswith("_")
    )
    summary = aggregate(ids)
    rendered = render(summary)
    if args.write_results:
        (HERE / "results.md").write_text(rendered, encoding="utf-8")
        print(f"wrote {HERE / 'results.md'}")
    else:
        print(rendered)
    return 0


if __name__ == "__main__":
    sys.exit(main())
