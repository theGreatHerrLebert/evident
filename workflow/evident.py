#!/usr/bin/env python3
"""EVIDENT CLI.

Subcommands:
  validate    structural manifest checks (delegates to validate_manifest.py)
  list        enumerate claims, with filters and optional JSON/TSV output

The replay subcommand is intentionally absent — it lands when the verifier
field is designed.
"""

from __future__ import annotations

import argparse
import json
import pathlib
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

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
