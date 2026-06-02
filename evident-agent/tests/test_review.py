"""Tests for review.call_review + semantic validation.

Uses a fake API client that returns scripted responses. No real
Anthropic call is made; the real-API run is the manual fixture
record step described in the plan's verification section.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional

import pytest

from evident_agent.review import (
    CHECK_KEYS,
    ReviewRejected,
    ReviewTransportError,
    ReviewVerdict,
    call_review,
    reject_if_hallucinated_criterion,
    verdict_to_sidecar_entry,
)


# ---------- Fake API client ----------

@dataclass
class FakeBlock:
    type: str
    name: Optional[str] = None
    input: Optional[dict[str, Any]] = None


@dataclass
class FakeResponse:
    id: str
    content: list[FakeBlock]


class FakeMessages:
    def __init__(self, responses: list[FakeResponse | Exception]):
        self.responses = list(responses)
        self.calls = 0

    def create(self, **_kwargs):
        self.calls += 1
        if not self.responses:
            raise AssertionError("FakeMessages: no more scripted responses")
        item = self.responses.pop(0)
        if isinstance(item, Exception):
            raise item
        return item


class FakeClient:
    def __init__(self, responses):
        self.messages = FakeMessages(responses)


def _ok_tool_input() -> dict[str, Any]:
    return {
        "verdict": "endorse",
        "checks": {k: "pass" for k in CHECK_KEYS},
        "observed_value": "0.008",
        "tolerance": "< 0.02",
        "rationale": "Digest shows relative_error 0.008 against tolerance < 0.02 with no outliers above bound.",
    }


def _ok_response() -> FakeResponse:
    return FakeResponse(
        id="msg_test",
        content=[FakeBlock(type="tool_use", name="submit_review", input=_ok_tool_input())],
    )


# ---------- Happy path ----------

def test_call_review_returns_validated_verdict() -> None:
    client = FakeClient([_ok_response()])
    v = call_review(
        model="claude-opus-4-7",
        claim_yaml="id: claim-A\n",
        digest_rendered="<digest></digest>",
        api_client=client,
    )
    assert v.verdict == "endorse"
    assert v.observed_value == "0.008"
    assert v.tolerance == "< 0.02"
    assert v.model == "claude-opus-4-7"
    assert v.request_id == "msg_test"
    assert client.messages.calls == 1


# ---------- Transport: tool not called ----------

def test_transport_error_when_tool_not_used_then_retry_succeeds() -> None:
    """First response has no tool_use block; retry succeeds."""
    bad = FakeResponse(id="msg_bad", content=[FakeBlock(type="text")])
    client = FakeClient([bad, _ok_response()])
    v = call_review(
        model="claude-opus-4-7",
        claim_yaml="x",
        digest_rendered="y",
        api_client=client,
    )
    assert v.verdict == "endorse"
    assert client.messages.calls == 2


def test_transport_error_after_one_retry_raises() -> None:
    bad = FakeResponse(id="msg_bad", content=[FakeBlock(type="text")])
    client = FakeClient([bad, bad])
    with pytest.raises(ReviewTransportError):
        call_review(
            model="claude-opus-4-7",
            claim_yaml="x",
            digest_rendered="y",
            api_client=client,
        )
    assert client.messages.calls == 2


def test_transport_error_when_tool_input_is_not_dict() -> None:
    bad = FakeResponse(
        id="msg_bad",
        content=[FakeBlock(type="tool_use", name="submit_review", input=None)],
    )
    client = FakeClient([bad, _ok_response()])
    v = call_review(
        model="claude-opus-4-7",
        claim_yaml="x",
        digest_rendered="y",
        api_client=client,
    )
    assert v.verdict == "endorse"  # retry succeeded
    assert client.messages.calls == 2


# ---------- Semantic rejections (no retry) ----------

def _resp_with(tool_input: dict[str, Any]) -> FakeResponse:
    return FakeResponse(
        id="msg_x",
        content=[FakeBlock(type="tool_use", name="submit_review", input=tool_input)],
    )


def test_rejects_invalid_verdict_enum() -> None:
    bad_input = _ok_tool_input()
    bad_input["verdict"] = "applaud"
    client = FakeClient([_resp_with(bad_input)])
    with pytest.raises(ReviewRejected, match="verdict"):
        call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)
    assert client.messages.calls == 1  # no retry on semantic failure


def test_rejects_missing_check_key() -> None:
    bad_input = _ok_tool_input()
    del bad_input["checks"]["outliers_checked"]
    client = FakeClient([_resp_with(bad_input)])
    with pytest.raises(ReviewRejected, match="outliers_checked"):
        call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)


def test_rejects_invalid_check_value() -> None:
    bad_input = _ok_tool_input()
    bad_input["checks"]["metric_present"] = "maybe"
    client = FakeClient([_resp_with(bad_input)])
    with pytest.raises(ReviewRejected, match="metric_present"):
        call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)


def test_rejects_endorse_with_failing_check() -> None:
    """The load-bearing rule: any check fail/unknown forces dissent."""
    bad_input = _ok_tool_input()
    bad_input["checks"]["within_tolerance"] = "fail"
    client = FakeClient([_resp_with(bad_input)])
    with pytest.raises(ReviewRejected, match="not 'pass'"):
        call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)


def test_rejects_endorse_with_unknown_check() -> None:
    bad_input = _ok_tool_input()
    bad_input["checks"]["reproducible_chain"] = "unknown"
    client = FakeClient([_resp_with(bad_input)])
    with pytest.raises(ReviewRejected):
        call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)


def test_rejects_short_rationale() -> None:
    bad_input = _ok_tool_input()
    bad_input["rationale"] = "too short"
    client = FakeClient([_resp_with(bad_input)])
    with pytest.raises(ReviewRejected, match=">= 50 characters"):
        call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)


def test_rejects_endorse_without_observed_value() -> None:
    bad_input = _ok_tool_input()
    bad_input["observed_value"] = None
    client = FakeClient([_resp_with(bad_input)])
    with pytest.raises(ReviewRejected, match="observed_value"):
        call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)


def test_rejects_endorse_without_tolerance() -> None:
    bad_input = _ok_tool_input()
    bad_input["tolerance"] = None
    client = FakeClient([_resp_with(bad_input)])
    with pytest.raises(ReviewRejected, match="tolerance"):
        call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)


def test_dissent_with_failure_reason_passes() -> None:
    """Dissent doesn't require observed_value / tolerance and may
    carry a failure_reason."""
    dissent_input = {
        "verdict": "dissent",
        "checks": {
            "metric_present": "pass",
            "within_tolerance": "fail",
            "outliers_checked": "pass",
            "reproducible_chain": "pass",
        },
        "observed_value": "0.021",
        "tolerance": "< 0.02",
        "failure_reason": "row 47 reports 0.021 which violates the tolerance",
        "rationale": "The digest at row 47 shows 0.021 exceeding the 0.02 tolerance bound.",
    }
    client = FakeClient([_resp_with(dissent_input)])
    v = call_review(model="m", claim_yaml="c", digest_rendered="d", api_client=client)
    assert v.verdict == "dissent"
    assert v.failure_reason
    assert v.checks["within_tolerance"] == "fail"


# ---------- Hallucinated criterion check ----------

def test_hallucinated_criterion_in_failure_reason_rejected() -> None:
    v = ReviewVerdict(
        verdict="dissent",
        checks={k: "pass" if k != "within_tolerance" else "fail" for k in CHECK_KEYS},
        rationale="x" * 60,
        observed_value="0.021",
        tolerance="< 0.02",
        failure_reason="criterion: `relative_error_fictional` is violated",
    )
    with pytest.raises(ReviewRejected, match="not in claim"):
        reject_if_hallucinated_criterion(v, claim_criteria=["relative_error"])


def test_real_criterion_in_failure_reason_accepted() -> None:
    v = ReviewVerdict(
        verdict="dissent",
        checks={k: "pass" if k != "within_tolerance" else "fail" for k in CHECK_KEYS},
        rationale="x" * 60,
        observed_value="0.021",
        tolerance="< 0.02",
        failure_reason="criterion `relative_error` is exceeded at row 47",
    )
    reject_if_hallucinated_criterion(v, claim_criteria=["relative_error"])  # no raise


def test_failure_reason_without_criterion_reference_passes() -> None:
    """A failure_reason that describes a defect without naming any
    criterion id is allowed — we only reject *hallucinated* names."""
    v = ReviewVerdict(
        verdict="dissent",
        checks={k: "pass" if k != "within_tolerance" else "fail" for k in CHECK_KEYS},
        rationale="x" * 60,
        observed_value="0.021",
        tolerance="< 0.02",
        failure_reason="value 0.021 exceeds the bound 0.02",
    )
    reject_if_hallucinated_criterion(v, claim_criteria=["relative_error"])


# ---------- Sidecar conversion ----------

def test_sdk_exception_is_mapped_to_review_transport_error_and_retried() -> None:
    """Codex F-CR2 regression: a real SDK exception (timeout,
    connection error, rate limit) must trigger the retry path and,
    on final failure, surface as a ReviewTransportError — not the
    raw SDK exception type.
    """

    class FakeAPIError(Exception):
        """Stands in for anthropic.APIError in tests; the
        ``_sdk_transport_exception_types`` helper falls back to ()
        when the SDK isn't installed, so we monkeypatch the catch
        list to include this stand-in.
        """

    import evident_agent.review as review_mod

    original = review_mod._sdk_transport_exception_types
    review_mod._sdk_transport_exception_types = lambda: (FakeAPIError,)
    try:
        client = FakeClient([FakeAPIError("network timeout"), _ok_response()])
        v = call_review(
            model="m", claim_yaml="c", digest_rendered="d", api_client=client
        )
        # Retry succeeded.
        assert v.verdict == "endorse"
        assert client.messages.calls == 2

        # Now both attempts fail; expect ReviewTransportError, not FakeAPIError.
        client2 = FakeClient([FakeAPIError("timeout 1"), FakeAPIError("timeout 2")])
        with pytest.raises(ReviewTransportError, match="FakeAPIError"):
            call_review(
                model="m", claim_yaml="c", digest_rendered="d", api_client=client2
            )
        assert client2.messages.calls == 2
    finally:
        review_mod._sdk_transport_exception_types = original


def test_verdict_to_sidecar_entry_preserves_fields() -> None:
    v = ReviewVerdict(
        verdict="endorse",
        checks={k: "pass" for k in CHECK_KEYS},
        rationale="x" * 60,
        observed_value="0.008",
        tolerance="< 0.02",
        model="claude-opus-4-7",
    )
    entry = verdict_to_sidecar_entry(
        v,
        claim_id="claim-A",
        author_name="claude-opus-4-7",
        author_version="20250101",
        author_context="evident-agent review v0.2a",
    )
    assert entry.kind == "endorse"
    assert entry.author.name == "claude-opus-4-7"
    assert entry.author.version == "20250101"
    assert entry.observed_value == "0.008"
    assert entry.checks == {k: "pass" for k in CHECK_KEYS}
