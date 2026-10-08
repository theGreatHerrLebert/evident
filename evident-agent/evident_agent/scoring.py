"""Artifact scoring — extract observed metric values from a claim's artifact.

Imports the project's ``evident/tools/claim_scoring.py`` (proteon ships
one) when available; falls back to a minimal JSON reader otherwise.

Each tolerance that names an ``output`` can get its own observed value,
written to the sidecar's ``values`` map keyed by that output, so
typed-trust assesses every criterion. The first tolerance's value is
also the primary ``value``, which older readers bind to the first
criterion.
"""

from __future__ import annotations

import contextlib
import importlib.util
import json
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, Iterator, List, Optional


@dataclass
class Observations:
    """What scoring recovered for one claim."""

    value: Optional[float] = None
    """Primary observation: the first tolerance's value."""
    values: Dict[str, float] = field(default_factory=dict)
    """Observed value per tolerance ``output``."""
    ignored: List[str] = field(default_factory=list)
    """Outputs found that typed-trust cannot bind to exactly one criterion."""
    conflict: Optional[str] = None
    """Set when ``value`` and ``values`` disagree about the first criterion;
    typed-trust rejects such an entry, so replay must not write it."""

    def __bool__(self) -> bool:
        return self.value is not None or bool(self.values)


def _number(v: Any) -> Optional[float]:
    """A finite float, or None for anything else (bools included)."""
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        return None
    try:
        v = float(v)
    except OverflowError:  # an int too large for a float, e.g. 10**400
        return None
    return v if math.isfinite(v) else None


def _structured(t: Any) -> bool:
    """A tolerance typed-trust turns into a checkable criterion. Prose-only
    tolerances (allowed at research tier) translate to none, so their
    ``output`` cannot carry an observation."""
    # typed-trust reads a null field as absent, so `metric: null` is
    # prose-only, and a partial triple fails translation of the whole claim.
    return isinstance(t, dict) and all(t.get(k) is not None for k in ("metric", "op", "value"))


def _declared_outputs(claim: dict) -> List[Optional[str]]:
    """Each tolerance's ``output``, in order (None where not declared)."""
    return [
        t.get("output") if isinstance(t, dict) else None
        for t in claim.get("tolerances") or []
    ]


def bindable_outputs(claim: dict) -> List[str]:
    """Outputs typed-trust binds to exactly one criterion: declared by
    exactly one structured tolerance. Mirrors translate_last_verified."""
    outs = [
        t.get("output")
        for t in claim.get("tolerances") or []
        if _structured(t) and t.get("output")
    ]
    return [o for o in outs if outs.count(o) == 1]


def _first_output(claim: dict) -> Optional[str]:
    """The output ``value`` would also bind to, if any."""
    tols = claim.get("tolerances") or []
    if not tols or not _structured(tols[0]):
        return None
    out = tols[0].get("output")
    return out if out in bindable_outputs(claim) else None


def _finish(obs: Observations, claim: dict) -> Observations:
    """Keep only what typed-trust will accept for this claim."""
    bindable = set(bindable_outputs(claim))
    for key in [k for k in obs.values if k not in bindable]:
        obs.ignored.append(key)
        del obs.values[key]
    first = _first_output(claim)
    if obs.value is not None and first in obs.values and obs.values[first] != obs.value:
        obs.conflict = (
            f"value {obs.value} and values[{first!r}] {obs.values[first]} "
            f"disagree about the first criterion"
        )
    return obs


@contextlib.contextmanager
def _isolated_import_path(tools_dir: Path) -> Iterator[None]:
    """Put ``tools_dir`` first on sys.path, and afterwards forget every
    module newly imported from it, so a later call for another project
    re-imports its own helpers. A helper whose name the host process had
    already imported is still served from the host's cache: Python's
    import system offers no clean way around that."""
    before = set(sys.modules)
    sys.path.insert(0, str(tools_dir))
    try:
        yield
    finally:
        if str(tools_dir) in sys.path:
            sys.path.remove(str(tools_dir))
        root = tools_dir.resolve()
        for name in set(sys.modules) - before:
            if name.startswith(_SCORER_PREFIX) or _inside(
                getattr(sys.modules[name], "__file__", None), root
            ):
                sys.modules.pop(name, None)


def _inside(path: Optional[str], root: Path) -> bool:
    """True if ``path`` resolves to a file under ``root``."""
    if not path:
        return False
    try:
        return Path(path).resolve().is_relative_to(root)
    except (OSError, ValueError):
        return False


_SCORER_PREFIX = "_evident_project_claim_scoring_"


def _try_project_scoring(claim: dict, source_dir: Path) -> Optional[Observations]:
    """Use the project's ``evident/tools/claim_scoring.score_claim`` when
    available. Returns None if there is none, it fails, or it scores
    nothing, so the JSON fallback can still run.

    The scorer is loaded from its file under a private module name, never
    as ``claim_scoring``, so a module of that name already imported by the
    host is neither used nor evicted.
    """
    tools_dir = source_dir / "evident" / "tools"
    path = tools_dir / "claim_scoring.py"
    if not path.is_file():
        return None
    name = _SCORER_PREFIX + str(abs(hash(str(path.resolve()))))
    with _isolated_import_path(tools_dir):
        try:
            spec = importlib.util.spec_from_file_location(name, path)
            module = importlib.util.module_from_spec(spec)
            sys.modules[name] = module  # dataclasses look their module up here
            spec.loader.exec_module(module)
            # Positional: proteon names the parameter ``repo_root``. The
            # earlier ``base_dir=`` keyword raised TypeError on every call,
            # which was swallowed as "no observation".
            score = module.score_claim(claim, source_dir)
        except Exception:
            return None
    outputs = _declared_outputs(claim)
    obs = Observations()
    for i, t in enumerate(getattr(score, "tolerances", None) or []):
        v = _number(getattr(t, "observed", None))
        if v is None:
            continue
        if i == 0:
            obs.value = v
        output = getattr(t, "output", None) or (outputs[i] if i < len(outputs) else None)
        if output:
            obs.values[output] = v
    return obs if obs else None


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


def _fallback_extract(claim: dict, source_dir: Path) -> Observations:
    """Minimal fallback: read the artifact as a JSON object.

    The primary value comes from a top-level ``value``, ``observed`` or
    ``primary_metric`` scalar; per-tolerance values from a top-level
    ``values`` object keyed by tolerance ``output``. For anything more
    complex, the project needs its own ``evident/tools/claim_scoring.py``.
    """
    obs = Observations()
    evidence = claim.get("evidence") or {}
    artifact_str = evidence.get("artifact") or ""
    artifact_path = _resolve_artifact_path(artifact_str, source_dir)
    if artifact_path is None:
        return obs
    try:
        payload = json.loads(artifact_path.read_text())
    except Exception:
        return obs
    if not isinstance(payload, dict):
        return obs
    for key in ("value", "observed", "primary_metric"):
        v = _number(payload.get(key))
        if v is not None:
            obs.value = v
            break
    raw_values = payload.get("values")
    if isinstance(raw_values, dict):
        for k, raw in raw_values.items():
            v = _number(raw)
            if v is not None:
                obs.values[str(k)] = v
    return obs


def extract_observations(claim: dict, source_dir: Path) -> Observations:
    """Extract every observed value scoring can recover for a claim.

    Tries the project's claim_scoring first; falls back to the JSON
    reader. An empty result means nothing could be extracted. Outputs typed-trust
    could not bind are moved to ``ignored``, and a disagreement between
    ``value`` and ``values`` is reported in ``conflict``.
    """
    project = _try_project_scoring(claim, source_dir)
    if project:
        return _finish(project, claim)
    return _finish(_fallback_extract(claim, source_dir), claim)


def extract_primary_observation(claim: dict, source_dir: Path) -> Optional[float]:
    """The primary observed value only; kept for existing callers."""
    return extract_observations(claim, source_dir).value
