"""Prompt construction + tool schema round-trip tests."""

from __future__ import annotations

import json

from evident_agent.prompt import (
    SYSTEM_FRAMING,
    TOOL_DEFINITION,
    build_messages,
    build_request,
    build_user_message,
    schema_check_keys,
    schema_field_required,
)


def test_system_framing_defaults_to_dissent() -> None:
    assert "Default to **Dissent**" in SYSTEM_FRAMING
    # The six checks must all be named so the model knows what to verify.
    for token in (
        "exact metric",
        "tolerance comparator",
        "EVERY criterion",
        "truncated",
        "Outliers",
        "reproducible from the cited commit",
    ):
        assert token in SYSTEM_FRAMING


def test_tool_schema_has_required_fields() -> None:
    assert schema_field_required("verdict")
    assert schema_field_required("checks")
    assert schema_field_required("rationale")


def test_tool_schema_check_keys_match_six_check_design() -> None:
    keys = set(schema_check_keys())
    assert keys == {
        "metric_present",
        "within_tolerance",
        "outliers_checked",
        "reproducible_chain",
    }


def test_tool_schema_verdict_enum_is_endorse_dissent_only() -> None:
    enum = TOOL_DEFINITION["input_schema"]["properties"]["verdict"]["enum"]
    assert set(enum) == {"endorse", "dissent"}


def test_tool_schema_serializes_to_json() -> None:
    """The tool definition must be JSON-serializable for the Anthropic
    SDK. Catch any accidentally-non-serializable types."""
    s = json.dumps(TOOL_DEFINITION)
    assert "submit_review" in s


def test_build_user_message_inlines_claim_and_digest() -> None:
    claim_yaml = "id: claim-A\nkind: measurement\n"
    digest = "<digest header=\"{...}\">\ncontent\n</digest>"
    msg = build_user_message(claim_yaml, digest)
    assert "claim-A" in msg
    assert "<digest" in msg
    assert "submit_review" in msg


def test_build_messages_is_single_user_turn() -> None:
    msgs = build_messages("c", "d")
    assert len(msgs) == 1
    assert msgs[0]["role"] == "user"


def test_build_request_forces_tool_use_and_zero_temperature() -> None:
    req = build_request(model="claude-opus-4-7", claim_yaml="c", digest_rendered="d")
    assert req["model"] == "claude-opus-4-7"
    assert req["temperature"] == 0
    assert req["tool_choice"] == {"type": "tool", "name": "submit_review"}
    assert req["tools"][0] == TOOL_DEFINITION
    assert req["system"] == SYSTEM_FRAMING


def test_build_request_max_tokens_default_caps_rationale_length() -> None:
    """1024 tokens is plenty for a careful rationale; if the default
    drifts upward we'd start paying for essays."""
    req = build_request(model="claude-opus-4-7", claim_yaml="c", digest_rendered="d")
    assert req["max_tokens"] == 1024
