"""Tests for typed-trust binary discovery."""

from __future__ import annotations

import os
from pathlib import Path
from unittest import mock

from evident_agent.typed_trust import find_binary


def test_explicit_override_wins() -> None:
    assert find_binary("/custom/path/typed-trust") == "/custom/path/typed-trust"


def test_env_var_is_used_when_no_override(monkeypatch) -> None:
    monkeypatch.setenv("TYPED_TRUST_BIN", "/from/env/typed-trust")
    # Make sure PATH lookup won't preempt the env var.
    monkeypatch.setattr("shutil.which", lambda _: None)
    assert find_binary() == "/from/env/typed-trust"


def test_path_lookup_when_no_override_no_env(monkeypatch) -> None:
    monkeypatch.delenv("TYPED_TRUST_BIN", raising=False)
    monkeypatch.setattr("shutil.which", lambda name: "/usr/local/bin/typed-trust")
    assert find_binary() == "/usr/local/bin/typed-trust"


def test_falls_back_to_bare_name_when_nothing_resolves(monkeypatch) -> None:
    monkeypatch.delenv("TYPED_TRUST_BIN", raising=False)
    monkeypatch.setattr("shutil.which", lambda _: None)
    monkeypatch.setattr(
        "evident_agent.typed_trust._candidate_typed_trust_dirs",
        lambda: [],
    )
    assert find_binary() == "typed-trust"
