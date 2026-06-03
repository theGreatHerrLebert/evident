"""Sidecar ``last_concorded.json`` read/write.

The concordance counterpart to ``last_verified.json``. Per the v4
design (sidecar boundary section), the typed-trust status resolver
dispatches by ``claim.kind``: measurement claims consult
``last_verified``; concordance claims consult ``last_concorded``.

Format keyed by claim_id:

.. code-block:: json

    {
      "<claim-id>": {
        "comparison_status": "pass" | "fail" | "not_assessed",
        "observed_value": 1.6,
        "observed_unit": "percentage_points",
        "diagnostics": { ... },
        "image_digest": "sha256:...",
        "produced_at": "2026-06-04T10:00:00Z"
      }
    }

Pattern-specific fields (``observed_ordering`` /
``prior_ordering`` for ordinal_match; ``observed_series`` /
``parameter_series`` for monotone_with) are also preserved.

This module is intentionally schema-loose: it round-trips whatever
fields the comparator wrote, plus the load-bearing
``comparison_status`` discriminator the framework reads.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, Optional


@dataclass
class LastConcordedEntry:
    """One concordance result, ready to round-trip through JSON.

    ``comparison_status`` is the only field required by the
    framework's status resolver. The other fields are propagated
    through unchanged.
    """

    comparison_status: str  # "pass" | "fail" | "not_assessed"
    observed_value: Optional[float] = None
    observed_unit: Optional[str] = None
    observed_ordering: Optional[list[str]] = None
    prior_ordering: Optional[list[str]] = None
    observed_series: Optional[list[float]] = None
    parameter_series: Optional[list[float]] = None
    image_digest: Optional[str] = None
    produced_at: Optional[str] = None
    diagnostics: dict = field(default_factory=dict)

    def to_dict(self) -> dict:
        out: dict = {"comparison_status": self.comparison_status}
        for name in (
            "observed_value",
            "observed_unit",
            "observed_ordering",
            "prior_ordering",
            "observed_series",
            "parameter_series",
            "image_digest",
            "produced_at",
        ):
            v = getattr(self, name)
            if v is not None:
                out[name] = v
        if self.diagnostics:
            out["diagnostics"] = dict(self.diagnostics)
        return out

    @classmethod
    def from_dict(cls, raw: dict) -> "LastConcordedEntry":
        return cls(
            comparison_status=str(raw.get("comparison_status", "not_assessed")),
            observed_value=raw.get("observed_value"),
            observed_unit=raw.get("observed_unit"),
            observed_ordering=raw.get("observed_ordering"),
            prior_ordering=raw.get("prior_ordering"),
            observed_series=raw.get("observed_series"),
            parameter_series=raw.get("parameter_series"),
            image_digest=raw.get("image_digest"),
            produced_at=raw.get("produced_at"),
            diagnostics=dict(raw.get("diagnostics", {})),
        )


def read(path: Path) -> Dict[str, LastConcordedEntry]:
    """Load a sidecar file; return empty dict if the file doesn't exist."""
    if not path.is_file():
        return {}
    raw = json.loads(path.read_text())
    if not isinstance(raw, dict):
        return {}
    out: Dict[str, LastConcordedEntry] = {}
    for claim_id, entry in raw.items():
        if not isinstance(entry, dict):
            continue
        out[claim_id] = LastConcordedEntry.from_dict(entry)
    return out


def write(path: Path, entries: Dict[str, LastConcordedEntry]) -> None:
    """Write entries atomically (write to tempfile + rename)."""
    payload: Dict[str, Any] = {
        claim_id: entry.to_dict() for claim_id, entry in entries.items()
    }
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    tmp.replace(path)


def merge(
    existing: Dict[str, LastConcordedEntry],
    new_entries: Dict[str, LastConcordedEntry],
) -> Dict[str, LastConcordedEntry]:
    """Merge ``new_entries`` into ``existing``; new entries win
    for any claim_id present in both."""
    out = dict(existing)
    out.update(new_entries)
    return out
