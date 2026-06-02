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
    """A single manifest claim paired with its source path and in-file index.

    ``source_path``: the YAML file the claim was authored in (the include
    file, when the claim came in via ``include:``).
    ``top_manifest_path``: the top-level manifest the agent was invoked
    with. Per workflow/SCHEMA.md, the claim's ``source`` field resolves
    relative to the top manifest's directory — NOT the include file's
    directory.
    """

    id: str
    kind: str
    tier: str
    source_path: Path
    top_manifest_path: Path
    span: str
    raw: dict

    def source_dir(self) -> Path:
        """Resolve the claim's ``source`` field to an absolute path.

        Schema-correct resolution: the ``source`` field is relative to
        the top-level manifest's directory.
        """
        rel = self.raw.get("source") or "."
        return (self.top_manifest_path.parent / rel).resolve()


def load_claims(manifest_path: Path) -> List[ClaimRecord]:
    """Load all claims from a manifest, resolving any ``include:`` paths
    relative to the manifest's directory. Returns ``ClaimRecord``s with
    BOTH the originating file path (for SourceSpan) AND the top manifest
    path (for ``source`` resolution per schema).
    """
    import yaml

    manifest_path = manifest_path.resolve()
    text = manifest_path.read_text()
    parsed = yaml.safe_load(text) or {}
    out: List[ClaimRecord] = []

    for idx, claim in enumerate(parsed.get("claims") or []):
        out.append(_make_record(claim, manifest_path, manifest_path, idx))

    for inc in parsed.get("include") or []:
        inc_path = (manifest_path.parent / inc).resolve()
        inc_parsed = yaml.safe_load(inc_path.read_text()) or {}
        for idx, claim in enumerate(inc_parsed.get("claims") or []):
            out.append(_make_record(claim, inc_path, manifest_path, idx))

    return out


def _make_record(
    claim: dict,
    source_path: Path,
    top_manifest_path: Path,
    idx: int,
) -> ClaimRecord:
    return ClaimRecord(
        id=claim.get("id", "(unknown)"),
        kind=claim.get("kind", "measurement"),
        tier=claim.get("tier", "research"),
        source_path=source_path,
        top_manifest_path=top_manifest_path,
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
