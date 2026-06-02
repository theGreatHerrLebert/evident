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
    assert all(c.source_path == m.resolve() for c in claims)
    # For inline claims, source_path == top_manifest_path.
    assert all(c.top_manifest_path == m.resolve() for c in claims)


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
    # Source path for X should be the included file (for SourceSpan).
    x = next(c for c in claims if c.id == "claim-X")
    assert x.source_path == included.resolve()
    # But the TOP manifest path is the entry-point, not the included file.
    assert x.top_manifest_path == top.resolve()


def test_source_dir_resolves_from_top_manifest_not_include(tmp_path: Path) -> None:
    """Per workflow/SCHEMA.md, claim.source is relative to the top
    manifest dir. For ``proteon/evident/evident.yaml`` including
    ``claims/foo.yaml`` with ``source: ..``, the source dir is
    ``proteon/``, not ``proteon/evident/`` (which is what
    include_file.parent / '..' would give).
    """
    # Layout mirrors proteon's: top_dir/manifest.yaml + top_dir/claims/x.yaml
    # with claim.source = "..", expecting top_dir/.. as the source.
    top_dir = tmp_path / "evident"
    top_dir.mkdir()
    repo_root = tmp_path  # the parent of top_dir
    (repo_root / "repo_marker").write_text("yes")

    _write_manifest(
        top_dir / "claims" / "x.yaml",
        """
        claims:
          - id: claim-X
            kind: measurement
            source: ..
        """,
    )
    top = _write_manifest(
        top_dir / "evident.yaml",
        """
        version: 0.1
        project: test
        include:
          - claims/x.yaml
        """,
    )

    claims = load_claims(top)
    x = next(c for c in claims if c.id == "claim-X")

    # Correct resolution: evident.yaml's parent / ".." == tmp_path.
    # Wrong resolution (what the bug did): claims/x.yaml's parent / ".."
    # == top_dir (the manifest dir).
    expected = (top_dir / "..").resolve()
    assert x.source_dir() == expected
    assert (x.source_dir() / "repo_marker").is_file()


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
