"""Tests for manifest loading + claim filtering."""

from __future__ import annotations

from pathlib import Path
from textwrap import dedent

import pytest

from evident_agent.manifest import filter_claims, load_claims


def _write_manifest(path: Path, body: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(dedent(body).strip() + "\n")
    return path


def test_load_inline_claims(tmp_path: Path) -> None:
    m = _write_manifest(
        tmp_path / "evident.yaml",
        """
        version: 0.1
        project: test
        claims:
          - id: claim-A
            kind: measurement
            tier: ci
          - id: claim-B
            kind: policy
            tier: release
        """,
    )
    claims = load_claims(m)
    ids = [c.id for c in claims]
    assert "claim-A" in ids
    assert "claim-B" in ids
    assert all(c.source_path == m for c in claims)


def test_load_with_includes(tmp_path: Path) -> None:
    included = _write_manifest(
        tmp_path / "claims" / "x.yaml",
        """
        claims:
          - id: claim-X
            kind: measurement
        """,
    )
    top = _write_manifest(
        tmp_path / "evident.yaml",
        f"""
        version: 0.1
        project: test
        include:
          - claims/x.yaml
        """,
    )
    # Ensure the included path was created.
    assert included.is_file()
    claims = load_claims(top)
    ids = [c.id for c in claims]
    assert "claim-X" in ids
    # Source path for X should be the included file, not the top-level.
    x = next(c for c in claims if c.id == "claim-X")
    assert x.source_path == included


def test_filter_by_id(tmp_path: Path) -> None:
    m = _write_manifest(
        tmp_path / "evident.yaml",
        """
        claims:
          - id: claim-A
            kind: measurement
          - id: claim-B
            kind: measurement
        """,
    )
    claims = load_claims(m)
    only_a = list(filter_claims(claims, claim_filter="claim-A"))
    assert len(only_a) == 1
    assert only_a[0].id == "claim-A"


def test_filter_excludes_non_measurement_by_default(tmp_path: Path) -> None:
    m = _write_manifest(
        tmp_path / "evident.yaml",
        """
        claims:
          - id: meas-A
            kind: measurement
          - id: pol-B
            kind: policy
          - id: ref-C
            kind: reference
        """,
    )
    claims = load_claims(m)
    meas = list(filter_claims(claims))  # default kind="measurement"
    assert [c.id for c in meas] == ["meas-A"]
    # kind=None disables the filter.
    all_kinds = list(filter_claims(claims, kind=None))
    assert {c.id for c in all_kinds} == {"meas-A", "pol-B", "ref-C"}
