"""Replay orchestration, extracted from the ``replay`` CLI callback.

This is the shared core that both the ``evident-agent replay`` CLI
command and the ``evident-agent-mcp`` ``replay`` tool call. It contains
no ``click`` and no ``sys.exit``: callers drive output through the
``on_event`` callback and handle the two domain exceptions
(:class:`NoClaimsMatched`, :class:`RenderFailed`).

Per-claim execution is classified into a docker *outcome* discriminator
(``completed`` / ``timed_out`` / ``infrastructure_error`` / ``skipped`` /
``dry_run``). A ``completed`` run with a nonzero exit code is a valid
experimental result; ``timed_out`` and ``infrastructure_error`` are
tool/infra failures the MCP layer surfaces as recoverable errors.
"""

from __future__ import annotations

import contextlib
import datetime
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterator, List, Optional

from . import docker, manifest, scoring, sidecar, typed_trust


@contextlib.contextmanager
def _sidecar_lock(sidecar_path: Path) -> Iterator[None]:
    """Advisory exclusive lock guarding the read-merge-write of a shared
    sidecar, so concurrent replays don't lose each other's entries.
    Held only around the (fast) merge, never during docker execution."""
    import fcntl

    sidecar_path.parent.mkdir(parents=True, exist_ok=True)
    lock_path = sidecar_path.with_name(sidecar_path.name + ".lock")
    with open(lock_path, "w") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


# ---------------------------------------------------------------------
# Output sink: nl-aware to preserve the CLI's exact stdout/stderr shape.
# CLI passes a click.echo shim; MCP passes a log collector.
# ---------------------------------------------------------------------
OnEvent = Callable[..., None]


def _noop(*_args, **_kwargs) -> None:  # default sink
    return None


# ---------------------------------------------------------------------
# Domain exceptions (replace the callback's sys.exit calls)
# ---------------------------------------------------------------------
class NoClaimsMatched(Exception):
    """No claim matched the selection filter/kind."""

    def __init__(self, claim_filter: Optional[str]):
        super().__init__(f"no measurement claims matched (filter={claim_filter!r})")
        self.claim_filter = claim_filter


class RenderFailed(Exception):
    """The typed-trust render subprocess exited non-zero."""

    def __init__(self, exit_code: int, stderr: str):
        super().__init__(f"typed-trust render failed (exit {exit_code})")
        self.exit_code = exit_code
        self.stderr = stderr


# ---------------------------------------------------------------------
# Results
# ---------------------------------------------------------------------
@dataclass
class ReplayClaimResult:
    claim_id: str
    exit_code: int
    duration_s: float
    observed: Optional[float]
    # completed | timed_out | infrastructure_error | skipped | dry_run
    outcome: str
    stderr_tail: Optional[str] = None

    @property
    def skipped_execution(self) -> bool:
        return self.outcome in ("skipped", "dry_run")


@dataclass
class ReplayResult:
    selected_count: int
    claims: List[ReplayClaimResult] = field(default_factory=list)
    sidecar_path: Optional[Path] = None  # None on dry-run (not written)
    new_count: int = 0
    total_count: int = 0
    rendered: Optional[str] = None
    dry_run: bool = False


def run_replay(
    *,
    manifest_path: Path,
    claim_filter: Optional[str] = None,
    image: str = "proteon-evident:latest",
    source_dir: Optional[Path] = None,
    budget: float = 600.0,
    sidecar_path: Optional[Path] = None,
    dry_run: bool = False,
    no_execute: bool = False,
    render: Optional[str] = None,
    typed_trust_binary: Optional[str] = None,
    on_event: Optional[OnEvent] = None,
) -> ReplayResult:
    """Replay selected measurement claims and populate the sidecar.

    Faithful extraction of the former ``replay`` callback body. Output
    is emitted through ``on_event(msg, *, err=False, nl=True)``; the
    empty-selection and render-failure paths raise domain exceptions
    instead of calling ``sys.exit``.
    """
    emit = on_event or _noop

    if sidecar_path is None:
        sidecar_path = manifest_path.parent / "last_verified.json"

    claims = list(manifest.load_claims(manifest_path))
    selected = list(manifest.filter_claims(claims, claim_filter=claim_filter))
    if not selected:
        raise NoClaimsMatched(claim_filter)

    new_entries: dict[str, sidecar.LastVerifiedEntry] = {}
    today = datetime.date.today().isoformat()
    claim_results: List[ReplayClaimResult] = []

    for i, claim in enumerate(selected, start=1):
        emit(f"[{i}/{len(selected)}] {claim.id}")
        # Per workflow/SCHEMA.md, claim.source resolves relative to the
        # TOP manifest directory, not the include file's directory.
        resolved_source = source_dir or claim.source_dir()

        outcome = "completed"
        stderr_tail: Optional[str] = None

        # Stage 1: execute
        if no_execute:
            emit("  (--no-execute) skipping docker invocation")
            exit_code = 0
            duration_s = 0.0
            outcome = "skipped"
        else:
            try:
                result = docker.run(
                    image=image,
                    claim_id=claim.id,
                    source_dir=resolved_source,
                    budget_seconds=budget,
                    dry_run=dry_run,
                )
            except (FileNotFoundError, OSError) as exc:
                # docker binary missing / not launchable — infrastructure,
                # not a claim result. Record and move on.
                emit(f"  docker unavailable: {exc}", err=True)
                claim_results.append(
                    ReplayClaimResult(
                        claim_id=claim.id,
                        exit_code=127,
                        duration_s=0.0,
                        observed=None,
                        outcome="infrastructure_error",
                        stderr_tail=str(exc),
                    )
                )
                continue
            emit(f"  cmd:      docker run … {image} replay {claim.id}")
            emit(f"  cwd:      {resolved_source}")
            emit(f"  duration: {result.duration_s:.1f}s")
            emit(f"  exit:     {result.exit_code}")
            if result.exit_code != 0 and result.stderr_tail:
                emit(f"  stderr:   {result.stderr_tail[-200:]}", err=True)
            exit_code = result.exit_code
            duration_s = result.duration_s
            stderr_tail = result.stderr_tail
            if result.timed_out:
                outcome = "timed_out"

        if dry_run:
            claim_results.append(
                ReplayClaimResult(
                    claim_id=claim.id,
                    exit_code=exit_code,
                    duration_s=duration_s,
                    observed=None,
                    outcome="dry_run",
                    stderr_tail=stderr_tail,
                )
            )
            continue

        # Stage 2: extract observed value
        observed = scoring.extract_primary_observation(claim.raw, resolved_source)
        if observed is not None:
            emit(f"  observed: {observed}")
        else:
            emit("  observed: (not extracted)")

        # Stage 3: stage sidecar entry
        entry = sidecar.LastVerifiedEntry(
            commit=_resolve_commit(resolved_source),
            date=today,
            value=observed if exit_code == 0 else None,
            corpus_sha=claim.raw.get("inputs", {}).get("corpus_sha"),
        )
        new_entries[claim.id] = entry
        claim_results.append(
            ReplayClaimResult(
                claim_id=claim.id,
                exit_code=exit_code,
                duration_s=duration_s,
                observed=observed,
                outcome=outcome,
                stderr_tail=stderr_tail,
            )
        )

    written_path: Optional[Path] = None
    if dry_run:
        total_count = len(sidecar.read(sidecar_path))
        emit(
            f"(--dry-run) sidecar NOT written; {len(selected)} claims would be processed"
        )
    else:
        # Re-read under the lock so concurrent replays of different claims
        # don't clobber each other (Codex High #4).
        with _sidecar_lock(sidecar_path):
            existing = sidecar.read(sidecar_path)
            merged = sidecar.merge(existing, new_entries)
            sidecar.write(sidecar_path, merged)
        written_path = sidecar_path
        total_count = len(merged)
        emit(
            f"sidecar written: {sidecar_path} ({len(new_entries)} new / {len(merged)} total)"
        )

    # Optional: render via typed-trust
    rendered: Optional[str] = None
    if render is not None:
        tt = typed_trust.run(
            manifest_path=manifest_path,
            sidecar_path=sidecar_path,
            format=render,
            claim_filter=claim_filter,
            binary=typed_trust_binary,
        )
        if tt.exit_code != 0:
            raise RenderFailed(tt.exit_code, tt.stderr)
        emit(tt.stdout, nl=False)
        rendered = tt.stdout

    return ReplayResult(
        selected_count=len(selected),
        claims=claim_results,
        sidecar_path=written_path,
        new_count=len(new_entries),
        total_count=total_count,
        rendered=rendered,
        dry_run=dry_run,
    )


def _resolve_commit(source_dir: Path) -> Optional[str]:
    """Return the source dir's git HEAD commit, or None."""
    try:
        out = subprocess.run(
            ["git", "-C", str(source_dir), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
            timeout=5,
        )
        if out.returncode == 0:
            return out.stdout.strip()
    except Exception:
        pass
    return None
