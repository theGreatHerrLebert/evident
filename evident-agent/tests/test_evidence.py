"""Per-format evidence digest extractor tests."""

from __future__ import annotations

import json
from pathlib import Path

from evident_agent.evidence import (
    Digest,
    MAX_BODY_BYTES,
    extract_csv,
    extract_fallback,
    extract_json,
    extract_pytest_or_log,
    make_digest,
)


# ---------- JSON ----------

def test_extract_json_with_present_metric(tmp_path: Path) -> None:
    artifact = tmp_path / "out.json"
    artifact.write_text(json.dumps({"summary": {"runs": 1000}, "relative_error": 0.008}))
    d = extract_json(artifact, "relative_error")
    assert d.header["format"] == "json"
    assert d.header["metric_present"] == "pass"
    assert "0.008" in d.body
    assert not d.truncated


def test_extract_json_metric_absent_marks_fail(tmp_path: Path) -> None:
    artifact = tmp_path / "out.json"
    artifact.write_text(json.dumps({"unrelated": 42}))
    d = extract_json(artifact, "relative_error")
    assert d.header["metric_present"] == "fail"


def test_extract_json_unparseable_returns_truncated_text(tmp_path: Path) -> None:
    artifact = tmp_path / "out.json"
    artifact.write_text("this is not JSON {{{")
    d = extract_json(artifact, "relative_error")
    assert d.header["metric_present"] == "unknown"
    assert d.header.get("parse_error")
    assert "this is not JSON" in d.body


def test_extract_jsonl_with_metric_summary(tmp_path: Path) -> None:
    artifact = tmp_path / "out.jsonl"
    artifact.write_text(
        "\n".join(json.dumps({"id": i, "relative_error": 0.007 + i * 0.0001}) for i in range(50))
    )
    d = extract_json(artifact, "relative_error")
    assert d.header["format"] == "jsonl"
    # Summary block must include count, min/max so the model can see
    # tolerance behavior across the population.
    body = d.body
    assert "relative_error_summary" in body or "relative_error" in body
    assert "min" in body
    assert "max" in body


# ---------- CSV ----------

def test_extract_csv_with_present_metric_column(tmp_path: Path) -> None:
    artifact = tmp_path / "out.csv"
    artifact.write_text(
        "id,relative_error\n1,0.007\n2,0.008\n3,0.011\n4,0.005\n"
    )
    d = extract_csv(artifact, "relative_error")
    assert d.header["format"] == "csv"
    assert d.header["metric_present"] == "pass"
    assert "relative_error" in d.body
    assert "max" in d.body


def test_extract_csv_with_absent_metric_column(tmp_path: Path) -> None:
    artifact = tmp_path / "out.csv"
    artifact.write_text("id,unrelated\n1,0.5\n")
    d = extract_csv(artifact, "relative_error")
    assert d.header["metric_present"] == "fail"


# ---------- pytest / log ----------

def test_extract_pytest_keeps_failure_lines_and_tail(tmp_path: Path) -> None:
    artifact = tmp_path / "pytest.log"
    lines = ["info: setup"] * 200 + ["FAILED test_outlier"] + ["info: running"] * 200 + [
        "===== 1 failed, 999 passed in 12s ====="
    ]
    artifact.write_text("\n".join(lines))
    d = extract_pytest_or_log(artifact, "relative_error")
    assert d.header["format"] == "log"
    # Failure line and final summary both kept.
    assert "FAILED" in d.body
    assert "1 failed" in d.body


def test_extract_pytest_finds_metric_match(tmp_path: Path) -> None:
    artifact = tmp_path / "pytest.log"
    artifact.write_text("computed relative_error=0.008 ok\n===== 1 passed in 5s =====")
    d = extract_pytest_or_log(artifact, "relative_error")
    assert d.header["metric_present"] == "pass"
    assert "relative_error" in d.body


# ---------- fallback ----------

def test_extract_fallback_unknown_format_carries_flag(tmp_path: Path) -> None:
    artifact = tmp_path / "out.bin"
    artifact.write_text("some opaque content with relative_error: 0.008 maybe")
    d = extract_fallback(artifact, "relative_error")
    assert d.header["format_unknown"] is True
    assert d.header["metric_present"] == "pass"


def test_extract_fallback_truncates_large_files(tmp_path: Path) -> None:
    artifact = tmp_path / "out.bin"
    artifact.write_text("x" * (MAX_BODY_BYTES + 1000))
    d = extract_fallback(artifact, None)
    assert d.truncated is True
    assert len(d.body.encode("utf-8")) <= MAX_BODY_BYTES


# ---------- dispatcher ----------

def test_make_digest_routes_by_extension(tmp_path: Path) -> None:
    j = tmp_path / "a.json"
    j.write_text(json.dumps({"relative_error": 0.01}))
    c = tmp_path / "b.csv"
    c.write_text("id,relative_error\n1,0.01\n")
    p = tmp_path / "c.log"
    p.write_text("relative_error=0.01\n1 passed in 1s")
    u = tmp_path / "d.unknown"
    u.write_text("relative_error=0.01")

    assert make_digest(j, "relative_error").header["format"] == "json"
    assert make_digest(c, "relative_error").header["format"] == "csv"
    assert make_digest(p, "relative_error").header["format"] == "log"
    assert make_digest(u, "relative_error").header["format"] == "unknown"


# ---------- digest rendering ----------

def test_digest_render_includes_header_and_body(tmp_path: Path) -> None:
    d = Digest(
        header={"format": "json", "source": "x.json", "metric_present": "pass"},
        body='{"relative_error": 0.008}',
        truncated=False,
    )
    rendered = d.render()
    assert "metric_present" in rendered
    assert "relative_error" in rendered


def test_missing_artifact_returns_missing_digest(tmp_path: Path) -> None:
    nope = tmp_path / "does-not-exist.json"
    d = extract_json(nope, "relative_error")
    assert d.header.get("missing") is True
    assert d.header["metric_present"] == "unknown"
