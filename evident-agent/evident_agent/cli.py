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

from . import (
    docker,
    evidence,
    manifest,
    prompt as prompt_mod,
    review as review_mod,
    review_sidecar,
    scoring,
    sidecar,
    typed_trust,
    violation as violation_mod,
)


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
    "models",
    required=True,
    type=str,
    multiple=True,
    help=(
        "Anthropic model id (e.g. claude-opus-4-7). Repeatable for "
        "Phase 2c multi-model panels: `--model claude-opus-4-7 "
        "--model claude-haiku-4-5-20251001` runs both against the "
        "same claim digest and appends one sidecar entry per model. "
        "Sequential by default; flock keeps the sidecar consistent."
    ),
)
@click.option(
    "--model-version",
    default=None,
    type=str,
    help=(
        "Author version for the sidecar entry. Default: same as the "
        "model id. With multi-model --model the override applies to "
        "every model uniformly; for per-model versions use distinct "
        "model ids."
    ),
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
    "--record",
    "record_path",
    default=None,
    type=click.Path(path_type=Path),
    help=(
        "Capture each successful API response as a fixture entry "
        "({id, tool_input, digest}) under this path. One file per "
        "claim id: <record_path>/<claim_id>.json. Used to bootstrap "
        "deferred CI fixtures from a real model run."
    ),
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
    models: tuple[str, ...],
    model_version: Optional[str],
    review_sidecar_path: Optional[Path],
    last_verified_sidecar_path: Optional[Path],
    no_api: bool,
    record_path: Optional[Path],
    render: Optional[str],
    typed_trust_binary: Optional[str],
) -> None:
    """Author Endorse / Dissent / Challenge ReviewEvents on a claim's
    evidence (Phase 2a + 2b + 2c).

    Per claim, per model:
      1. Build a per-format evidence digest from the cited artifact
         (built once per claim; every model sees the same digest).
      2. Construct a default-Dissent prompt with the submit_review tool.
      3. Call the Anthropic API (one retry on transport failure) using
         an independent client per model — no shared session state.
      4. Validate the response — reject Endorse-with-failing-check,
         hallucinated criterion names, short rationales.
      5. For substantive Challenge: validate_contradiction +
         build_backing_claim with the full author identity folded into
         the short-hash.
      6. Append the validated entry to review_events.json under flock.
      7. Log stderr telemetry per call (model, verdict, tokens,
         elapsed_ms).
    After the per-claim loop:
      8. Print end-of-run compact summary when N > 1.
      9. Optionally invoke typed-trust to emit a rendered report.
    """
    if review_sidecar_path is None:
        review_sidecar_path = manifest_path.parent / "review_events.json"
    if last_verified_sidecar_path is None:
        candidate = manifest_path.parent / "last_verified.json"
        if candidate.is_file():
            last_verified_sidecar_path = candidate

    if model_version is None:
        # Phase 2c: when --model-version isn't supplied, each model uses
        # its own id as the version. With one --model this matches Phase
        # 2a behavior; with multiple --model the versions naturally
        # differ per model.
        model_version = ""
    panel_models: list[str] = list(models)

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

        # Build messages + call API. Phase 2c: loop over the panel of
        # models. Each model gets a fresh client (call_review's lazy
        # default_api_client makes a new Anthropic instance per call),
        # the same digest, and no shared session state. Verdicts
        # are aggregated into per-claim panel telemetry at the end.
        claim_yaml_text = _claim_yaml_block(claim.raw)
        claim_results: list[tuple[str, str, "review_mod.ReviewVerdict"]] = []
        for j, model in enumerate(panel_models, start=1):
            mv = model_version if model_version else model
            click.echo(f"  [{j}/{len(panel_models)}] via {model} ({mv})")

            import time as _time

            t0 = _time.monotonic()
            try:
                verdict = review_mod.call_review(
                    model=model,
                    claim_yaml=claim_yaml_text,
                    digest_rendered=digest_obj.render(),
                )
            except review_mod.ReviewTransportError as exc:
                click.echo(
                    f"    skip: transport error after retry: {exc}", err=True
                )
                continue
            except review_mod.ReviewRejected as exc:
                click.echo(
                    f"    skip: response rejected by validation: {exc}", err=True
                )
                continue
            elapsed_ms = int((_time.monotonic() - t0) * 1000)

            # Hallucination check requires the claim's criteria ids.
            criteria_ids = _claim_criterion_ids(claim.raw)
            try:
                review_mod.reject_if_hallucinated_criterion(verdict, criteria_ids)
            except review_mod.ReviewRejected as exc:
                click.echo(f"    skip: hallucinated criterion: {exc}", err=True)
                continue

            # Truncated-evidence-without-citation check (F9). The model
            # cannot Endorse when the digest was truncated and its
            # cited observed_value isn't in the digest text.
            try:
                review_mod.reject_if_truncated_endorse_lacks_evidence(
                    verdict, digest_obj.body, digest_obj.truncated
                )
            except review_mod.ReviewRejected as exc:
                click.echo(
                    f"    skip: truncated digest, no citation: {exc}", err=True
                )
                continue

            # Phase 2b: substantive Challenges need their violation
            # contradiction-checked against the target before we
            # materialize the backing claim. Procedural Challenges and
            # Endorse/Dissent skip this step.
            if (
                verdict.verdict == "challenge"
                and verdict.challenge_category in prompt_mod.SUBSTANTIVE_CATEGORIES
            ):
                try:
                    violation_mod.validate_contradiction(
                        claim.raw,
                        verdict.challenge_target_criterion_id or "",
                        verdict.challenge_violation or {},
                    )
                except violation_mod.ViolationRejected as exc:
                    click.echo(
                        f"    skip: substantive challenge violation rejected: {exc}",
                        err=True,
                    )
                    continue

            try:
                entry = review_mod.verdict_to_sidecar_entry(
                    verdict,
                    claim_id=claim.id,
                    author_name=model,
                    author_version=mv,
                    author_context="evident-agent review v0.2c",
                    target_claim=claim.raw,
                )
            except review_mod.ReviewRejected as exc:
                click.echo(
                    f"    skip: sidecar construction rejected: {exc}", err=True
                )
                continue

            click.echo(
                f"    verdict: {verdict.verdict} elapsed_ms={elapsed_ms} "
                f"(rationale: {verdict.rationale[:60]}…)"
            )
            if verdict.verdict == "challenge" and entry.challenge is not None:
                backing_id = (entry.challenge.get("backing_claim") or {}).get("id")
                click.echo(
                    f"    challenge: category={verdict.challenge_category}"
                    + (
                        f" backing={backing_id}"
                        if backing_id
                        else " (procedural, no backing)"
                    )
                )

            # --record: capture per-model fixture under a model-named
            # subdirectory so multi-model runs don't clobber each
            # other. Single-model runs land at the top-level for
            # backward compatibility with the Phase 2a/b fixtures.
            #
            # Codex F-CR2C-1: claim ids and model names are arbitrary
            # strings. `_safe_subdir` sanitizes the per-claim subdir
            # name and re-verifies the resolved path stays inside the
            # record dir before any file write. `_write_record_fixture`
            # then runs `_safe_fixture_path` on the per-model fixture
            # filename. Both safety checks compose.
            if record_path is not None:
                target_dir = (
                    _safe_subdir(record_path, claim.id)
                    if len(panel_models) > 1
                    else record_path
                )
                fixture_name = model if len(panel_models) > 1 else claim.id
                _write_record_fixture(
                    target_dir,
                    claim_id=fixture_name,
                    verdict=verdict,
                    digest_rendered=digest_obj.render(),
                )
                click.echo(
                    f"    recorded: {target_dir / (fixture_name + '.json')}"
                )

            claim_results.append((model, mv, verdict))
            new_entries.append(entry)

        # End-of-claim panel summary line — codex F-2C-12. Only emit
        # when more than one model was requested (per-claim panel).
        if len(panel_models) > 1:
            verdict_counts = {"endorse": 0, "dissent": 0, "challenge": 0}
            for _, _, v in claim_results:
                verdict_counts[v.verdict] = verdict_counts.get(v.verdict, 0) + 1
            non_zero = {k: n for k, n in verdict_counts.items() if n > 0}
            summary_parts = ", ".join(f"{n} {k}" for k, n in non_zero.items())
            click.echo(
                f"  Panel summary for {claim.id}: "
                f"{len(claim_results)} reviewers: {summary_parts}"
            )
            for model, mv, v in claim_results:
                click.echo(f"    - {model} ({mv}): {v.verdict}")

    if not new_entries:
        click.echo("no review events written; sidecar untouched", err=True)
        # Codex F-2C-11: if --render is requested with no successful
        # calls, render from the existing sidecar when it exists.
        # Otherwise log explicitly.
        if render is not None and review_sidecar_path.is_file():
            click.echo(
                f"  rendering existing sidecar {review_sidecar_path}; "
                f"current run contributed no new events",
                err=True,
            )
        elif render is not None:
            click.echo("  no review events found for any claim", err=True)
    else:
        review_sidecar.append_events(review_sidecar_path, new_entries)
        click.echo(
            f"review sidecar updated: {review_sidecar_path} ({len(new_entries)} new)"
        )

    if render is not None:
        # F-2C-11: if the sidecar doesn't exist (no events ever), skip
        # the typed-trust invocation cleanly.
        if not review_sidecar_path.is_file():
            return
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


def _write_record_fixture(
    record_dir: Path,
    *,
    claim_id: str,
    verdict: "review_mod.ReviewVerdict",
    digest_rendered: str,
) -> None:
    """Materialize a fixture entry in the shape the deferred CI tests
    expect: ``{id, tool_input, digest}``. ``tool_input`` is
    reconstructed from the validated ReviewVerdict — equivalent for
    round-trip purposes, since the test replays through the same
    validator pipeline.
    """
    import json

    record_dir.mkdir(parents=True, exist_ok=True)
    tool_input: dict[str, object] = {
        "verdict": verdict.verdict,
        "checks": dict(verdict.checks),
        "observed_value": verdict.observed_value,
        "tolerance": verdict.tolerance,
        "failure_reason": verdict.failure_reason,
        "rationale": verdict.rationale,
    }
    if verdict.verdict == "challenge":
        ch: dict[str, object] = {"category": verdict.challenge_category}
        if verdict.challenge_target_criterion_id is not None:
            ch["target_criterion_id"] = verdict.challenge_target_criterion_id
        if verdict.challenge_violation is not None:
            ch["violation"] = dict(verdict.challenge_violation)
        tool_input["challenge"] = ch

    fixture = {
        "id": verdict.request_id or "msg_recorded",
        "tool_input": tool_input,
        "digest": digest_rendered,
    }
    out_path = _safe_fixture_path(record_dir, claim_id)
    out_path.write_text(json.dumps(fixture, indent=2, sort_keys=False) + "\n")


def _sanitize_path_component(raw: str) -> str:
    """Turn an arbitrary string into a safe single-segment filename.

    Replaces path separators / drive letters / control chars and
    neutralizes lone ``.``/``..`` and dot-prefixed segments. The
    returned string is suitable as a directory name OR a file stem
    inside a known-safe parent. The caller still verifies the resolved
    path stays inside the parent via ``_safe_fixture_path`` /
    ``_safe_subdir``.
    """
    import re

    safe = re.sub(r"[/\\\x00-\x1f]", "_", raw)
    if safe in (".", "..") or safe.startswith(".."):
        safe = "_" + safe.lstrip(".")
    if not safe:
        safe = "_unnamed"
    return safe


def _safe_subdir(record_dir: Path, segment: str) -> Path:
    """Compose ``record_dir / <safe_segment>`` and verify the result
    stays inside ``record_dir``. Used by the Phase 2c multi-model
    `--record` path which creates a per-claim subdirectory before
    `_safe_fixture_path` runs on the per-model fixture name.

    Codex F-CR2C-1 regression: previously ``record_path / claim.id``
    was composed raw, so a claim id containing ``/`` or ``..`` would
    escape the record dir before any sanitization fired.
    """
    safe = _sanitize_path_component(segment)
    candidate = (record_dir / safe).resolve()
    record_root = record_dir.resolve()
    try:
        candidate.relative_to(record_root)
    except ValueError as exc:
        raise click.UsageError(
            f"refusing to compose record subdir for {segment!r}: "
            f"sanitized path {candidate} escapes record directory {record_root}"
        ) from exc
    return candidate


def _safe_fixture_path(record_dir: Path, claim_id: str) -> Path:
    """Compose ``record_dir / <safe_claim_id>.json`` and verify the
    result stays inside ``record_dir``.

    Claim ids are arbitrary strings elsewhere in the repo: namespaced
    ids like ``org/claim`` and traversal-like ids are syntactically
    valid manifest input. Using them as raw path components either
    creates files outside the record dir or fails on missing
    intermediates. Sanitize by replacing path separators / drive
    letters / control chars / dot-prefixed segments with ``_``, then
    re-check via ``Path.resolve()`` that the result is a child of
    ``record_dir``. Any residual escape is a hard error.
    """
    safe = _sanitize_path_component(claim_id)
    candidate = (record_dir / f"{safe}.json").resolve()
    record_root = record_dir.resolve()
    try:
        candidate.relative_to(record_root)
    except ValueError as exc:
        raise click.UsageError(
            f"refusing to record fixture for claim id {claim_id!r}: "
            f"sanitized path {candidate} escapes record directory {record_root}"
        ) from exc
    return candidate


@main.command()
@click.option(
    "--repo",
    "repo_path",
    required=True,
    type=click.Path(exists=True, file_okay=False, dir_okay=True, path_type=Path),
    help="Path to a local git repo (or directory tree) to extract from.",
)
@click.option(
    "--output-dir",
    "output_dir",
    required=True,
    type=click.Path(file_okay=False, path_type=Path),
    help="Directory to write extracted/<artifact-id>/ outputs into.",
)
@click.option(
    "--model",
    default="claude-opus-4-7",
    show_default=True,
    help="Anthropic model id to use for extraction.",
)
@click.option(
    "--dry-run",
    is_flag=True,
    default=False,
    help=(
        "Source-audit mode: walk the repo and emit EXTRACTION.md + "
        "dry_run.json describing what WOULD be sent to the model. "
        "No API call is made; no evident.yaml is written."
    ),
)
@click.option(
    "--project",
    default=None,
    help=(
        "Override the manifest's `project:` field. Defaults to "
        "`extracted/<source-id>`."
    ),
)
def extract(
    repo_path: Path,
    output_dir: Path,
    model: str,
    dry_run: bool,
    project: Optional[str],
) -> None:
    """Phase 5: extract structured claims from a local repo.

    Reads README + CHANGELOG + docs from the repo, redacts external
    citations (DOIs, arXiv links, preprint URLs, bibliography
    sections), and asks the model to extract structured tolerances.
    Each tolerance is validated by the source-span/local-binding
    validator before reaching the manifest.

    Per the v3 plan, this PR is the **repo** walker only. Paper
    extraction (--paper) ships in PR6.
    """
    from . import extract as _extract_pkg

    result = _extract_pkg.cli.run_extract_repo(
        repo_path=repo_path,
        output_dir=output_dir,
        project=project,
        model=model,
        dry_run=dry_run,
    )
    if dry_run:
        click.echo(
            f"dry-run audit written to {output_dir}/EXTRACTION.md "
            "(no model call made)"
        )
        return
    assert result is not None
    click.echo(
        f"extracted {len(result.claims)} claim(s), "
        f"{len(result.rejections)} rejection(s) — output in {output_dir}"
    )


if __name__ == "__main__":
    main()
