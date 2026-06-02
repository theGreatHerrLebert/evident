"""Manifest loading + claim selection.

The framework's ``workflow/validate_manifest._collect_claims`` returns a
flat list of dict claims with no per-claim source path, so for the
agent's needs (knowing which file each claim came from to feed
typed-trust's SourceSpan) we read manifests directly with PyYAML.

Schema validation is the framework's job; typed-trust will also catch
malformed inputs at translation time. The agent's reader is therefore
deliberately permissive — fail downstream, not here.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Iterator, List, Optional


@dataclass
class ClaimRecord:
    """A single manifest claim paired with its source path and in-file index."""

    id: str
    kind: str
    tier: str
    source_path: Path
    span: str
    raw: dict


def load_claims(manifest_path: Path) -> List[ClaimRecord]:
    """Load all claims from a manifest, resolving any ``include:`` paths
    relative to the manifest's directory. Returns ``ClaimRecord``s with
    the originating file path preserved so typed-trust's SourceSpan
    points at the right location.
    """
    import yaml

    text = manifest_path.read_text()
    parsed = yaml.safe_load(text) or {}
    out: List[ClaimRecord] = []

    for idx, claim in enumerate(parsed.get("claims") or []):
        out.append(_make_record(claim, manifest_path, idx))

    for inc in parsed.get("include") or []:
        inc_path = manifest_path.parent / inc
        inc_parsed = yaml.safe_load(inc_path.read_text()) or {}
        for idx, claim in enumerate(inc_parsed.get("claims") or []):
            out.append(_make_record(claim, inc_path, idx))

    return out


def _make_record(claim: dict, source_path: Path, idx: int) -> ClaimRecord:
    return ClaimRecord(
        id=claim.get("id", "(unknown)"),
        kind=claim.get("kind", "measurement"),
        tier=claim.get("tier", "research"),
        source_path=source_path,
        span=f"claims[{idx}]",
        raw=claim,
    )


def filter_claims(
    claims: List[ClaimRecord],
    claim_filter: Optional[str] = None,
    kind: Optional[str] = "measurement",
) -> Iterator[ClaimRecord]:
    """Filter to claims with the given id and/or kind.

    Defaults to ``kind="measurement"`` since the agent's job is to
    populate measurement claims' observations. Pass ``kind=None`` to
    include all kinds.
    """
    for c in claims:
        if claim_filter is not None and c.id != claim_filter:
            continue
        if kind is not None and c.kind != kind:
            continue
        yield c
