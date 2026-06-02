"""``evident-agent`` CLI entry point.

Single subcommand for Phase 1: ``replay``. Workflow per claim:

1. Read manifest, filter by ``--claim`` if given (default: all
   measurement claims).
2. Resolve source dir (manifest.parent / claim.source).
3. Invoke docker replay (or skip with ``--no-execute`` for sidecar
   regeneration from existing artifacts).
4. Run scoring locally on the produced artifact to get the primary
   observed value.
5. Write the sidecar entry (claim_id → ``{commit, date, value,
   corpus_sha}``).
6. After all claims, optionally invoke typed-trust with the populated
   sidecar to emit a rendered report.
"""

from __future__ import annotations

import datetime
import sys
from pathlib import Path
from typing import Optional

import click

from . import docker, manifest, scoring, sidecar, typed_trust


@click.group()
def main() -> None:
    """EVIDENT agent — populate typed-trust inputs by running cited procedures."""


@main.command()
@click.option(
    "--manifest",
    "manifest_path",
    required=True,
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    help="Path to evident.yaml (or an included claim file).",
)
@click.option(
    "--claim",
    "claim_filter",
    default=None,
    help="Run only this claim id. Default: all measurement claims.",
)
@click.option(
    "--image",
    default="proteon-evident:latest",
    help="Docker image to invoke for per-claim replay.",
)
@click.option(
    "--source-dir",
    default=None,
    type=click.Path(file_okay=False, path_type=Path),
    help="Override the per-claim source directory. Default: manifest.parent / claim.source.",
)
@click.option(
    "--budget",
    type=float,
    default=600.0,
    help="Per-claim execution timeout in seconds (default 600).",
)
@click.option(
    "--sidecar",
    "sidecar_path",
    default=None,
    type=click.Path(path_type=Path),
    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
)
@click.option(
    "--dry-run",
    is_flag=True,
    help="Print the docker command per claim without executing.",
)
@click.option(
    "--no-execute",
    is_flag=True,
    help="Skip docker; only score existing artifacts and update the sidecar.",
)
@click.option(
    "--render",
    default=None,
    type=click.Choice(["json", "md", "html", "mermaid"]),
    help="After populating the sidecar, also invoke typed-trust and print the report.",
)
@click.option(
    "--typed-trust-binary",
    default=None,
    type=str,
    help="Path to the typed-trust binary. Default: search PATH and repo-relative builds.",
)
def replay(
    manifest_path: Path,
    claim_filter: Optional[str],
    image: str,
    source_dir: Optional[Path],
    budget: float,
    sidecar_path: Optional[Path],
    dry_run: bool,
    no_execute: bool,
    render: Optional[str],
    typed_trust_binary: Optional[str],
) -> None:
    """Replay one or more measurement claims and populate the sidecar."""
    if sidecar_path is None:
        sidecar_path = manifest_path.parent / "last_verified.json"

    claims = list(manifest.load_claims(manifest_path))
    selected = list(manifest.filter_claims(claims, claim_filter=claim_filter))
    if not selected:
        click.echo(f"no measurement claims matched (filter={claim_filter!r})", err=True)
        sys.exit(2)

    existing = sidecar.read(sidecar_path)
    new_entries: dict[str, sidecar.LastVerifiedEntry] = {}

    today = datetime.date.today().isoformat()

    for i, claim in enumerate(selected, start=1):
        click.echo(f"[{i}/{len(selected)}] {claim.id}")
        resolved_source = source_dir or (
            claim.source_path.parent / (claim.raw.get("source") or ".")
        ).resolve()

        # Stage 1: execute
        if no_execute:
            click.echo(f"  (--no-execute) skipping docker invocation")
            exit_code = 0
            duration_s = 0.0
        else:
            result = docker.run(
                image=image,
                claim_id=claim.id,
                source_dir=resolved_source,
                budget_seconds=budget,
                dry_run=dry_run,
            )
            click.echo(f"  cmd:      docker run … {image} replay {claim.id}")
            click.echo(f"  cwd:      {resolved_source}")
            click.echo(f"  duration: {result.duration_s:.1f}s")
            click.echo(f"  exit:     {result.exit_code}")
            if result.exit_code != 0 and result.stderr_tail:
                click.echo(f"  stderr:   {result.stderr_tail[-200:]}", err=True)
            exit_code = result.exit_code
            duration_s = result.duration_s

        if dry_run:
            continue

        # Stage 2: extract observed value
        observed = scoring.extract_primary_observation(claim.raw, resolved_source)
        if observed is not None:
            click.echo(f"  observed: {observed}")
        else:
            click.echo("  observed: (not extracted)")

        # Stage 3: write sidecar entry
        entry = sidecar.LastVerifiedEntry(
            commit=_resolve_commit(resolved_source),
            date=today,
            value=observed if exit_code == 0 else None,
            corpus_sha=claim.raw.get("inputs", {}).get("corpus_sha"),
        )
        new_entries[claim.id] = entry

    merged = sidecar.merge(existing, new_entries)
    sidecar.write(sidecar_path, merged)
    click.echo(f"sidecar written: {sidecar_path} ({len(new_entries)} new / {len(merged)} total)")

    # Optional: render via typed-trust
    if render is not None:
        result = typed_trust.run(
            manifest_path=manifest_path,
            sidecar_path=sidecar_path,
            format=render,
            claim_filter=claim_filter,
            binary=typed_trust_binary,
        )
        if result.exit_code != 0:
            click.echo(result.stderr, err=True)
            sys.exit(result.exit_code)
        click.echo(result.stdout, nl=False)


def _resolve_commit(source_dir: Path) -> Optional[str]:
    """Return the source dir's git HEAD commit, or None."""
    import subprocess

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


if __name__ == "__main__":
    main()
