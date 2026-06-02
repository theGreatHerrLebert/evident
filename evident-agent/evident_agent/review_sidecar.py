"""Append-only ``review_events.json`` sidecar with concurrency safety.

Mirrors Phase 1's ``sidecar.py`` (last_verified) but with two extra
properties:

1. **Append-only.** Existing entries are never mutated; new runs add
   new events. A reviewer's recorded speech act is not later state to
   overwrite.

2. **Concurrent-write safe.** Two agent processes can both call
   ``append_events`` for the same sidecar path simultaneously without
   losing entries. We take an advisory ``fcntl.flock`` around the
   read-modify-write so the OS serializes the critical section.
   Atomic-rename alone is insufficient — two readers can both observe
   version N and both rename, losing the earlier rename's content.

The canonical event_id is derived deterministically from the event
payload — see ``canonical_event_id``. Identical payloads produce
identical ids; any change in payload yields a different id. This
avoids the ``(claim_id, author, kind, timestamp)`` tuple collision
risk under sub-second concurrent runs.
"""

from __future__ import annotations

import fcntl
import hashlib
import json
import os
import tempfile
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Optional


@dataclass
class ReviewAuthor:
    """Author block for a ReviewEvent sidecar entry.

    Mirrors typed-trust's ``ManifestReviewAuthor`` deserialization
    shape exactly. ``version`` is required for ``kind == "model"`` —
    typed-trust rejects model authors without it.
    """

    kind: str  # "human" | "model" | "automated" | "organization" | "anonymous"
    name: str
    version: Optional[str] = None
    context: Optional[str] = None
    orcid: Optional[str] = None
    affiliation: Optional[str] = None


@dataclass
class ReviewEventEntry:
    """One sidecar entry.

    Field ordering matches typed-trust's ``ManifestReviewEvent``. Any
    ``None`` optional is omitted at serialization time so the on-disk
    file is compact.
    """

    claim_id: str
    kind: str  # "endorse" | "dissent" | "challenge" | "promote_from_extracted"
    author: ReviewAuthor
    rationale: str
    timestamp: str
    event_id: Optional[str] = None
    checks: Optional[dict[str, str]] = None
    observed_value: Optional[str] = None
    tolerance: Optional[str] = None
    failure_reason: Optional[str] = None
    # Phase 2b: structured challenge block for kind=challenge events.
    # Always present (and required) when kind=challenge; absent for
    # Endorse/Dissent. Shape mirrors typed-trust's ManifestChallengeBlock.
    challenge: Optional[dict[str, Any]] = None
    # Phase 5 PR3 / curator tooling: target_claim + from_tier + to_tier
    # + reviewed_extraction_sha. Required when kind = "promote_from_extracted".
    # Mirrors typed-trust's ManifestPromoteFromExtractedBlock.
    promote_from_extracted: Optional[dict[str, Any]] = None
    protocol: Optional[str] = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize to JSON-ready dict, omitting None optionals."""
        raw = asdict(self)
        author = {k: v for k, v in raw["author"].items() if v is not None}
        out: dict[str, Any] = {
            "claim_id": raw["claim_id"],
            "kind": raw["kind"],
            "author": author,
            "rationale": raw["rationale"],
            "timestamp": raw["timestamp"],
        }
        for k in (
            "event_id",
            "checks",
            "observed_value",
            "tolerance",
            "failure_reason",
            "challenge",
            "promote_from_extracted",
            "protocol",
        ):
            if raw[k] is not None:
                out[k] = raw[k]
        return out


def canonical_event_id(entry: ReviewEventEntry) -> str:
    """Compute a stable event_id from the entry payload.

    Canonical JSON with fixed key ordering → sha256 → ``sha256:<hex>``.
    Must match typed-trust's ``canonical_event_id`` byte-for-byte so the
    same entry produces the same id whether the agent or typed-trust
    computes it.
    """
    author: dict[str, Any] = {
        "kind": entry.author.kind,
        "name": entry.author.name,
    }
    for key, val in (
        ("version", entry.author.version),
        ("context", entry.author.context),
        ("orcid", entry.author.orcid),
        ("affiliation", entry.author.affiliation),
    ):
        if val is not None:
            author[key] = val

    payload: dict[str, Any] = {
        "claim_id": entry.claim_id,
        "kind": entry.kind,
        "author": author,
        "rationale": entry.rationale,
        "timestamp": entry.timestamp,
    }
    for key, val in (
        ("checks", entry.checks),
        ("observed_value", entry.observed_value),
        ("tolerance", entry.tolerance),
        ("failure_reason", entry.failure_reason),
        ("challenge", _canonical_challenge(entry.challenge)),
        ("protocol", entry.protocol),
    ):
        if val is not None:
            payload[key] = val

    # Match Rust's serde_json::Map (preserves insertion order) by
    # using sort_keys=False — both sides build payload with the same
    # key order, so canonical JSON output is identical.
    encoded = json.dumps(payload, sort_keys=False, separators=(",", ":")).encode("utf-8")
    return f"sha256:{hashlib.sha256(encoded).hexdigest()}"


def read_events(path: Path) -> list[ReviewEventEntry]:
    """Read all events from the sidecar, or return [] if it doesn't
    exist or is empty. Does **not** take a lock — callers that need
    consistency under concurrency should use ``append_events`` (which
    holds the lock across read + write)."""
    if not path.is_file():
        return []
    text = path.read_text()
    if not text.strip():
        return []
    data = json.loads(text)
    events = data.get("events") or []
    return [_entry_from_dict(e) for e in events]


def append_events(path: Path, new_entries: list[ReviewEventEntry]) -> list[ReviewEventEntry]:
    """Append entries to the sidecar under a file lock.

    Workflow:
    1. Open (or create) the lockfile + sidecar.
    2. Take an exclusive ``fcntl.flock``.
    3. Read existing events.
    4. Append the new entries (computing canonical event_ids when not set).
    5. Atomically rename a tempfile into place.
    6. Release the lock.

    Returns the full merged list as written.
    """
    # The lock is taken on a side-by-side .lock file so that the
    # tempfile-rename pattern doesn't race with the lock.
    lock_path = path.with_suffix(path.suffix + ".lock")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with open(lock_path, "a+") as lock_f:
        fcntl.flock(lock_f.fileno(), fcntl.LOCK_EX)
        try:
            existing = read_events(path)

            # Fill in canonical event_ids for any entry that didn't
            # have one. The agent normally pre-fills these so the
            # logged id matches typed-trust's view, but we don't
            # require it.
            merged = list(existing)
            for entry in new_entries:
                if entry.event_id is None:
                    entry.event_id = canonical_event_id(entry)
                merged.append(entry)

            payload = {"events": [e.to_dict() for e in merged]}
            _atomic_write_json(path, payload)
            return merged
        finally:
            fcntl.flock(lock_f.fileno(), fcntl.LOCK_UN)


def _atomic_write_json(path: Path, payload: dict[str, Any]) -> None:
    """Write JSON via tempfile + rename. The rename is atomic on
    POSIX so a concurrent reader either sees the prior content or
    the new content, never partial."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w",
        dir=str(path.parent),
        prefix=f".{path.name}.",
        suffix=".tmp",
        delete=False,
    ) as tf:
        json.dump(payload, tf, indent=2, sort_keys=False)
        tf.write("\n")
        tf.flush()
        os.fsync(tf.fileno())
        tmp_name = tf.name
    os.replace(tmp_name, path)


def _entry_from_dict(d: dict[str, Any]) -> ReviewEventEntry:
    author_d = d.get("author") or {}
    author = ReviewAuthor(
        kind=author_d.get("kind", ""),
        name=author_d.get("name", ""),
        version=author_d.get("version"),
        context=author_d.get("context"),
        orcid=author_d.get("orcid"),
        affiliation=author_d.get("affiliation"),
    )
    return ReviewEventEntry(
        claim_id=d.get("claim_id", ""),
        kind=d.get("kind", ""),
        author=author,
        rationale=d.get("rationale", ""),
        timestamp=d.get("timestamp", ""),
        event_id=d.get("event_id"),
        checks=d.get("checks"),
        observed_value=d.get("observed_value"),
        tolerance=d.get("tolerance"),
        failure_reason=d.get("failure_reason"),
        challenge=d.get("challenge"),
        promote_from_extracted=d.get("promote_from_extracted"),
        protocol=d.get("protocol"),
    )


def _canonical_challenge(challenge: Optional[dict[str, Any]]) -> Optional[dict[str, Any]]:
    """Canonical projection of the challenge block for hashing.

    Mirrors typed-trust's ``challenge_canonical_value`` byte-for-byte:
    keep category + target_criterion_id + violation; deliberately
    exclude the backing claim (whose id is derived from the violation
    tuple, so it adds no discriminating info).
    """
    if challenge is None:
        return None
    out: dict[str, Any] = {"category": challenge["category"]}
    if challenge.get("target_criterion_id") is not None:
        out["target_criterion_id"] = challenge["target_criterion_id"]
    if challenge.get("violation") is not None:
        v = challenge["violation"]
        out["violation"] = {
            "metric": v["metric"],
            "observed_value": float(v["observed_value"]),
            "bound": float(v["bound"]),
            "comparator": v["comparator"],
            "citation": v["citation"],
        }
    return out
