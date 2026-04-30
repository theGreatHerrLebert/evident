#!/usr/bin/env python3
"""EVIDENT CLI.

Subcommands:
  validate    structural manifest checks (delegates to validate_manifest.py)
  list        enumerate claims, with filters and optional JSON/TSV output
  draft       emit a claim YAML stub from flags or an existing pytest file
  replay      re-execute claims' evidence.command and update last_verified
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
import os
import pathlib
import re
import signal
import subprocess
import sys
import time

import validate_manifest


def _load_claims(manifest: pathlib.Path) -> list[dict]:
    if not manifest.exists():
        sys.stderr.write(f"manifest not found: {manifest}\n")
        sys.exit(2)
    return validate_manifest._collect_claims(manifest)


def _clean(value: object) -> str:
    """Strip YAML folded-scalar trailing newlines and collapse internal whitespace."""
    if value is None:
        return ""
    return " ".join(str(value).split())


def _row_for(claim: dict) -> dict:
    evidence = claim.get("evidence") or {}
    return {
        "id": claim.get("id", ""),
        "tier": claim.get("tier", ""),
        "oracles": list(evidence.get("oracle") or []),
        "title": _clean(claim.get("title", "")),
        "command": _clean(evidence.get("command", "")),
        "artifact": _clean(evidence.get("artifact", "")),
    }


def _filter(rows: list[dict], tier: str | None, oracle: str | None, id_sub: str | None) -> list[dict]:
    out = []
    for r in rows:
        if tier and r["tier"] != tier:
            continue
        if id_sub and id_sub not in r["id"]:
            continue
        if oracle:
            needle = oracle.lower()
            if not any(needle in o.lower() for o in r["oracles"]):
                continue
        out.append(r)
    return out


def _format_cell(row: dict, col: str) -> str:
    val = row[col]
    if isinstance(val, list):
        return ", ".join(val)
    return str(val).replace("\n", " ").strip()


def _render_table(rows: list[dict]) -> None:
    cols = ["id", "tier", "oracles", "title"]
    headers = {"id": "ID", "tier": "TIER", "oracles": "ORACLES", "title": "TITLE"}
    caps = {"id": 50, "tier": 10, "oracles": 35, "title": 60}

    if not rows:
        print("(no claims match)")
        return

    widths = {}
    for col in cols:
        widest_data = max((len(_format_cell(r, col)) for r in rows), default=0)
        widths[col] = min(max(len(headers[col]), widest_data), caps[col])

    def trunc(text: str, width: int) -> str:
        if len(text) <= width:
            return text
        return text[: max(width - 3, 0)] + "..."

    print("  ".join(headers[c].ljust(widths[c]) for c in cols))
    print("  ".join("-" * widths[c] for c in cols))
    for r in rows:
        line_cells = [trunc(_format_cell(r, c), widths[c]).ljust(widths[c]) for c in cols]
        print("  ".join(line_cells))


def _render_tsv(rows: list[dict]) -> None:
    print("id\ttier\toracles\ttitle\tcommand\tartifact")
    for r in rows:
        print("\t".join([
            r["id"],
            r["tier"],
            ",".join(r["oracles"]),
            _format_cell(r, "title"),
            _format_cell(r, "command"),
            _format_cell(r, "artifact"),
        ]))


def _render_json(rows: list[dict]) -> None:
    print(json.dumps(rows, indent=2))


def cmd_validate(args: argparse.Namespace) -> int:
    try:
        validate_manifest.validate_manifest(pathlib.Path(args.manifest))
    except Exception as exc:
        print(f"manifest invalid: {exc}", file=sys.stderr)
        return 1
    print(f"manifest valid: {args.manifest}")
    return 0


def cmd_list(args: argparse.Namespace) -> int:
    manifest_path = pathlib.Path(args.manifest)
    claims = _load_claims(manifest_path)
    rows = [_row_for(c) for c in claims]
    rows = _filter(rows, args.tier, args.oracle, args.id)

    # Stale filter — drops claims whose sidecar entry is fresher than
    # `--stale DAYS`. Reads `last_verified.json` next to the manifest;
    # missing sidecar means every claim is treated as never-run, which
    # is correct: until replay populates the sidecar, nothing has been
    # verified.
    if args.stale is not None:
        state = _load_sidecar(manifest_path)
        today = _dt.date.today()
        ids_stale = {
            cid
            for cid in (c.get("id") for c in claims)
            if cid and _is_stale(state.get(cid), args.stale, today)
        }
        rows = [r for r in rows if r["id"] in ids_stale]

    if args.format == "json":
        _render_json(rows)
    elif args.format == "tsv":
        _render_tsv(rows)
    else:
        _render_table(rows)
    return 0


# ---------------------------------------------------------------------------
# `draft` — emit a claim YAML stub
#
# The drafted stub is structurally complete (every field the schema requires
# is present) but semantically a starting point: TODO markers occupy fields
# the author must fill in. The validator is the gate — running
# `evident validate` on a fresh draft surfaces the exact set of TODOs that
# are still present, because they fail vocabulary or path checks. That
# round-trip is the entire point: the author iteratively narrows the TODOs
# until the claim validates, and at that point the claim is real.
# ---------------------------------------------------------------------------

# Regex patterns for `--from-test` extraction. AST would be more robust but
# regexes cover the patterns that show up in the existing oracle tests, and
# the failure mode of a missed extraction is "the author edits the TODO" —
# the worst case is no worse than the no-test starting point.
_RE_PYTEST_MARK_ORACLE = re.compile(r'pytest\.mark\.oracle\(\s*["\']([^"\']+)["\']\s*\)')
_RE_ASSERT_BOUND = re.compile(r'assert\b[^,\n]*?\s*([<>]=?|==)\s*([0-9]+(?:\.[0-9]+)?(?:[eE][-+]?[0-9]+)?)')
_RE_MODULE_DOCSTRING = re.compile(r'^\s*"""([^"]+?)"""', re.DOTALL)


def _extract_test_hints(path: pathlib.Path) -> dict:
    """Parse a pytest file and return whatever can be inferred from it.

    Returns a dict with keys:
      oracles: list[str]                  from pytest.mark.oracle("…") calls
      tolerance_bounds: list[(op, value)] from assert ... <op> <number>
      module_title: str | None            first line of module docstring
      command: str                        suggested `pytest …` invocation

    Missing or unparseable signals are simply absent — extraction is
    best-effort and the validator tells the author what's still TODO.
    """
    text = path.read_text(encoding="utf-8")
    oracles = sorted(set(_RE_PYTEST_MARK_ORACLE.findall(text)))
    bounds = [(op, float(val)) for op, val in _RE_ASSERT_BOUND.findall(text)]
    # Deduplicate + cap to keep the stub readable.
    seen = set()
    uniq_bounds: list[tuple[str, float]] = []
    for ob in bounds:
        if ob in seen:
            continue
        seen.add(ob)
        uniq_bounds.append(ob)
        if len(uniq_bounds) >= 8:
            break
    title = None
    m = _RE_MODULE_DOCSTRING.match(text)
    if m:
        first_line = m.group(1).strip().splitlines()[0]
        title = first_line.strip().rstrip(".") if first_line else None
    return {
        "oracles": oracles,
        "tolerance_bounds": uniq_bounds,
        "module_title": title,
        "command": f"pytest {path.as_posix()} -v",
    }


def _yaml_quote_scalar(value: str) -> str:
    """Quote a YAML scalar so leading punctuation does not trigger parsers.

    Used for prose strings that begin with `|`, `>`, `-`, `*`, `?`, etc. —
    YAML treats those as block-scalar or list-marker indicators in
    plain-style.
    """
    needs_quote = (
        not value
        or value[0] in '|>-*?&!@`%'
        or ": " in value
        or value.startswith("- ")
    )
    if needs_quote:
        escaped = value.replace('"', '\\"')
        return f'"{escaped}"'
    return value


def _render_stub_yaml(
    *,
    claim_id: str,
    title: str,
    kind: str,
    subsystem: str | None,
    tier: str,
    case_path: str,
    source: str,
    oracles: list[str],
    command: str,
    tolerance_bounds: list[tuple[str, float]],
    project: str,
) -> str:
    """Produce a YAML stub for a claim with TODO placeholders.

    Stub passes the schema's STRUCTURAL checks (every required field
    present, types correct) but fails semantic checks where the author
    has not yet filled in real values — vocabulary (subsystem, oracle),
    path existence (case), and the project-vs-oracle pinned-version
    cross-check. That is by design: the validator becomes the
    fill-in-the-blanks worksheet.
    """
    lines: list[str] = []
    add = lines.append

    add("claims:")
    add(f"  - id: {claim_id}")
    add(f"    title: {_yaml_quote_scalar(title)}")
    add(f"    kind: {kind}")
    if subsystem:
        add(f"    subsystem: {subsystem}")
    add(f"    case: {case_path}")
    add(f"    source: {source}")
    add(f"    tier: {tier}")
    add("    trust_strategy:")
    add("      - validation")

    if kind == "measurement":
        # capabilities is optional and, if present, must be a non-empty list.
        # The stub omits it rather than emit `capabilities: []` (which the
        # validator rejects); the author re-adds the field with real values.
        add("    inputs:")
        add("      corpus: TODO-corpus-name")
        add("      n: 1")
        add("      class: fixture  # one of: single-chain, multi-chain, random-sample, synthetic, fixture")
        if tier == "release":
            add("      corpus_sha: PENDING-PIN-CORPUS-SHA  # required at tier=release with n>1")
        add("    outputs:")
        add("      TODO_output:")
        add("        unit: TODO")
        add("        description: TODO one-line description.")
        add("    pinned_versions:")
        add(f"      {project}: PENDING-PIN-AT-NEXT-RELEASE")
        for oracle in oracles or ["TODO-oracle"]:
            add(f"      {oracle}: PENDING-PIN")

    add("    claim: >")
    add("      TODO: prose statement of what is being claimed, in one paragraph.")
    add("      Should be redundant with the structured fields, never the source")
    add("      of truth.")

    if kind == "measurement":
        add("    tolerances:")
        if tolerance_bounds:
            for i, (op, value) in enumerate(tolerance_bounds):
                add("      - metric: relative_error  # TODO: confirm metric")
                add(f"        op: \"{op}\"")
                add(f"        value: {value}")
                add(f"        output: TODO_output  # TODO: bind to outputs[*]")
                add(
                    "        prose: "
                    f"{_yaml_quote_scalar(f'TODO: derived from assert ... {op} {value}')}"
                )
        else:
            add("      - metric: relative_error")
            add("        op: \"<\"")
            add("        value: 0.01")
            add("        output: TODO_output")
            add("        prose: TODO tolerance prose.")

    add("    evidence:")
    add("      oracle:")
    for oracle in oracles or ["TODO-oracle"]:
        add(f"        - {oracle}")
    add(f"      command: {_yaml_quote_scalar(command)}")
    if tier == "ci":
        add('      artifact: pytest console output (CI-tier; no persisted artifact)')
    else:
        add("      artifact: TODO/path/to/artifact")

    add("    last_verified:")
    add("      commit: null")
    add("      date: null")
    add("      value: null")
    add("      corpus_sha: null")

    add("    assumptions:")
    add("      - >")
    add("        TODO — list each assumption that, if violated, would invalidate")
    add("        the claim. Assumptions about the oracle, the corpus, the engine")
    add("        settings, and the trust chain belong here.")

    add("    failure_modes:")
    add("      - >")
    add("        TODO — list how the claim could be wrong even when its tolerance")
    add("        passes. Compensating-component bugs, fixture drift, oracle drift,")
    add("        and convention gaps are typical entries.")

    return "\n".join(lines) + "\n"


def cmd_draft(args: argparse.Namespace) -> int:
    hints: dict = {"oracles": [], "tolerance_bounds": [], "module_title": None, "command": ""}
    if args.from_test:
        test_path = pathlib.Path(args.from_test)
        if not test_path.is_file():
            print(f"--from-test path is not a file: {test_path}", file=sys.stderr)
            return 2
        hints = _extract_test_hints(test_path)

    # Resolve each field: explicit flag > from-test hint > TODO placeholder.
    oracles = args.oracle or hints["oracles"] or []
    command = args.command or hints["command"] or "TODO: command to replay this claim"
    title = args.title or hints["module_title"] or "TODO: one-line claim title"

    claim_id = args.id or "TODO-claim-id"
    case_path = args.case or f"claims/{claim_id}.md"
    source = args.source or ".."
    subsystem = args.subsystem  # may be None — required only at kind=measurement

    project = args.project or "TODO-project"
    yaml_text = _render_stub_yaml(
        claim_id=claim_id,
        title=title,
        kind=args.kind,
        subsystem=subsystem,
        tier=args.tier,
        case_path=case_path,
        source=source,
        oracles=oracles,
        command=command,
        tolerance_bounds=hints["tolerance_bounds"],
        project=project,
    )

    if args.out:
        out_path = pathlib.Path(args.out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(yaml_text, encoding="utf-8")
        print(f"wrote {out_path}", file=sys.stderr)
    else:
        sys.stdout.write(yaml_text)
    return 0


# ---------------------------------------------------------------------------
# `replay` — re-execute claims' evidence.command and update last_verified
#
# Closes the loop the schema's last_verified.{date, commit, value, corpus_sha}
# block has been promising. Without active replay, that block stays null on
# every claim and the manifest decays into aspirational text instead of a
# continuously-verified contract.
#
# Storage decision: a sidecar `last_verified.json` file next to the manifest,
# keyed by claim id. Reasons:
#
# - Manifests stay version-controlled and clean — they do not churn on every
#   replay.
# - Sidecar is machine-written; manifests are human-written. Keeping the
#   write boundaries aligned with the read/write authorities prevents the
#   kind of "run regenerated my carefully-edited prose" damage that
#   in-yaml writes would risk.
# - The sidecar can be `.gitignore`d for ephemeral dev replay and committed
#   for release-tier audit, depending on the consumer's policy.
# - "Stale by N days" filtering becomes a single JSON read, no manifest
#   round-trip.
#
# Value extraction is deliberately omitted from v0. The runner records exit
# code + duration + commit + date; numerical-value extraction needs a
# per-claim parser (regex against stdout, JSON-path against an artifact,
# etc.) and is its own design. Until that lands, last_verified.value stays
# null even on green replays — a green run with null value is still strictly
# more information than null-everywhere, and is what powers the
# "stale-by-N-days" filter that follows.
# ---------------------------------------------------------------------------

SIDECAR_FILENAME = "last_verified.json"


def _sidecar_path(manifest: pathlib.Path) -> pathlib.Path:
    return manifest.parent / SIDECAR_FILENAME


def _load_sidecar(manifest: pathlib.Path) -> dict:
    p = _sidecar_path(manifest)
    if not p.exists():
        return {}
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        sys.stderr.write(f"warning: corrupt sidecar at {p}: {exc}\n")
        return {}
    return data if isinstance(data, dict) else {}


def _save_sidecar(manifest: pathlib.Path, state: dict) -> None:
    p = _sidecar_path(manifest)
    p.write_text(
        json.dumps(state, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def _git_short_commit(start: pathlib.Path) -> str | None:
    """Return short HEAD commit for the repo containing `start`, or None.

    Used to fill `commit` in last_verified entries. Returning None when
    git is missing or the path is not in a repo is fine — the entry just
    records the date without a commit pin, which is honest about what
    we know.
    """
    try:
        out = subprocess.run(
            ["git", "-C", str(start), "rev-parse", "--short", "HEAD"],
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0:
        return None
    sha = out.stdout.strip()
    return sha or None


def _run_command(command: str, cwd: pathlib.Path, timeout: float) -> dict:
    """Execute `command` under shell, return a result dict.

    Result keys:
      exit_code: int — process exit code, -1 on timeout, -2 on spawn error
      duration_seconds: float — wall time elapsed
      stdout_tail: str — last ~2 KB of stdout, useful for the eventual
                         value-extractor design
      stderr_tail: str — last ~2 KB of stderr, surfaces failure cause

    Process-group hygiene: the child is started in a new POSIX session
    (`start_new_session=True`) so the shell and any commands it spawns
    share one process group. On timeout we kill the whole group with
    SIGTERM, then SIGKILL if it doesn't exit within 5s. Without this,
    `subprocess.run(..., shell=True, timeout=...)` only kills the shell
    on timeout, leaving long-running benchmark children orphaned —
    real risk for evidence.command lines that invoke pytest with heavy
    fixtures.
    """
    started = time.monotonic()
    try:
        proc = subprocess.Popen(
            command,
            shell=True,
            cwd=str(cwd),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
    except (FileNotFoundError, OSError) as exc:
        return {
            "exit_code": -2,
            "duration_seconds": round(time.monotonic() - started, 3),
            "stdout_tail": "",
            "stderr_tail": f"spawn error: {exc}",
        }

    pgid = os.getpgid(proc.pid)
    try:
        stdout, stderr = proc.communicate(timeout=timeout)
        return {
            "exit_code": proc.returncode,
            "duration_seconds": round(time.monotonic() - started, 3),
            "stdout_tail": (stdout or "")[-2048:],
            "stderr_tail": (stderr or "")[-2048:],
        }
    except subprocess.TimeoutExpired:
        # SIGTERM the whole group, give it 5s, then SIGKILL.
        try:
            os.killpg(pgid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            stdout, stderr = proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(pgid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = proc.communicate()
        return {
            "exit_code": -1,
            "duration_seconds": round(time.monotonic() - started, 3),
            "stdout_tail": (stdout or "")[-2048:],
            "stderr_tail": (
                (stderr or "")[-2048:]
                + f"\n[evident replay] timed out after {timeout}s, "
                f"process group {pgid} killed"
            ),
        }


def _is_stale(entry: dict | None, n_days: int, today: _dt.date) -> bool:
    """Return True if the sidecar entry counts as stale.

    "Never run" qualifies as stale — explicit, not an empty.
    """
    if not entry:
        return True
    iso = entry.get("date")
    if not isinstance(iso, str) or not iso:
        return True
    try:
        d = _dt.date.fromisoformat(iso[:10])
    except ValueError:
        return True
    return (today - d).days >= n_days


def cmd_replay(args: argparse.Namespace) -> int:
    manifest_path = pathlib.Path(args.manifest).resolve()

    # Validate first. Replay executes commands; running them against a
    # manifest with duplicate IDs (sidecar entries clobber each other),
    # invalid paths, malformed evidence, or unknown vocabulary is worse
    # than refusing to run. The validator already enforces all of those
    # invariants.
    try:
        validate_manifest.validate_manifest(manifest_path)
    except (SystemExit, ValueError) as exc:
        msg = str(exc)
        if msg and msg != "1":
            print(f"manifest invalid: {msg}", file=sys.stderr)
        else:
            print("manifest invalid (re-run `evident validate` for details)", file=sys.stderr)
        return 2

    claims = _load_claims(manifest_path)
    state = _load_sidecar(manifest_path)
    today = _dt.date.today()

    # Filter to the claims the user asked for.
    selected: list[dict] = []
    for claim in claims:
        cid = claim.get("id")
        if not cid:
            continue
        if args.id and args.id not in cid:
            continue
        if args.tier and claim.get("tier") != args.tier:
            continue
        if args.stale is not None and not _is_stale(state.get(cid), args.stale, today):
            continue
        selected.append(claim)

    if not selected:
        print("(no claims match)", file=sys.stderr)
        return 0

    today_iso = today.isoformat()
    cwd = manifest_path.parent

    # Cache git commits per source path — typical manifests have one
    # source for every claim, so this avoids spawning git for each.
    commit_cache: dict[pathlib.Path, str | None] = {}

    def _commit_for_claim(claim: dict) -> str | None:
        # Each claim's `source` is a path relative to the manifest root.
        # Resolve it and ask git there. The schema defines
        # last_verified.commit as the SOURCE SHA where the claim
        # passed; recording the manifest's repo SHA when a claim points
        # at a different source root would write a convincing-but-wrong
        # value into the sidecar.
        src = claim.get("source") or "."
        resolved = (manifest_path.parent / src).resolve()
        if resolved not in commit_cache:
            commit_cache[resolved] = _git_short_commit(resolved)
        return commit_cache[resolved]

    n_pass = n_fail = 0
    for claim in selected:
        cid = claim["id"]
        evidence = claim.get("evidence") or {}
        command = (evidence.get("command") or "").strip()
        if not command:
            print(f"  SKIP   {cid}  (no evidence.command)", file=sys.stderr)
            continue

        if args.dry_run:
            print(f"  DRY    {cid}  $ {command}")
            continue

        print(f"  RUN    {cid}  $ {command}", flush=True)
        result = _run_command(command, cwd, args.timeout)
        ok = result["exit_code"] == 0

        # Pull corpus_sha from the claim if it has one — that field is
        # the closest thing to "data version" the schema captures today.
        inputs = claim.get("inputs") or {}
        corpus_sha = inputs.get("corpus_sha") if isinstance(inputs, dict) else None

        entry = {
            "date": today_iso if ok else None,
            "commit": _commit_for_claim(claim) if ok else None,
            "value": None,
            "corpus_sha": corpus_sha if ok else None,
            "exit_code": result["exit_code"],
            "duration_seconds": result["duration_seconds"],
            "command": command,
        }
        # Carry stderr tail into the sidecar on failures so the next
        # operator does not have to re-run to see what blew up.
        if not ok:
            entry["stderr_tail"] = result["stderr_tail"]

        state[cid] = entry
        if ok:
            n_pass += 1
            print(f"  OK     {cid}  ({result['duration_seconds']:.1f}s)")
        else:
            n_fail += 1
            tail = result["stderr_tail"].splitlines()[-1:] or [""]
            print(
                f"  FAIL   {cid}  exit={result['exit_code']} "
                f"({result['duration_seconds']:.1f}s)  {tail[0][:80]}",
                file=sys.stderr,
            )

    if not args.dry_run:
        _save_sidecar(manifest_path, state)
        print(
            f"sidecar: {_sidecar_path(manifest_path)}  "
            f"({n_pass} pass, {n_fail} fail, {len(selected)} total)",
            file=sys.stderr,
        )

    return 0 if n_fail == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(prog="evident")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_val = sub.add_parser("validate", help="Structural check of a manifest")
    p_val.add_argument("manifest", nargs="?", default="evident.yaml")
    p_val.set_defaults(func=cmd_validate)

    p_list = sub.add_parser("list", help="List claims from a manifest")
    p_list.add_argument("manifest", nargs="?", default="evident.yaml")
    p_list.add_argument("--tier", choices=["ci", "release", "research"], default=None)
    p_list.add_argument("--oracle", default=None, help="filter: oracle name substring")
    p_list.add_argument("--id", default=None, help="filter: id substring")
    p_list.add_argument("--format", choices=["table", "json", "tsv"], default="table")
    p_list.add_argument(
        "--stale",
        type=int,
        default=None,
        metavar="DAYS",
        help=(
            "Filter to claims whose sidecar last_verified.date is older than "
            "DAYS (or absent). Reads `last_verified.json` next to the "
            "manifest. Use 0 to surface every claim regardless of age."
        ),
    )
    p_list.set_defaults(func=cmd_list)

    p_replay = sub.add_parser(
        "replay",
        help="Re-execute claims' evidence.command and update last_verified",
        description=(
            "Run each selected claim's evidence.command in the manifest's "
            "directory, capture exit code + duration + commit + date, and "
            "write to a sidecar `last_verified.json`. The sidecar is the "
            "machine-written audit trail; the manifest stays "
            "human-authored and version-controlled."
        ),
    )
    p_replay.add_argument("manifest", nargs="?", default="evident.yaml")
    p_replay.add_argument(
        "--id",
        default=None,
        help="Run only claims whose id contains this substring",
    )
    p_replay.add_argument(
        "--tier",
        choices=["ci", "release", "research"],
        default=None,
        help="Run only claims at this tier",
    )
    p_replay.add_argument(
        "--stale",
        type=int,
        default=None,
        metavar="DAYS",
        help="Run only claims whose last verify is older than DAYS (or absent)",
    )
    p_replay.add_argument(
        "--timeout",
        type=float,
        default=600.0,
        metavar="SEC",
        help="Per-claim wall-time budget (default: 600s)",
    )
    p_replay.add_argument(
        "--dry-run",
        action="store_true",
        help="List the commands that would run; do not execute or write",
    )
    p_replay.set_defaults(func=cmd_replay)

    p_draft = sub.add_parser(
        "draft",
        help="Emit a claim YAML stub from flags or an existing pytest file",
        description=(
            "Generate a structurally-complete claim YAML with TODO markers in "
            "fields the author still has to fill. Validation surfaces the "
            "remaining TODOs as errors, so the author iteratively narrows the "
            "stub until the claim validates."
        ),
    )
    p_draft.add_argument("--id", help="Claim id stem (becomes claims[].id)")
    p_draft.add_argument(
        "--project",
        help=(
            "Project under test — must match the top-level `project` field "
            "of the manifest this claim belongs to. Becomes the key in "
            "pinned_versions that names the source of the claim."
        ),
    )
    p_draft.add_argument("--title", help="One-line title for the claim")
    p_draft.add_argument(
        "--kind",
        choices=["measurement", "policy", "reference"],
        default="measurement",
        help="Claim kind (default: measurement)",
    )
    p_draft.add_argument(
        "--tier",
        choices=["ci", "release", "research"],
        default="ci",
        help="Tier (default: ci)",
    )
    p_draft.add_argument(
        "--subsystem",
        help="Subsystem from vocabularies.subsystem (required for kind=measurement)",
    )
    p_draft.add_argument("--case", help="Path to operational case writeup (default: claims/<id>.md)")
    p_draft.add_argument("--source", help="Path to project-under-test root (default: ..)")
    p_draft.add_argument(
        "--oracle",
        action="append",
        help="Add an oracle name to evidence.oracle (repeatable)",
    )
    p_draft.add_argument(
        "--command",
        help="evidence.command (default inferred from --from-test, else TODO)",
    )
    p_draft.add_argument(
        "--from-test",
        help=(
            "Pytest file to scrape for hints (oracle markers, tolerance "
            "bounds, module docstring title). Best-effort regex parse."
        ),
    )
    p_draft.add_argument(
        "--out",
        help="Write the stub to this path instead of stdout",
    )
    p_draft.set_defaults(func=cmd_draft)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
