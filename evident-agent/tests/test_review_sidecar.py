"""Tests for the append-only review_events.json sidecar.

Covers: append-only semantics, canonical event_id stability,
canonical event_id parity with typed-trust (round-tripped through the
binary), atomic writes, concurrent appends do not lose entries.
"""

from __future__ import annotations

import json
import os
import multiprocessing
import subprocess
from pathlib import Path

import pytest

from evident_agent.review_sidecar import (
    ReviewAuthor,
    ReviewEventEntry,
    append_events,
    canonical_event_id,
    read_events,
)


def _endorse_entry(claim_id: str = "claim-A", rationale: str = "ok") -> ReviewEventEntry:
    return ReviewEventEntry(
        claim_id=claim_id,
        kind="endorse",
        author=ReviewAuthor(
            kind="model",
            name="claude-opus-4-7",
            version="20250101",
            context="evident-agent review v0.2a",
        ),
        rationale=rationale,
        timestamp="2026-06-02T10:31:44Z",
        checks={
            "metric_present": "pass",
            "within_tolerance": "pass",
            "outliers_checked": "pass",
            "reproducible_chain": "pass",
        },
        observed_value="0.008",
        tolerance="< 0.02",
    )


def test_read_events_missing_file_returns_empty(tmp_path: Path) -> None:
    assert read_events(tmp_path / "nope.json") == []


def test_append_creates_sidecar_with_canonical_event_id(tmp_path: Path) -> None:
    path = tmp_path / "review_events.json"
    entry = _endorse_entry()
    assert entry.event_id is None
    merged = append_events(path, [entry])
    assert len(merged) == 1
    assert merged[0].event_id is not None
    assert merged[0].event_id.startswith("sha256:")
    # On-disk shape matches typed-trust's sidecar wrapper.
    disk = json.loads(path.read_text())
    assert "events" in disk
    assert disk["events"][0]["claim_id"] == "claim-A"
    assert disk["events"][0]["kind"] == "endorse"
    assert disk["events"][0]["event_id"] == merged[0].event_id


def test_canonical_event_id_is_stable(tmp_path: Path) -> None:
    a = _endorse_entry()
    b = _endorse_entry()
    assert canonical_event_id(a) == canonical_event_id(b)


def test_canonical_event_id_changes_on_payload_diff(tmp_path: Path) -> None:
    a = _endorse_entry(rationale="ok")
    b = _endorse_entry(rationale="different rationale")
    assert canonical_event_id(a) != canonical_event_id(b)


def test_canonical_event_id_distinguishes_observed_value(tmp_path: Path) -> None:
    a = _endorse_entry()
    b = _endorse_entry()
    b.observed_value = "0.009"
    assert canonical_event_id(a) != canonical_event_id(b)


def test_append_preserves_existing_entries(tmp_path: Path) -> None:
    path = tmp_path / "review_events.json"
    append_events(path, [_endorse_entry(claim_id="claim-A")])
    append_events(path, [_endorse_entry(claim_id="claim-B")])
    events = read_events(path)
    assert {e.claim_id for e in events} == {"claim-A", "claim-B"}
    assert len(events) == 2


def test_append_is_atomic_no_partial_file(tmp_path: Path) -> None:
    """After a successful append, the file is fully valid JSON."""
    path = tmp_path / "review_events.json"
    append_events(path, [_endorse_entry()])
    # If the write were torn, this parse would fail.
    parsed = json.loads(path.read_text())
    assert "events" in parsed


def _worker_append(args):
    """Helper for the concurrent-write test. Module-level so it pickles
    cleanly for multiprocessing.Pool."""
    import sys
    sys.path.insert(0, "/scratch/TMAlign/evident/evident-agent")
    from evident_agent.review_sidecar import (
        ReviewAuthor,
        ReviewEventEntry,
        append_events,
    )
    path_str, claim_id = args
    entry = ReviewEventEntry(
        claim_id=claim_id,
        kind="endorse",
        author=ReviewAuthor(
            kind="model",
            name="claude-opus-4-7",
            version="20250101",
        ),
        rationale=f"rationale for {claim_id} long enough to pass downstream validation rules",
        timestamp="2026-06-02T10:31:44Z",
    )
    append_events(Path(path_str), [entry])


def test_concurrent_appends_do_not_lose_entries(tmp_path: Path) -> None:
    """Two processes appending to the same sidecar must both land.

    Without ``fcntl.flock``, the two processes could each observe
    the empty sidecar, each compute version-1, and each rename
    independently — losing one of the two appends. With the lock,
    one waits while the other runs.
    """
    path = tmp_path / "review_events.json"
    N = 8
    args = [(str(path), f"claim-{i:02d}") for i in range(N)]
    # Use spawn to ensure clean import on Linux (default is fork; either
    # works here, but spawn is more conservative).
    ctx = multiprocessing.get_context("spawn")
    with ctx.Pool(processes=N) as pool:
        pool.map(_worker_append, args)

    events = read_events(path)
    claim_ids = sorted(e.claim_id for e in events)
    assert claim_ids == sorted(f"claim-{i:02d}" for i in range(N))


def test_to_dict_omits_none_optionals(tmp_path: Path) -> None:
    entry = ReviewEventEntry(
        claim_id="claim-A",
        kind="endorse",
        author=ReviewAuthor(kind="model", name="x", version="v"),
        rationale="r",
        timestamp="t",
    )
    out = entry.to_dict()
    assert "checks" not in out
    assert "observed_value" not in out
    # version IS included because it's non-None.
    assert out["author"]["version"] == "v"


def _challenge_entry(claim_id: str = "ball-electrostatic-ci") -> ReviewEventEntry:
    return ReviewEventEntry(
        claim_id=claim_id,
        kind="challenge",
        author=ReviewAuthor(
            kind="model",
            name="claude-opus-4-7",
            version="20250101",
        ),
        rationale="Row 47 reports 0.025, exceeding the 0.02 bound on electrostatic_error.",
        timestamp="2026-06-02T10:31:44Z",
        challenge={
            "category": "weak_statistics",
            "target_criterion_id": "electrostatic_error",
            "violation": {
                "metric": "electrostatic_error",
                "observed_value": 0.025,
                "bound": 0.02,
                "comparator": "<",
                "citation": "row 47 of results.csv",
            },
            "backing_claim": {
                "id": f"{claim_id}-counter-abcd1234",
                "title": "counter",
                "kind": "measurement",
                "tier": "ci",
                "source": ".",
                "claim": "counter",
                "tolerances": [
                    {"metric": "electrostatic_error", "op": ">=", "value": 0.02, "prose": "x"}
                ],
                "evidence": {"oracle": ["BALL"], "command": "pytest", "artifact": "x"},
                "last_verified": {"date": "2026-06-02", "value": 0.025},
            },
        },
    )


def test_canonical_hash_includes_challenge_block(tmp_path: Path) -> None:
    """Codex F-2B: two Challenges by the same author at the same
    timestamp but with different violations must have distinct
    canonical event_ids."""
    a = _challenge_entry()
    b = _challenge_entry()
    b.challenge["violation"]["observed_value"] = 0.030
    assert canonical_event_id(a) != canonical_event_id(b)


def test_canonical_hash_excludes_backing_claim_id(tmp_path: Path) -> None:
    """The agent's deterministic backing-id derivation means changing
    only the backing id (without changing the violation) should NOT
    change the canonical hash — the backing carries no additional
    discriminating info."""
    a = _challenge_entry()
    b = _challenge_entry()
    b.challenge["backing_claim"]["id"] = "different-backing-id"
    # Backing id is excluded from the canonical projection in
    # _canonical_challenge.
    assert canonical_event_id(a) == canonical_event_id(b)


def test_challenge_entry_round_trips_through_sidecar(tmp_path: Path) -> None:
    path = tmp_path / "review_events.json"
    append_events(path, [_challenge_entry()])
    on_disk = json.loads(path.read_text())
    e = on_disk["events"][0]
    assert e["kind"] == "challenge"
    assert e["challenge"]["category"] == "weak_statistics"
    assert e["challenge"]["violation"]["observed_value"] == 0.025
    assert e["challenge"]["backing_claim"]["tolerances"][0]["op"] == ">="


def test_challenge_entry_canonical_event_id_matches_typed_trust(tmp_path: Path) -> None:
    """End-to-end parity: the canonical event_id Python computes for a
    Challenge entry matches what the typed-trust Rust binary computes
    when it reads the same sidecar."""
    binary = (
        Path(__file__).resolve().parents[2]
        / "typed-trust"
        / "target"
        / "debug"
        / "typed-trust"
    )
    if not binary.is_file():
        pytest.skip(f"typed-trust binary not built at {binary}")

    manifest = tmp_path / "evident.yaml"
    manifest.write_text(
        "version: 0.1\n"
        "project: test\n"
        "claims:\n"
        "  - id: ball-electrostatic-ci\n"
        "    kind: measurement\n"
        "    tier: ci\n"
        "    title: t\n"
        "    claim: c\n"
        "    tolerances:\n"
        "      - metric: electrostatic_error\n"
        "        op: \"<\"\n"
        "        value: 0.02\n"
        "        prose: stay under 2%\n"
        "    evidence:\n"
        "      oracle: [BALL]\n"
        "      command: \"true\"\n"
        "      artifact: out.json\n"
    )
    entry = _challenge_entry()
    sidecar = tmp_path / "review_events.json"
    append_events(sidecar, [entry])
    expected_id = canonical_event_id(entry)

    result = subprocess.run(
        [
            str(binary),
            "--format",
            "json",
            "--review-events-sidecar",
            str(sidecar),
            str(manifest),
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    bundle = json.loads(result.stdout)
    review_events = bundle["reports"][0]["_graph"]["review_events"]
    assert len(review_events) == 1
    assert review_events[0]["id"] == expected_id


def test_canonical_event_id_matches_typed_trust(tmp_path: Path) -> None:
    """Round-trip the sidecar through the typed-trust binary and
    confirm the canonical event_id Python computes matches what Rust
    computes for the same payload. The binary writes the id it
    computes back into the JSON output's `_graph.review_events[*].id`.
    """
    binary = (
        Path(__file__).resolve().parents[2]
        / "typed-trust"
        / "target"
        / "debug"
        / "typed-trust"
    )
    if not binary.is_file():
        pytest.skip(f"typed-trust binary not built at {binary}")

    # Minimal one-claim manifest.
    manifest = tmp_path / "evident.yaml"
    manifest.write_text(
        "version: 0.1\n"
        "project: test\n"
        "claims:\n"
        "  - id: claim-A\n"
        "    kind: measurement\n"
        "    tier: ci\n"
        "    title: t\n"
        "    claim: c\n"
        "    tolerances:\n"
        "      - metric: relative_error\n"
        "        op: \"<\"\n"
        "        value: 0.02\n"
        "        prose: stay under 2%\n"
        "    evidence:\n"
        "      oracle: [Test]\n"
        "      command: \"true\"\n"
        "      artifact: out.json\n"
    )
    entry = _endorse_entry(rationale="rationale that is plenty long enough to satisfy validation rules in 2a")
    sidecar = tmp_path / "review_events.json"
    append_events(sidecar, [entry])
    expected_id = canonical_event_id(entry)

    result = subprocess.run(
        [
            str(binary),
            "--format",
            "json",
            "--review-events-sidecar",
            str(sidecar),
            str(manifest),
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    bundle = json.loads(result.stdout)
    review_events = bundle["reports"][0]["_graph"]["review_events"]
    assert len(review_events) == 1
    rust_id = review_events[0]["id"]
    assert rust_id == expected_id, f"python={expected_id} rust={rust_id}"
