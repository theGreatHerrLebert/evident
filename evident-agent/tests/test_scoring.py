"""Tests for artifact scoring (fallback path).

Proteon's claim_scoring is exercised in the e2e test when available;
here we test the minimal fallback for JSON artifacts.
"""

from __future__ import annotations

import json
from pathlib import Path

from evident_agent.scoring import extract_primary_observation


def test_fallback_extracts_value_key(tmp_path: Path) -> None:
    artifact = tmp_path / "result.json"
    artifact.write_text(json.dumps({"value": 0.0017, "other": "data"}))
    claim = {
        "id": "test-claim",
        "evidence": {"artifact": "result.json"},
    }
    obs = extract_primary_observation(claim, tmp_path)
    assert obs == 0.0017


def test_fallback_extracts_observed_key(tmp_path: Path) -> None:
    artifact = tmp_path / "result.json"
    artifact.write_text(json.dumps({"observed": 0.005}))
    claim = {
        "id": "test-claim",
        "evidence": {"artifact": "result.json"},
    }
    assert extract_primary_observation(claim, tmp_path) == 0.005


def test_fallback_handles_missing_artifact(tmp_path: Path) -> None:
    claim = {
        "id": "test-claim",
        "evidence": {"artifact": "nonexistent.json"},
    }
    assert extract_primary_observation(claim, tmp_path) is None


def test_fallback_handles_no_evidence_block() -> None:
    claim = {"id": "test-claim"}
    assert extract_primary_observation(claim, Path("/tmp")) is None


def test_fallback_handles_archived_artifact_string(tmp_path: Path) -> None:
    """Manifests sometimes write '<path> (archived in <archive>)' — we
    should take the first whitespace-separated token as the path."""
    artifact = tmp_path / "results.json"
    artifact.write_text(json.dumps({"value": 0.02}))
    claim = {
        "id": "test-claim",
        "evidence": {
            "artifact": "results.json (archived in v0.2.0-evidence.tar.gz release asset)"
        },
    }
    assert extract_primary_observation(claim, tmp_path) == 0.02
