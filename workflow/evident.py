#!/usr/bin/env python3
"""EVIDENT CLI.

Subcommands:
  validate    structural manifest checks (delegates to validate_manifest.py)
  list        enumerate claims, with filters and optional JSON/TSV output
  draft       emit a claim YAML stub from flags or an existing pytest file

The replay subcommand is intentionally absent — it lands when the verifier
field is designed.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

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
    claims = _load_claims(pathlib.Path(args.manifest))
    rows = [_row_for(c) for c in claims]
    rows = _filter(rows, args.tier, args.oracle, args.id)

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
    p_list.set_defaults(func=cmd_list)

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
