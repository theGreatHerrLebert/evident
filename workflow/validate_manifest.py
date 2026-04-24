#!/usr/bin/env python3
"""Validate the lightweight EVIDENT manifest structure.

This intentionally checks structure, not scientific truth. Domain-specific
claims still need their own oracle or benchmark commands.
"""

from __future__ import annotations

import argparse
import pathlib
import sys
from typing import Any

try:
    import yaml
except ImportError as exc:  # pragma: no cover - only hit in incomplete images
    raise SystemExit("Missing dependency: PyYAML") from exc


REQUIRED_CLAIM_FIELDS = {
    "id",
    "title",
    "case",
    "source",
    "tier",
    "trust_strategy",
    "claim",
    "evidence",
    "assumptions",
    "failure_modes",
}

VALID_TIERS = {"ci", "release", "research"}
VALID_TRUST_STRATEGIES = {"understanding", "validation", "proof"}


def fail(message: str) -> None:
    raise ValueError(message)


def require_non_empty_string(value: Any, field: str, claim_id: str) -> None:
    if not isinstance(value, str) or not value.strip():
        fail(f"claim {claim_id}: {field} must be a non-empty string")


def require_string_list(value: Any, field: str, claim_id: str) -> list[str]:
    if not isinstance(value, list) or not value:
        fail(f"claim {claim_id}: {field} must be a non-empty list")
    for item in value:
        if not isinstance(item, str) or not item.strip():
            fail(f"claim {claim_id}: {field} must contain only non-empty strings")
    return value


def validate_existing_path(root: pathlib.Path, value: Any, field: str, claim_id: str) -> None:
    require_non_empty_string(value, field, claim_id)
    path = root / value
    if "#" in str(path):
        path = pathlib.Path(str(path).split("#", 1)[0])
    if not path.exists():
        fail(f"claim {claim_id}: {field} path does not exist: {value}")


def validate_evidence(value: Any, claim_id: str) -> None:
    if not isinstance(value, dict):
        fail(f"claim {claim_id}: evidence must be a mapping")
    for field in ("oracle", "tolerance", "command", "artifact"):
        if field not in value:
            fail(f"claim {claim_id}: evidence.{field} is required")
    require_string_list(value["oracle"], "evidence.oracle", claim_id)
    require_non_empty_string(value["tolerance"], "evidence.tolerance", claim_id)
    require_non_empty_string(value["command"], "evidence.command", claim_id)
    require_non_empty_string(value["artifact"], "evidence.artifact", claim_id)


def validate_manifest(path: pathlib.Path) -> None:
    root = path.parent
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        fail("manifest must be a mapping")
    if data.get("version") is None:
        fail("version is required")
    claims = data.get("claims")
    if not isinstance(claims, list) or not claims:
        fail("claims must be a non-empty list")

    seen_ids: set[str] = set()
    for index, claim in enumerate(claims):
        if not isinstance(claim, dict):
            fail(f"claim at index {index} must be a mapping")
        missing = sorted(REQUIRED_CLAIM_FIELDS - claim.keys())
        claim_id = str(claim.get("id", f"<index:{index}>"))
        if missing:
            fail(f"claim {claim_id}: missing required fields: {', '.join(missing)}")
        require_non_empty_string(claim["id"], "id", claim_id)
        if claim["id"] in seen_ids:
            fail(f"duplicate claim id: {claim['id']}")
        seen_ids.add(claim["id"])

        require_non_empty_string(claim["title"], "title", claim_id)
        require_non_empty_string(claim["claim"], "claim", claim_id)
        validate_existing_path(root, claim["case"], "case", claim_id)
        validate_existing_path(root, claim["source"], "source", claim_id)
        if "pattern" in claim:
            validate_existing_path(root, claim["pattern"], "pattern", claim_id)

        require_non_empty_string(claim["tier"], "tier", claim_id)
        if claim["tier"] not in VALID_TIERS:
            fail(f"claim {claim_id}: invalid tier {claim['tier']!r}")

        strategies = require_string_list(claim["trust_strategy"], "trust_strategy", claim_id)
        invalid = sorted(set(strategies) - VALID_TRUST_STRATEGIES)
        if invalid:
            fail(f"claim {claim_id}: invalid trust strategies: {', '.join(invalid)}")

        validate_evidence(claim["evidence"], claim_id)
        require_string_list(claim["assumptions"], "assumptions", claim_id)
        require_string_list(claim["failure_modes"], "failure_modes", claim_id)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", nargs="?", default="evident.yaml")
    args = parser.parse_args()

    try:
        validate_manifest(pathlib.Path(args.manifest))
    except Exception as exc:
        print(f"manifest invalid: {exc}", file=sys.stderr)
        return 1
    print(f"manifest valid: {args.manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
