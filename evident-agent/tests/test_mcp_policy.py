"""Unit tests for the MCP allow-list path policy, incl. the hardened
``allow_missing`` mode and symlink/TOCTOU defenses."""

from __future__ import annotations

from pathlib import Path

import pytest

from evident_agent.mcp.policy import AllowListPathPolicy, PolicyDenied


def test_empty_policy_denies() -> None:
    pol = AllowListPathPolicy()
    assert pol.is_empty()
    with pytest.raises(PolicyDenied):
        pol.check("/tmp")


def test_dir_root_allows_inside_denies_outside(tmp_path: Path) -> None:
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    inside = allowed / "m.yaml"
    inside.write_text("x")
    outside = tmp_path / "outside.yaml"
    outside.write_text("x")

    pol = AllowListPathPolicy()
    pol.allow(allowed)

    assert pol.check(inside) == inside.resolve()
    with pytest.raises(PolicyDenied):
        pol.check(outside)


def test_file_root_exact_match(tmp_path: Path) -> None:
    f = tmp_path / "only.yaml"
    f.write_text("x")
    sib = tmp_path / "other.yaml"
    sib.write_text("x")

    pol = AllowListPathPolicy()
    pol.allow(f)
    assert pol.check(f) == f.resolve()
    with pytest.raises(PolicyDenied):
        pol.check(sib)


def test_allow_missing_inside_root(tmp_path: Path) -> None:
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    target = allowed / "sub" / "sidecar.json"  # does not exist yet

    pol = AllowListPathPolicy()
    pol.allow(allowed)

    # strict check fails (missing); allow_missing authorizes it
    with pytest.raises(PolicyDenied):
        pol.check(target)
    got = pol.check(target, allow_missing=True)
    assert got == (allowed.resolve() / "sub" / "sidecar.json")


def test_allow_missing_rejects_dotdot_escape(tmp_path: Path) -> None:
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    escape = allowed / "x" / ".." / ".." / "evil.json"

    pol = AllowListPathPolicy()
    pol.allow(allowed)
    with pytest.raises(PolicyDenied):
        pol.check(escape, allow_missing=True)


def test_symlink_escape_denied(tmp_path: Path) -> None:
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    outside = tmp_path / "outside"
    outside.mkdir()
    secret = outside / "secret.txt"
    secret.write_text("x")
    link = allowed / "link.txt"
    link.symlink_to(secret)

    pol = AllowListPathPolicy()
    pol.allow(allowed)
    # symlink target escapes the root → denied even though the link is inside
    with pytest.raises(PolicyDenied):
        pol.check(link)


def test_symlink_inside_allowed(tmp_path: Path) -> None:
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    real = allowed / "real.txt"
    real.write_text("x")
    link = allowed / "link.txt"
    link.symlink_to(real)

    pol = AllowListPathPolicy()
    pol.allow(allowed)
    assert pol.check(link) == real.resolve()


def test_recheck_catches_toctou_symlink_swap(tmp_path: Path) -> None:
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    outside = tmp_path / "outside"
    outside.mkdir()
    target = allowed / "new" / "sidecar.json"

    pol = AllowListPathPolicy()
    pol.allow(allowed)
    # authorized while missing
    pol.check(target, allow_missing=True)

    # attacker swaps the intermediate dir for a symlink pointing outside
    (allowed / "new").symlink_to(outside)
    (outside / "sidecar.json").write_text("evil")
    with pytest.raises(PolicyDenied):
        pol.recheck(target)
