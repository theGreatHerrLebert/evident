"""Phase 5 PR5: CLI + processor + dry-run tests.

Cover the end-to-end composition between the walker (repo.py), the
framing (PR4), the validator (PR4), and the render (PR4). The model
is mocked at the SDK boundary — tests inject a fake
``client.messages.create`` response shaped like Anthropic's SDK
output.

Codex v3 explicitly: tests must cover both the
'model emits clean output' path and the 'model emits bad tolerances
that the validator drops' path.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import json
import yaml

from evident_agent.extract import cli, repo


FIXTURES = Path(__file__).resolve().parent / "fixtures" / "extract" / "repo"


# ---------------------------------------------------------------------
# SDK-shape mocks
# ---------------------------------------------------------------------


@dataclass
class _FakeToolBlock:
    type: str
    name: str
    input: dict
    id: str = "block-1"


@dataclass
class _FakeMessage:
    content: list
    id: str = "msg-fake-1"


class _FakeMessages:
    def __init__(self, response: _FakeMessage):
        self.response = response
        self.last_request: dict | None = None

    def create(self, **kwargs):
        self.last_request = kwargs
        return self.response


class _FakeClient:
    def __init__(self, response: _FakeMessage):
        self.messages = _FakeMessages(response)


def _tool_response(claims: list[dict], rejections: list[dict] | None = None):
    block = _FakeToolBlock(
        type="tool_use",
        name="submit_extracted_claims",
        input={
            "claims": claims,
            "rejections": rejections or [],
        },
    )
    return _FakeMessage(content=[block])


# ---------------------------------------------------------------------
# dry-run
# ---------------------------------------------------------------------


def test_dry_run_writes_audit_outputs_and_no_manifest(tmp_path: Path):
    """Codex v1 P1: dry-run must NOT produce a manifest. Otherwise
    the output dir can be mistaken for a real negative extraction.
    """
    out = tmp_path / "out"
    result = cli.run_extract_repo(
        repo_path=FIXTURES / "clean_repo",
        output_dir=out,
        dry_run=True,
    )
    assert result is None
    assert (out / "EXTRACTION.md").is_file()
    assert (out / "dry_run.json").is_file()
    assert not (out / "evident.yaml").exists()


def test_dry_run_extraction_md_contains_no_model_call_notice(tmp_path: Path):
    out = tmp_path / "out"
    cli.run_extract_repo(
        repo_path=FIXTURES / "clean_repo",
        output_dir=out,
        dry_run=True,
    )
    md = (out / "EXTRACTION.md").read_text()
    assert "No model call was made" in md


def test_dry_run_json_is_structured_audit(tmp_path: Path):
    out = tmp_path / "out"
    cli.run_extract_repo(
        repo_path=FIXTURES / "cites_paper_repo",
        output_dir=out,
        dry_run=True,
    )
    payload = json.loads((out / "dry_run.json").read_text())
    assert payload["mode"] == "dry_run"
    assert payload["no_model_call_was_made"] is True
    # Redactions are recorded.
    kinds = {r["reason"] for r in payload["redactions"]}
    assert "bibliography" in kinds


# ---------------------------------------------------------------------
# Response processor — the validator's hook
# ---------------------------------------------------------------------


def _good_clean_claim() -> dict:
    return {
        "id": "clean-repo-throughput",
        "title": "Throughput claim",
        "claim": "Our system sustains throughput > 1000 req/sec.",
        "subject_aliases": ["our system", "we"],
        "tolerances": [
            {
                "metric": "throughput",
                "op": ">",
                "value": 1000,
                "source_span": (
                    "Our system sustains throughput greater than "
                    "1000 requests per second on the production "
                    "cluster"
                ),
                "prose": "stated bound 1000 req/sec",
            }
        ],
    }


def _invented_threshold_claim() -> dict:
    """A claim where the model invented a bound the source never
    stated. The validator must reject it."""
    return {
        "id": "invented-claim",
        "title": "Invented threshold",
        "claim": "Marketing said blazing-fast.",
        "subject_aliases": ["our system"],
        "tolerances": [
            {
                "metric": "latency",
                "op": "<",
                "value": 0.5,
                # No mention of latency or 0.5 in the source_span.
                "source_span": "Used by Fortune 500 companies.",
                "prose": "best-in-class",
            }
        ],
    }


def test_process_tool_response_keeps_valid_claims_and_rejects_invented():
    walked = repo.walk_repo(FIXTURES / "clean_repo")
    response_input = {
        "claims": [_good_clean_claim(), _invented_threshold_claim()],
        "rejections": [],
    }
    result = cli.process_tool_response(
        response_input,
        walked,
        extractor_model="claude-opus-4-7",
        extracted_at="2026-09-14T10:00:00Z",
    )
    accepted_ids = [c.id for c in result.claims]
    assert accepted_ids == ["clean-repo-throughput"]
    # The invented claim's tolerance is in rejections.
    assert any(
        r.locator == "invented-claim" for r in result.rejections
    )


def test_process_tool_response_drops_claim_with_zero_valid_tolerances():
    """Codex v3: if all tolerances on a claim fail validation, the
    claim is dropped entirely."""
    walked = repo.walk_repo(FIXTURES / "clean_repo")
    response_input = {
        "claims": [_invented_threshold_claim()],
        "rejections": [],
    }
    result = cli.process_tool_response(
        response_input,
        walked,
        extractor_model="claude-opus-4-7",
        extracted_at="2026-09-14T10:00:00Z",
    )
    assert result.claims == []
    assert any(
        r.locator == "invented-claim" for r in result.rejections
    )


def test_process_tool_response_passes_through_model_rejections():
    walked = repo.walk_repo(FIXTURES / "clean_repo")
    response_input = {
        "claims": [],
        "rejections": [
            {
                "candidate_text": "blazing-fast",
                "locator": "README.md L3",
                "reason": "ranking_language_only",
                "rationale": "no bound stated",
            }
        ],
    }
    result = cli.process_tool_response(
        response_input,
        walked,
        extractor_model="claude-opus-4-7",
        extracted_at="2026-09-14T10:00:00Z",
    )
    assert result.claims == []
    assert len(result.rejections) == 1
    assert result.rejections[0].reason == "ranking_language_only"


# ---------------------------------------------------------------------
# End-to-end with the mocked client
# ---------------------------------------------------------------------


def test_end_to_end_clean_repo_yields_one_claim(tmp_path: Path):
    out = tmp_path / "out"
    response = _tool_response(claims=[_good_clean_claim()])
    client = _FakeClient(response)
    result = cli.run_extract_repo(
        repo_path=FIXTURES / "clean_repo",
        output_dir=out,
        api_client=client,
        extracted_at="2026-09-14T10:00:00Z",
    )
    assert result is not None
    assert len(result.claims) == 1
    manifest = yaml.safe_load((out / "evident.yaml").read_text())
    assert len(manifest["claims"]) == 1


def test_end_to_end_marketing_repo_model_emits_zero_claims(tmp_path: Path):
    """Path (a): model gets it right and emits an empty result."""
    out = tmp_path / "out"
    response = _tool_response(
        claims=[],
        rejections=[
            {
                "candidate_text": "blazing-fast",
                "locator": "README.md",
                "reason": "ranking_language_only",
                "rationale": "ranking language",
            }
        ],
    )
    client = _FakeClient(response)
    result = cli.run_extract_repo(
        repo_path=FIXTURES / "marketing_repo",
        output_dir=out,
        api_client=client,
        extracted_at="2026-09-14T10:00:00Z",
    )
    assert result.claims == []
    manifest = yaml.safe_load((out / "evident.yaml").read_text())
    assert manifest["claims"] == []
    # EXTRACTION.md records the model rejection.
    extraction_md = (out / "EXTRACTION.md").read_text()
    assert "ranking_language_only" in extraction_md


def test_end_to_end_marketing_repo_validator_safety_net(tmp_path: Path):
    """Path (b): model emits two marketing-language tolerances the
    validator drops. The manifest is empty AND the EXTRACTION.md
    records the validator rejections.

    Codex v3 explicit: this is the load-bearing test that the
    validator behaves as the safety net when the model misbehaves.
    """
    out = tmp_path / "out"
    bad_claim_1 = {
        "id": "blaze-1",
        "title": "blazing-fast claim",
        "claim": "BlazeStack is blazing-fast.",
        "subject_aliases": ["our system", "blazestack"],
        "tolerances": [
            {
                "metric": "speed",
                "op": ">",
                "value": 9999,
                # source_span has neither the metric nor the value.
                "source_span": "BlazeStack is the blazing-fast solution",
                "prose": "marketing",
            }
        ],
    }
    bad_claim_2 = {
        "id": "blaze-2",
        "title": "best-in-class claim",
        "claim": "BlazeStack is best-in-class.",
        "subject_aliases": ["our system", "blazestack"],
        "tolerances": [
            {
                "metric": "quality",
                "op": ">",
                "value": 100,
                "source_span": "Industry-leading reliability",
                "prose": "marketing",
            }
        ],
    }
    response = _tool_response(claims=[bad_claim_1, bad_claim_2])
    client = _FakeClient(response)
    result = cli.run_extract_repo(
        repo_path=FIXTURES / "marketing_repo",
        output_dir=out,
        api_client=client,
        extracted_at="2026-09-14T10:00:00Z",
    )
    assert result.claims == []
    manifest = yaml.safe_load((out / "evident.yaml").read_text())
    assert manifest["claims"] == []
    # Both bad tolerances appear in the rejections list.
    locators = {r.locator for r in result.rejections}
    assert "blaze-1" in locators
    assert "blaze-2" in locators


def test_end_to_end_conflict_repo_emits_two_claims_with_distinct_spans(
    tmp_path: Path,
):
    """Codex v3 concrete shape: README claim and CHANGELOG claim get
    DIFFERENT ids and DIFFERENT source_spans. PR5 does NOT pick
    authority."""
    out = tmp_path / "out"
    readme_claim = {
        "id": "conflict-repo-throughput-readme",
        "title": "Throughput per README",
        "claim": "throughput > 1000 req/sec per the README",
        "subject_aliases": ["our system"],
        "tolerances": [
            {
                "metric": "throughput",
                "op": ">",
                "value": 1000,
                "source_span": (
                    "Our system sustains throughput greater than "
                    "1000 requests per second on the production cluster"
                ),
                "prose": "README v0.1 bound",
            }
        ],
    }
    changelog_claim = {
        "id": "conflict-repo-throughput-changelog",
        "title": "Throughput per CHANGELOG v0.2",
        "claim": "throughput > 5000 req/sec per CHANGELOG v0.2",
        "subject_aliases": ["our system"],
        "tolerances": [
            {
                "metric": "throughput",
                "op": ">",
                "value": 5000,
                "source_span": (
                    "Our system now sustains throughput greater than "
                    "5000 requests per second on the production cluster"
                ),
                "prose": "CHANGELOG v0.2 bound",
            }
        ],
    }
    response = _tool_response(claims=[readme_claim, changelog_claim])
    client = _FakeClient(response)
    result = cli.run_extract_repo(
        repo_path=FIXTURES / "conflict_repo",
        output_dir=out,
        api_client=client,
        extracted_at="2026-09-14T10:00:00Z",
    )
    assert len(result.claims) == 2
    ids = {c.id for c in result.claims}
    assert ids == {
        "conflict-repo-throughput-readme",
        "conflict-repo-throughput-changelog",
    }
    spans = {c.tolerances[0]["source_span"] for c in result.claims}
    assert len(spans) == 2  # distinct


# ---------------------------------------------------------------------
# Mocking pattern: framing.build_request stays unmocked
# ---------------------------------------------------------------------


def test_run_extract_repo_passes_redacted_text_to_model(tmp_path: Path):
    """The walker's redactions must travel into the model's input
    text. Verify by inspecting what the mock client received."""
    out = tmp_path / "out"
    response = _tool_response(claims=[])
    client = _FakeClient(response)
    cli.run_extract_repo(
        repo_path=FIXTURES / "cites_paper_repo",
        output_dir=out,
        api_client=client,
        extracted_at="2026-09-14T10:00:00Z",
    )
    request = client.messages.last_request
    assert request is not None
    user_msg = request["messages"][0]["content"]
    # Neither the DOI nor the bibliography text reaches the model.
    assert "10.1234/jis.2024.012" not in user_msg
    assert "Doe and Lee, 2023" not in user_msg
    assert "external reference omitted" in user_msg
