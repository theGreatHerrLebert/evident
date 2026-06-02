"""Tests for sidecar read/write/merge."""

from __future__ import annotations

import json
from pathlib import Path

from evident_agent.sidecar import LastVerifiedEntry, merge, read, write


def test_read_missing_file_returns_empty(tmp_path: Path) -> None:
    assert read(tmp_path / "nonexistent.json") == {}


def test_write_then_read_roundtrip(tmp_path: Path) -> None:
    path = tmp_path / "last_verified.json"
    entries = {
        "claim-A": LastVerifiedEntry(
            commit="abc123",
            date="2026-06-02",
            value=0.0017,
            corpus_sha="deadbeef",
        ),
        "claim-B": LastVerifiedEntry(
            date="2026-06-02",
            value=None,  # null value (e.g. command failed)
        ),
    }
    write(path, entries)
    loaded = read(path)
    assert loaded["claim-A"].commit == "abc123"
    assert loaded["claim-A"].value == 0.0017
    assert loaded["claim-B"].value is None
    assert loaded["claim-B"].commit is None


def test_to_dict_omits_none_fields() -> None:
    e = LastVerifiedEntry(date="2026-06-02", value=0.5)
    d = e.to_dict()
    assert d == {"date": "2026-06-02", "value": 0.5}
    assert "commit" not in d
    assert "corpus_sha" not in d


def test_merge_new_entries_win(tmp_path: Path) -> None:
    existing = {
        "claim-A": LastVerifiedEntry(value=0.001, date="2026-05-01"),
        "claim-B": LastVerifiedEntry(value=0.002),
    }
    new = {
        "claim-A": LastVerifiedEntry(value=0.003, date="2026-06-02"),
        "claim-C": LastVerifiedEntry(value=0.004),
    }
    out = merge(existing, new)
    assert out["claim-A"].value == 0.003  # new wins
    assert out["claim-A"].date == "2026-06-02"
    assert out["claim-B"].value == 0.002  # unchanged
    assert out["claim-C"].value == 0.004  # added


def test_write_is_pretty_printed(tmp_path: Path) -> None:
    """The sidecar should be human-readable JSON (indent=2, sorted keys)."""
    path = tmp_path / "lv.json"
    entries = {
        "z-claim": LastVerifiedEntry(value=0.1),
        "a-claim": LastVerifiedEntry(value=0.2),
    }
    write(path, entries)
    text = path.read_text()
    # Keys sorted alphabetically.
    assert text.index("a-claim") < text.index("z-claim")
    # Indented (newlines + spaces).
    assert "\n  " in text


def test_typed_trust_can_read_what_we_write(tmp_path: Path) -> None:
    """Cross-check: the file we emit deserializes as the shape typed-trust
    expects (string keys, optional commit/date/value/corpus_sha)."""
    path = tmp_path / "sidecar.json"
    entries = {
        "proteon-sasa-vs-biopython-ci": LastVerifiedEntry(
            commit="4d6ddbec",
            date="2026-06-02",
            value=0.008,
            corpus_sha=None,
        ),
    }
    write(path, entries)
    raw = json.loads(path.read_text())
    # Shape that typed-trust's --last-verified-sidecar deserializes.
    assert isinstance(raw, dict)
    entry = raw["proteon-sasa-vs-biopython-ci"]
    assert entry["commit"] == "4d6ddbec"
    assert entry["date"] == "2026-06-02"
    assert entry["value"] == 0.008
    # corpus_sha is None, omitted from output by to_dict.
    assert "corpus_sha" not in entry
