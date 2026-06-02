"""Phase 5 PR4: tests for the extract framing module.

Lighter-weight than the validator/render tests — the framing is
mostly a prompt + tool schema. We verify:

1. The tool schema is well-formed and accepts the expected fields.
2. The system prompt mentions the load-bearing prohibitions
   (so anyone editing it sees the test break when they remove
   protection).
3. ``build_request`` produces a complete Anthropic request dict.
"""

from __future__ import annotations

from evident_agent.extract import framing


def test_tool_schema_is_well_formed():
    schema = framing.TOOL_DEFINITION
    assert schema["name"] == "submit_extracted_claims"
    props = schema["input_schema"]["properties"]
    assert "claims" in props
    assert "rejections" in props


def test_tool_schema_claim_requires_subject_aliases_and_source_span():
    """Codex v3: these two fields are load-bearing for the validator;
    if the schema makes them optional, the model will omit them and
    every tolerance becomes unvalidatable.
    """
    schema = framing.TOOL_DEFINITION
    claim_schema = schema["input_schema"]["properties"]["claims"]["items"]
    assert "subject_aliases" in claim_schema["required"]
    tolerance_schema = claim_schema["properties"]["tolerances"]["items"]
    assert "source_span" in tolerance_schema["required"]


def test_rejection_reasons_enum_covers_codex_listed_modes():
    """Each rejection mode codex called out in the v3 plan must be
    in the enum, so the extractor can record it structurally."""
    rejection_schema = (
        framing.TOOL_DEFINITION["input_schema"]
        ["properties"]["rejections"]["items"]
    )
    reasons = rejection_schema["properties"]["reason"]["enum"]
    for needed in [
        "bound_not_stated",
        "comparator_bound_to_wrong_subject",
        "value_only_in_image_table",
        "metric_not_named",
        "ranking_language_only",
    ]:
        assert needed in reasons, f"missing rejection reason: {needed!r}"


def test_system_prompt_forbids_threshold_invention():
    """Anyone removing the 'do NOT infer a bound from a reported
    value' framing must break this test. The validator is the
    safety net, but the prompt reduces the bad-output rate at
    the source."""
    prompt = framing.SYSTEM_FRAMING.lower()
    assert "do not" in prompt or "must" in prompt
    assert "bound" in prompt
    assert "infer" in prompt or "invent" in prompt


def test_system_prompt_forbids_ranking_language_extraction():
    prompt = framing.SYSTEM_FRAMING.lower()
    assert "outperform" in prompt
    assert "ranking" in prompt or "state-of-the-art" in prompt


def test_system_prompt_calls_out_wrong_subject_binding():
    """The codex-flagged 'baseline error is below 0.5; our method
    reports 0.42' failure mode must be explicit in the prompt
    so the model can pattern-match it directly."""
    prompt = framing.SYSTEM_FRAMING.lower()
    assert "baseline" in prompt
    assert "wrong subject" in prompt or "bound to" in prompt


def test_build_request_returns_anthropic_shape():
    req = framing.build_request(
        source_text="a paper saying nothing in particular",
        source_id="arxiv:0000.00000",
        model="claude-opus-4-7",
    )
    assert req["model"] == "claude-opus-4-7"
    assert req["system"] == framing.SYSTEM_FRAMING
    assert req["tools"] == [framing.TOOL_DEFINITION]
    assert req["tool_choice"]["name"] == "submit_extracted_claims"
    assert req["messages"][0]["role"] == "user"


def test_build_user_message_includes_source_text_and_id():
    msg = framing.build_user_message(
        source_text="HELLO_PAPER_BODY", source_id="arxiv:9999.99999"
    )
    assert "HELLO_PAPER_BODY" in msg
    assert "arxiv:9999.99999" in msg
    assert "submit_extracted_claims" in msg
