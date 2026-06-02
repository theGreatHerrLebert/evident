"""Curator tooling: promote and drop subcommands.

After ``evident-agent extract`` produces a draft manifest at
``tier: research``, the curator reviews it and either:

- **Promotes** a claim to ``tier: ci`` or ``tier: release`` —
  authored as a ``PromoteFromExtracted`` event (Phase 5 PR3),
  with ``reviewed_extraction_sha`` recording the sha of the
  manifest the curator actually reviewed.
- **Drops** a claim from the manifest as pre-curation cleanup.
  No sidecar event is written; the audit trail comes from git.
  Drops happen on draft extractions before formal curation —
  removing extractor noise, not registering a "curator reviewed
  and rejected" decision. (Codex F-CURATOR-CR4 P2.)

Both operations are atomic against concurrent writes (flock on
a curator lockfile + atomic rename for the manifest).

## Architectural note: manifest mutation

The pre-Phase-5 evident-agent contract was "never mutate claim
YAMLs" (mirroring proteon's design philosophy). The curator
tool deliberately breaks this for **extracted manifests only** —
extracted manifests are draft documents the curator is meant
to refine, NOT immutable inputs from upstream. Hand-authored
manifests stay immutable.

The pre-edit sha recorded in ``reviewed_extraction_sha`` makes
each promotion reversible at the audit-trail level: a
verifier can recover the exact bytes the curator reviewed by
checking out the git ref where the manifest had that sha.
Codex F-CURATOR-CR-architecture flagged this as a deliberate
contract change and recommended the design note.

## Partial-commit discipline (codex F-CURATOR-CR1 P1)

The promotion writes the sidecar FIRST, then the manifest. The
reverse order would leave the manifest at tier:ci with no
matching event if the sidecar append failed — exactly the
gate-violating state PR3's validator is meant to catch.

The failure mode the chosen order produces — sidecar written,
manifest not — leaves an orphan event but does NOT violate the
gate: typed-trust sees the manifest still at tier:research, so
the orphan event is benign. A subsequent retry re-uses the same
event_id (idempotent on identical inputs), so re-running the
promotion converges cleanly.

## Event id semantics (codex F-CURATOR-CR3 P2)

``_compute_promotion_event_id`` hashes ``(target_claim,
from_tier, to_tier, reviewed_extraction_sha, timestamp,
curator_name)``. Including curator_name means two different
curators filing the SAME promotion at the SAME second get
distinct event_ids — independent audit records. The same
curator re-filing the same promotion at the same second is
the duplicate-by-design case and gets deduped by
append_events.
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

# Multi-step linear tier ladder. The curator can promote one rung
# at a time: research -> ci, then ci -> release. Skipping rungs
# (research -> release directly) is rejected because typed-trust's
# multi-step validator requires each leg to have its own event.
_TIER_LADDER = ("research", "ci", "release")


def adjacent_promotion_target(from_tier: str) -> Optional[str]:
    """Return the next tier up the linear ladder, or None if
    ``from_tier`` is at or above the top.

    Public helper used by both ``promote_claim`` and the review
    walkthrough — they share the same ladder contract. The
    leading-underscore alias below is preserved for backward
    compatibility (codex F-WALK-LADDER review note).
    """
    try:
        i = _TIER_LADDER.index(from_tier)
    except ValueError:
        return None
    if i + 1 >= len(_TIER_LADDER):
        return None
    return _TIER_LADDER[i + 1]


# Backward-compatibility alias for the older internal name.
_adjacent_promotion_target = adjacent_promotion_target


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
    curator_name: str,
) -> str:
    """Stable id for a promotion event. sha256 of the tuple that
    distinguishes one promotion from another.

    Codex F-CURATOR-CR3 (P2): includes ``curator_name`` so two
    different curators filing the same promotion at the same
    second get distinct event_ids — they're independent audit
    records. The same curator re-filing the same promotion at the
    same second is the duplicate-by-design case and gets deduped
    by ``append_events``.

    Typed-trust's canonical hash (PR3) does NOT include
    ``promote_from_extracted`` fields. The curator-side explicit
    event_id is what distinguishes one promotion from another;
    typed-trust honours the explicit value.
    """
    payload = {
        "target_claim": target_claim,
        "from_tier": from_tier,
        "to_tier": to_tier,
        "reviewed_extraction_sha": reviewed_extraction_sha,
        "timestamp": timestamp,
        "curator_name": curator_name,
    }
    blob = json.dumps(payload, sort_keys=True).encode("utf-8")
    return f"sha256:{hashlib.sha256(blob).hexdigest()}"


def _parse_curator(curator_arg: str) -> ReviewAuthor:
    """Parse a curator string into a ``ReviewAuthor``.

    Supported forms:
    - ``Jane Doe`` — name only
    - ``Jane Doe <orcid:0000-0001-2345-6789>`` — name + ORCID

    Codex F-CURATOR-CR2 (P2): unknown angle-bracket tokens are
    rejected with a clear error rather than silently dropped.
    Email tokens in particular are not part of the audit-identity
    schema today; adding them would require a typed-trust schema
    change. Until then, the curator must use one of the supported
    forms.
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
        else:
            raise CuratorError(
                f"curator identity {curator_arg!r}: unknown "
                f"angle-bracket token {token!r}. Supported: "
                "'<orcid:...>'. (email/affiliation tokens need a "
                "schema change to ReviewAuthor.)"
            )
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
    # Codex F-CURATOR-CR-note: take ownership of the fd immediately.
    # If fdopen fails, raw fd would otherwise leak.
    try:
        f = os.fdopen(fd, "w", encoding="utf-8")
    except Exception:
        os.close(fd)
        try:
            os.unlink(tmp_path_str)
        except FileNotFoundError:
            pass
        raise
    try:
        with f:
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
            # Multi-step linear ladder: each promotion advances one
            # rung. typed-trust's PR3+multi-step validator requires
            # an event for EACH leg, so the curator can promote
            # research -> ci then ci -> release as two separate
            # operations, but skipping rungs (research -> release
            # direct) is rejected.
            expected_to = _adjacent_promotion_target(from_tier)
            if expected_to is None:
                raise CuratorError(
                    f"claim {claim_id!r} is at tier {from_tier!r}, "
                    "which is not a valid promotion source; the "
                    "promotion ladder is research -> ci -> release"
                )
            if to_tier != expected_to:
                raise CuratorError(
                    f"claim {claim_id!r} is at tier {from_tier!r}; the "
                    f"adjacent target is {expected_to!r}, not {to_tier!r}. "
                    "Multi-step promotions must advance one rung at a "
                    "time (research -> ci, then ci -> release)."
                )
            target["tier"] = to_tier

            event_id = _compute_promotion_event_id(
                target_claim=claim_id,
                from_tier=from_tier,
                to_tier=to_tier,
                reviewed_extraction_sha=reviewed_sha,
                timestamp=ts,
                curator_name=author.name,
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

            # Codex F-CURATOR-CR1 (P1): append sidecar FIRST. If the
            # sidecar append fails (duplicate event_id, IO error,
            # etc.) and we'd already written the promoted manifest,
            # the manifest would sit at tier:ci with no corresponding
            # event — exactly the gate-violating state PR3's
            # validator guards against. Reverse order leaves an
            # orphan event but doesn't violate the gate (typed-trust
            # sees the manifest still at tier:research; the orphan
            # event is benign).
            append_events(sidecar_path, [entry])
            new_yaml = yaml.safe_dump(
                manifest, sort_keys=False, default_flow_style=False,
            )
            _atomic_write_text(manifest_path, new_yaml)
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
