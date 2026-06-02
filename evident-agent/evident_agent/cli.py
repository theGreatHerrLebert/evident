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

from . import docker, evidence, manifest, prompt as prompt_mod, review as review_mod, review_sidecar, scoring, sidecar, typed_trust


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
        # Per workflow/SCHEMA.md, claim.source resolves relative to the
        # TOP manifest directory, not the include file's directory.
        # ClaimRecord.source_dir() encapsulates that resolution.
        resolved_source = source_dir or claim.source_dir()

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

    if dry_run:
        click.echo(f"(--dry-run) sidecar NOT written; {len(selected)} claims would be processed")
    else:
        merged = sidecar.merge(existing, new_entries)
        sidecar.write(sidecar_path, merged)
        click.echo(
            f"sidecar written: {sidecar_path} ({len(new_entries)} new / {len(merged)} total)"
        )

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
    help="Review only this claim id. Default: all measurement claims.",
)
@click.option(
    "--model",
    required=True,
    type=str,
    help="Anthropic model id (e.g. claude-opus-4-7).",
)
@click.option(
    "--model-version",
    default=None,
    type=str,
    help="Author version for the sidecar entry. Default: same as --model.",
)
@click.option(
    "--review-sidecar",
    "review_sidecar_path",
    default=None,
    type=click.Path(path_type=Path),
    help="Sidecar path. Default: manifest.parent / 'review_events.json'.",
)
@click.option(
    "--last-verified-sidecar",
    "last_verified_sidecar_path",
    default=None,
    type=click.Path(path_type=Path),
    help=(
        "Path to a last_verified.json sidecar to overlay onto each claim "
        "before producing the digest. Default: manifest.parent / 'last_verified.json' if it exists."
    ),
)
@click.option(
    "--no-api",
    is_flag=True,
    help="Skip the API call; only build the prompt (for testing / record).",
)
@click.option(
    "--render",
    default=None,
    type=click.Choice(["json", "md", "html", "mermaid"]),
    help="After writing the review sidecar, also invoke typed-trust.",
)
@click.option(
    "--typed-trust-binary",
    default=None,
    type=str,
    help="Path to the typed-trust binary. Default: search PATH and repo-relative builds.",
)
def review(
    manifest_path: Path,
    claim_filter: Optional[str],
    model: str,
    model_version: Optional[str],
    review_sidecar_path: Optional[Path],
    last_verified_sidecar_path: Optional[Path],
    no_api: bool,
    render: Optional[str],
    typed_trust_binary: Optional[str],
) -> None:
    """Author Endorse/Dissent ReviewEvents on a claim's evidence (Phase 2a).

    Per claim:
      1. Build a per-format evidence digest from the cited artifact.
      2. Construct a default-Dissent prompt with the submit_review tool.
      3. Call the Anthropic API (one retry on transport failure).
      4. Validate the response — reject Endorse-with-failing-check,
         hallucinated criterion names, short rationales.
      5. Append the validated entry to the review_events.json sidecar.
      6. Optionally invoke typed-trust to emit a rendered report.
    """
    if review_sidecar_path is None:
        review_sidecar_path = manifest_path.parent / "review_events.json"
    if last_verified_sidecar_path is None:
        candidate = manifest_path.parent / "last_verified.json"
        if candidate.is_file():
            last_verified_sidecar_path = candidate

    if model_version is None:
        model_version = model

    claims = list(manifest.load_claims(manifest_path))
    selected = list(manifest.filter_claims(claims, claim_filter=claim_filter))
    if not selected:
        click.echo(f"no measurement claims matched (filter={claim_filter!r})", err=True)
        sys.exit(2)

    # Load the last_verified sidecar (if any) so per-claim verification
    # metadata — particularly the commit — can flow into the digest the
    # model sees. Without this, the "reproducible_chain" check has no
    # commit to verify against and would always default to fail/unknown.
    last_verified_by_claim: dict[str, sidecar.LastVerifiedEntry] = {}
    if last_verified_sidecar_path is not None and last_verified_sidecar_path.is_file():
        last_verified_by_claim = sidecar.read(last_verified_sidecar_path)

    new_entries: list[review_sidecar.ReviewEventEntry] = []
    for i, claim in enumerate(selected, start=1):
        click.echo(f"[{i}/{len(selected)}] {claim.id}")

        # Resolve source dir + artifact path (per workflow/SCHEMA.md,
        # the claim's `source` field is relative to the TOP manifest's
        # directory — encapsulated by ClaimRecord.source_dir()).
        source_dir = claim.source_dir()
        artifact_rel = (claim.raw.get("evidence") or {}).get("artifact")
        if not artifact_rel:
            click.echo("  skip: claim has no evidence.artifact", err=True)
            continue
        # `artifact` may be a free-form human string; pick the first
        # path-like token. Same posture as Phase 1.
        artifact_token = artifact_rel.split()[0]
        artifact_path = source_dir / artifact_token

        # Per-claim commit comes from (in order): the last_verified
        # sidecar entry > the manifest's inline last_verified.commit
        # > None. The digest header surfaces it so the model can
        # verify the reproducible_chain check.
        commit = _resolve_commit_for_claim(claim.raw, last_verified_by_claim.get(claim.id))

        # Extract digest.
        metric = _first_tolerance_metric(claim.raw)
        digest_obj = evidence.make_digest(
            artifact_path,
            metric,
            source_dir=source_dir,
            commit=commit,
        )
        click.echo(f"  digest: format={digest_obj.header.get('format')} metric_present={digest_obj.header.get('metric_present')} truncated={digest_obj.truncated}")

        if no_api:
            click.echo("  (--no-api) skipping API call; no sidecar entry written")
            continue

        # Build messages + call API.
        claim_yaml_text = _claim_yaml_block(claim.raw)
        try:
            verdict = review_mod.call_review(
                model=model,
                claim_yaml=claim_yaml_text,
                digest_rendered=digest_obj.render(),
            )
        except review_mod.ReviewTransportError as exc:
            click.echo(f"  skip: transport error after retry: {exc}", err=True)
            continue
        except review_mod.ReviewRejected as exc:
            click.echo(f"  skip: response rejected by validation: {exc}", err=True)
            continue

        # Hallucination check requires the claim's criteria ids.
        criteria_ids = _claim_criterion_ids(claim.raw)
        try:
            review_mod.reject_if_hallucinated_criterion(verdict, criteria_ids)
        except review_mod.ReviewRejected as exc:
            click.echo(f"  skip: hallucinated criterion: {exc}", err=True)
            continue

        # Truncated-evidence-without-citation check (F9). The model
        # cannot Endorse when the digest was truncated and its cited
        # observed_value isn't in the digest text — it's working blind.
        try:
            review_mod.reject_if_truncated_endorse_lacks_evidence(
                verdict, digest_obj.body, digest_obj.truncated
            )
        except review_mod.ReviewRejected as exc:
            click.echo(f"  skip: truncated digest, no citation: {exc}", err=True)
            continue

        click.echo(
            f"  verdict: {verdict.verdict} "
            f"(rationale: {verdict.rationale[:80]}…)"
        )
        entry = review_mod.verdict_to_sidecar_entry(
            verdict,
            claim_id=claim.id,
            author_name=model,
            author_version=model_version,
            author_context="evident-agent review v0.2a",
        )
        new_entries.append(entry)

    if not new_entries:
        click.echo("no review events written", err=True)
    else:
        review_sidecar.append_events(review_sidecar_path, new_entries)
        click.echo(
            f"review sidecar updated: {review_sidecar_path} ({len(new_entries)} new)"
        )

    if render is not None:
        result = typed_trust.run(
            manifest_path=manifest_path,
            sidecar_path=last_verified_sidecar_path,
            format=render,
            claim_filter=claim_filter,
            binary=typed_trust_binary,
            extra_args=["--review-events-sidecar", str(review_sidecar_path)],
        )
        if result.exit_code != 0:
            click.echo(result.stderr, err=True)
            sys.exit(result.exit_code)
        click.echo(result.stdout, nl=False)


def _resolve_commit_for_claim(
    claim_raw: dict,
    sidecar_entry: Optional[sidecar.LastVerifiedEntry],
) -> Optional[str]:
    """Pick the commit hash to surface in the digest header.

    Precedence: sidecar entry > inline manifest last_verified.commit
    > None. The digest header passes this to the model so it can
    check whether the evidence chain is reproducible from a specific
    commit (per the framing's reproducible_chain check).
    """
    if sidecar_entry is not None and sidecar_entry.commit:
        return sidecar_entry.commit
    inline = (claim_raw.get("last_verified") or {}).get("commit")
    return inline if isinstance(inline, str) and inline else None


def _first_tolerance_metric(claim_raw: dict) -> Optional[str]:
    """Pull the first tolerance metric (dotted path or simple name)
    from a claim's manifest dict. Returns None for prose-only or
    missing tolerances."""
    tols = claim_raw.get("tolerances") or []
    for t in tols:
        if isinstance(t, dict) and t.get("metric"):
            return str(t["metric"])
    return None


def _claim_criterion_ids(claim_raw: dict) -> list[str]:
    """Best-effort list of criterion identifier tokens for the
    hallucination check. Uses tolerance.metric values as the criterion
    surface (matches what typed-trust uses for CriterionId)."""
    out: list[str] = []
    tols = claim_raw.get("tolerances") or []
    for t in tols:
        if isinstance(t, dict) and t.get("metric"):
            out.append(str(t["metric"]))
    return out


def _claim_yaml_block(claim_raw: dict) -> str:
    """Render the claim dict back to YAML for inclusion in the prompt.

    The model sees the full structured claim (tier, all tolerances,
    evidence pointer, last_verified) — multi-criterion claims must
    reveal every criterion.
    """
    import yaml

    return yaml.safe_dump(claim_raw, sort_keys=False)


if __name__ == "__main__":
    main()
