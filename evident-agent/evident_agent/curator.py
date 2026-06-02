"""Curator tooling: promote and drop subcommands.

After ``evident-agent extract`` produces a draft manifest at
``tier: research``, the curator reviews it and either:

- **Promotes** a claim to ``tier: ci`` or ``tier: release`` —
  authored as a ``PromoteFromExtracted`` event (Phase 5 PR3),
  with ``reviewed_extraction_sha`` recording the sha of the
  manifest the curator actually reviewed.
- **Drops** a claim from the manifest entirely. No sidecar event
  needed; the audit trail comes from git.

Both operations are atomic against concurrent writes (flock on a
sidecar lockfile + atomic rename for the manifest).

The promotion event ships with an explicit ``event_id`` computed
from sha256 of ``(target_claim, from_tier, to_tier,
reviewed_extraction_sha, timestamp)``. Two distinct promotions
get distinct ids even if their other fields collide.
"""

from __future__ import annotations

import datetime
import fcntl
import hashlib
import json
import os
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

import yaml

from .review_sidecar import ReviewAuthor, ReviewEventEntry, append_events


VALID_TARGET_TIERS = ("ci", "release")


@dataclass
class PromotionResult:
    """Outcome of a successful ``promote_claim`` call."""

    claim_id: str
    from_tier: str
    to_tier: str
    reviewed_extraction_sha: str
    event_id: str
    sidecar_path: Path
    manifest_path: Path


@dataclass
class DropResult:
    claim_id: str
    manifest_path: Path
    remaining_claim_ids: list[str]


class CuratorError(Exception):
    """Curator operation failed with a structured reason. The CLI
    maps these to non-zero exits with the message visible."""


# ---------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------


def _now_utc_iso() -> str:
    return (
        datetime.datetime.now(tz=datetime.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _compute_promotion_event_id(
    *,
    target_claim: str,
    from_tier: str,
    to_tier: str,
    reviewed_extraction_sha: str,
    timestamp: str,
) -> str:
    """Stable id for a promotion event. sha256 of the tuple that
    distinguishes one promotion from another.

    Typed-trust's canonical hash (PR3) does NOT include
    ``promote_from_extracted`` fields. The curator-side explicit
    event_id ensures two semantically distinct promotions never
    collide on id; typed-trust honours the explicit value.
    """
    payload = {
        "target_claim": target_claim,
        "from_tier": from_tier,
        "to_tier": to_tier,
        "reviewed_extraction_sha": reviewed_extraction_sha,
        "timestamp": timestamp,
    }
    blob = json.dumps(payload, sort_keys=True).encode("utf-8")
    return f"sha256:{hashlib.sha256(blob).hexdigest()}"


def _parse_curator(curator_arg: str) -> ReviewAuthor:
    """Parse a curator string like ``"Jane Doe"``,
    ``"Jane Doe <orcid:0000-0001-...>"``, or
    ``"Jane Doe <jane@example.com>"`` into a ReviewAuthor.
    """
    s = curator_arg.strip()
    if not s:
        raise CuratorError("curator identity is required")
    name = s
    orcid = None
    if "<" in s and s.endswith(">"):
        name, rest = s.rsplit("<", 1)
        name = name.strip()
        token = rest[:-1].strip()
        if token.startswith("orcid:"):
            orcid = token.split(":", 1)[1].strip()
    if not name:
        raise CuratorError(
            f"curator identity {curator_arg!r} has no name"
        )
    return ReviewAuthor(kind="human", name=name, orcid=orcid)


def _atomic_write_text(path: Path, content: str) -> None:
    """Write `content` to `path` via tempfile + atomic rename. Used
    for manifest in-place edits so a concurrent reader never sees a
    half-written file."""
    fd, tmp_path_str = tempfile.mkstemp(
        prefix=f".{path.name}.",
        suffix=".tmp",
        dir=str(path.parent),
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(content)
        os.replace(tmp_path_str, str(path))
    except Exception:
        try:
            os.unlink(tmp_path_str)
        except FileNotFoundError:
            pass
        raise


def _lock_path_for_manifest(manifest_path: Path) -> Path:
    """Use the sidecar's lock convention. Sits next to the manifest
    so promote + sidecar append serialize against the same advisory
    lock without colliding with the manifest file itself."""
    return manifest_path.parent / ".curator.lock"


# ---------------------------------------------------------------------
# Public operations
# ---------------------------------------------------------------------


def promote_claim(
    *,
    manifest_path: Path,
    claim_id: str,
    to_tier: str,
    rationale: str,
    curator: str,
    sidecar_path: Optional[Path] = None,
    timestamp: Optional[str] = None,
) -> PromotionResult:
    """Promote one claim from ``tier: research`` to ``tier:
    ci|release``. Edits the manifest in place (atomic) AND appends a
    ``PromoteFromExtracted`` event to the sidecar (defaults to
    ``manifest.parent/review_events.json``).

    The recorded ``reviewed_extraction_sha`` is the sha256 of the
    manifest's pre-edit bytes — what the curator actually reviewed.
    """
    if to_tier not in VALID_TARGET_TIERS:
        raise CuratorError(
            f"to_tier {to_tier!r} not in {VALID_TARGET_TIERS}; "
            "PR3 contract supports promotion to ci or release only"
        )
    if not rationale.strip():
        raise CuratorError("rationale is required")
    if sidecar_path is None:
        sidecar_path = manifest_path.parent / "review_events.json"
    author = _parse_curator(curator)

    lock_path = _lock_path_for_manifest(manifest_path)
    lock_path.touch(exist_ok=True)
    ts = timestamp or _now_utc_iso()

    with open(lock_path, "r+") as lock_f:
        fcntl.flock(lock_f.fileno(), fcntl.LOCK_EX)
        try:
            raw = manifest_path.read_bytes()
            reviewed_sha = _sha256_bytes(raw)
            manifest = yaml.safe_load(raw.decode("utf-8")) or {}
            claims = manifest.get("claims") or []
            target = None
            for c in claims:
                if c.get("id") == claim_id:
                    target = c
                    break
            if target is None:
                raise CuratorError(
                    f"claim_id {claim_id!r} not in manifest "
                    f"{manifest_path}"
                )
            from_tier = target.get("tier", "")
            # PR3 contract: from_tier MUST be 'research' for the
            # first promotion. Multi-step promotions are deferred.
            if from_tier != "research":
                raise CuratorError(
                    f"claim {claim_id!r} is at tier {from_tier!r}, "
                    "not 'research'; the PR3 validator only accepts "
                    "research → ci|release transitions"
                )
            target["tier"] = to_tier

            event_id = _compute_promotion_event_id(
                target_claim=claim_id,
                from_tier=from_tier,
                to_tier=to_tier,
                reviewed_extraction_sha=reviewed_sha,
                timestamp=ts,
            )

            entry = ReviewEventEntry(
                claim_id=claim_id,
                kind="promote_from_extracted",
                author=author,
                rationale=rationale.strip(),
                timestamp=ts,
                event_id=event_id,
                promote_from_extracted={
                    "target_claim": claim_id,
                    "from_tier": from_tier,
                    "to_tier": to_tier,
                    "reviewed_extraction_sha": reviewed_sha,
                },
            )

            new_yaml = yaml.safe_dump(
                manifest, sort_keys=False, default_flow_style=False,
            )
            _atomic_write_text(manifest_path, new_yaml)
            append_events(sidecar_path, [entry])
        finally:
            fcntl.flock(lock_f.fileno(), fcntl.LOCK_UN)

    return PromotionResult(
        claim_id=claim_id,
        from_tier=from_tier,
        to_tier=to_tier,
        reviewed_extraction_sha=reviewed_sha,
        event_id=event_id,
        sidecar_path=sidecar_path,
        manifest_path=manifest_path,
    )


def drop_claim(
    *,
    manifest_path: Path,
    claim_id: str,
) -> DropResult:
    """Remove a claim from the manifest entirely. No sidecar event
    is written — the audit trail comes from git."""
    lock_path = _lock_path_for_manifest(manifest_path)
    lock_path.touch(exist_ok=True)
    with open(lock_path, "r+") as lock_f:
        fcntl.flock(lock_f.fileno(), fcntl.LOCK_EX)
        try:
            raw = manifest_path.read_bytes()
            manifest = yaml.safe_load(raw.decode("utf-8")) or {}
            claims = manifest.get("claims") or []
            new_claims = [c for c in claims if c.get("id") != claim_id]
            if len(new_claims) == len(claims):
                raise CuratorError(
                    f"claim_id {claim_id!r} not in manifest "
                    f"{manifest_path}"
                )
            manifest["claims"] = new_claims
            new_yaml = yaml.safe_dump(
                manifest, sort_keys=False, default_flow_style=False,
            )
            _atomic_write_text(manifest_path, new_yaml)
        finally:
            fcntl.flock(lock_f.fileno(), fcntl.LOCK_UN)
    return DropResult(
        claim_id=claim_id,
        manifest_path=manifest_path,
        remaining_claim_ids=[c.get("id") for c in new_claims],
    )
