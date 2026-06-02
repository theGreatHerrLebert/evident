"""Artifact scoring — extract observed metric values from a claim's artifact.

Imports proteon's ``claim_scoring.py`` when available; falls back to a
minimal JSON/JSONL+dotted-path reader otherwise.

For Phase 1 the agent only needs the **primary observed value** (the
scalar that goes into the sidecar's ``value`` field, which typed-trust
binds to the first criterion). Per-criterion multi-value support is a
Phase 1.5 extension that lands when typed-trust grows a richer sidecar
format.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, Optional


def _try_proteon_scoring(claim: dict, source_dir: Path) -> Optional[float]:
    """Use proteon's claim_scoring.score_claim when available.

    Returns the first tolerance's observed value, or None if scoring
    is unavailable or returns no observation.
    """
    # Search for proteon's tools directory.
    candidates = [
        source_dir / "evident" / "tools",
        Path("/scratch/TMAlign/proteon/evident/tools"),
    ]
    tools_dir = next((c for c in candidates if c.is_dir()), None)
    if tools_dir is None:
        return None
    sys.path.insert(0, str(tools_dir))
    try:
        from claim_scoring import score_claim  # type: ignore

        score = score_claim(claim, base_dir=source_dir)
        if not score.tolerances:
            return None
        # First tolerance's observed value is the primary observation.
        first = score.tolerances[0]
        return float(first.observed) if first.observed is not None else None
    except Exception:
        return None
    finally:
        if str(tools_dir) in sys.path:
            sys.path.remove(str(tools_dir))


def _resolve_artifact_path(artifact_str: str, source_dir: Path) -> Optional[Path]:
    """Extract the first path-like token from evidence.artifact and resolve."""
    # Manifests often write things like
    #   "validation/results.json (archived in v0.2.0-evidence.tar.gz release asset)"
    # — take the first whitespace-separated token.
    token = artifact_str.strip().split()[0] if artifact_str.strip() else ""
    if not token:
        return None
    candidate = source_dir / token
    if candidate.is_file():
        return candidate
    return None


def _fallback_extract(claim: dict, source_dir: Path) -> Optional[float]:
    """Minimal fallback: read the artifact as JSON, look for a known key.

    Supports the simplest case where the artifact is a JSON document
    with a top-level scalar named ``value``, ``observed``, or
    ``primary_metric``. For anything more complex, the agent needs
    proteon's claim_scoring.
    """
    evidence = claim.get("evidence") or {}
    artifact_str = evidence.get("artifact") or ""
    artifact_path = _resolve_artifact_path(artifact_str, source_dir)
    if artifact_path is None:
        return None
    try:
        payload = json.loads(artifact_path.read_text())
    except Exception:
        return None
    if not isinstance(payload, dict):
        return None
    for key in ("value", "observed", "primary_metric"):
        v = payload.get(key)
        if isinstance(v, (int, float)):
            return float(v)
    return None


def extract_primary_observation(claim: dict, source_dir: Path) -> Optional[float]:
    """Extract the primary observed value for a claim's artifact.

    Tries proteon's claim_scoring first (handles JSONL, dotted paths,
    aggregations, where-clauses); falls back to a simple JSON reader.
    Returns ``None`` if no value can be extracted — the agent then
    writes a sidecar entry with ``value: null``, which downstream
    surfaces as ``NotAssessed`` in the report.
    """
    proteon_result = _try_proteon_scoring(claim, source_dir)
    if proteon_result is not None:
        return proteon_result
    return _fallback_extract(claim, source_dir)
