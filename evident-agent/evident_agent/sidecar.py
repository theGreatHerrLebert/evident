"""Sidecar ``last_verified.json`` read/write.

Format matches ``workflow/evident.py``'s convention:

.. code-block:: json

    {
      "<claim-id>": {
        "commit": "...",
        "date": "YYYY-MM-DD",
        "value": 0.0017,
        "corpus_sha": "..."
      }
    }

All four fields are optional / nullable. Missing or null fields are
preserved when re-reading. Writes merge with any existing sidecar so
a partial agent run (one claim at a time) accumulates without
clobbering prior entries.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Dict, Optional


@dataclass
class LastVerifiedEntry:
    """Mirrors typed-trust's ``ManifestLastVerified``.

    All fields are optional. ``value`` carries the primary observed
    metric (typed-trust binds this to the first criterion).
    """

    commit: Optional[str] = None
    date: Optional[str] = None
    value: Optional[float] = None
    corpus_sha: Optional[str] = None

    def to_dict(self) -> dict:
        return {k: v for k, v in asdict(self).items() if v is not None}


def read(path: Path) -> Dict[str, LastVerifiedEntry]:
    """Load a sidecar file; return empty dict if the file doesn't exist."""
    if not path.is_file():
        return {}
    raw = json.loads(path.read_text())
    out: Dict[str, LastVerifiedEntry] = {}
    for claim_id, entry in raw.items():
        if not isinstance(entry, dict):
            continue
        out[claim_id] = LastVerifiedEntry(
            commit=entry.get("commit"),
            date=entry.get("date"),
            value=entry.get("value"),
            corpus_sha=entry.get("corpus_sha"),
        )
    return out


def write(path: Path, entries: Dict[str, LastVerifiedEntry]) -> None:
    """Write entries atomically. Uses a UNIQUE temp file in the target
    directory + ``os.replace`` so two concurrent writers can never share
    (and corrupt) the same temp inode."""
    import os
    import tempfile

    payload = {claim_id: entry.to_dict() for claim_id, entry in entries.items()}
    body = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    fd, tmpname = tempfile.mkstemp(
        dir=str(path.parent), prefix=path.name + ".", suffix=".tmp"
    )
    try:
        with os.fdopen(fd, "w") as f:
            f.write(body)
        os.replace(tmpname, path)
    except BaseException:
        try:
            os.unlink(tmpname)
        except OSError:
            pass
        raise


def merge(
    existing: Dict[str, LastVerifiedEntry],
    new_entries: Dict[str, LastVerifiedEntry],
) -> Dict[str, LastVerifiedEntry]:
    """Merge ``new_entries`` into ``existing``; new entries win for any
    claim_id present in both.
    """
    out = dict(existing)
    out.update(new_entries)
    return out
