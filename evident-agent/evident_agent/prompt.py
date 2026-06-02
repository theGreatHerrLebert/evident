"""Prompt construction + Anthropic tool-use schema for ``review``.

This module is the heart of Phase 2a. The plumbing around it
(API call, sidecar, CLI) is mechanical; what determines whether the
output is useful or noise is the framing here.

Key design choices:

- **Default-Dissent framing.** The model must verify, by direct
  citation, all of the conditions listed in ``SYSTEM_FRAMING`` before
  emitting Endorse. Otherwise: Dissent.

- **Structured submit_review schema.** The model commits to a
  per-check verdict (``metric_present``, ``within_tolerance``,
  ``outliers_checked``, ``reproducible_chain``) and an observed-value
  citation. This makes Endorse harder to fake: a vague-but-confident
  rationale no longer passes when ``observed_value`` is null or
  ``metric_present`` is unknown.

- **Adversarial role.** "Find reasons the evidence does NOT support
  the claim, before concluding it does." The opposite of an
  LLM-as-judge "is this correct" prompt, which yields yes-bias.

- **Truncation discipline.** When the digest header says
  ``truncated: true`` and the cited metric is absent, the model must
  Dissent. Caller-side validation rejects an Endorse that violates
  this.
"""

from __future__ import annotations

from typing import Any, Optional


SYSTEM_FRAMING = """You are an independent reviewer of a scientific claim.

Default to **Dissent** unless the supplied materials let you verify, by direct citation, ALL of the following:

1. The artifact contains the exact metric named in the claim's tolerance — not a metric with a similar name.
2. The observed value satisfies the tolerance comparator.
3. For multi-criterion claims, EVERY criterion is supported by evidence in the digest. If any criterion is unsupported, missing, ambiguous, or contradicted, Dissent and identify that criterion by name in your failure_reason.
4. The digest is not truncated past the relevant content. If the digest header says `truncated: true` and the cited metric value is not in the digest, Dissent.
5. Outliers within the digest do not contradict the headline value.
6. The evidence chain is reproducible from the cited commit (the digest header reports the commit and command).

Do NOT infer missing evidence. Do NOT treat a successful command exit, a headline summary, or the claim's own prose as proof — only the digest content counts. If any of your `checks` is `fail` or `unknown`, your verdict MUST be `dissent`."""


TOOL_DEFINITION: dict[str, Any] = {
    "name": "submit_review",
    "description": (
        "Submit your reviewer verdict on the cited claim. Endorse only "
        "if you can cite the verifying evidence by row/field/value; "
        "Dissent otherwise."
    ),
    "input_schema": {
        "type": "object",
        "properties": {
            "verdict": {
                "type": "string",
                "enum": ["endorse", "dissent"],
            },
            "checks": {
                "type": "object",
                "properties": {
                    "metric_present": {"type": "string", "enum": ["pass", "fail", "unknown"]},
                    "within_tolerance": {"type": "string", "enum": ["pass", "fail", "unknown"]},
                    "outliers_checked": {"type": "string", "enum": ["pass", "fail", "unknown"]},
                    "reproducible_chain": {"type": "string", "enum": ["pass", "fail", "unknown"]},
                },
                "required": [
                    "metric_present",
                    "within_tolerance",
                    "outliers_checked",
                    "reproducible_chain",
                ],
            },
            "observed_value": {"type": ["string", "null"]},
            "tolerance": {"type": ["string", "null"]},
            "failure_reason": {"type": ["string", "null"]},
            "rationale": {
                "type": "string",
                "description": (
                    "Specific, evidence-cited reasoning. Reference fields, "
                    "rows, observed values from the digest. Avoid "
                    "generalities. Minimum 50 characters."
                ),
            },
        },
        "required": ["verdict", "checks", "rationale"],
    },
}


def build_user_message(claim_yaml: str, digest_rendered: str) -> str:
    """Assemble the user-turn content for a single submit_review call.

    Caller passes the claim YAML (as authored — all criteria visible)
    and the rendered digest from ``evidence.make_digest(...).render()``.
    """
    return (
        "## Claim under review\n\n"
        "```yaml\n"
        f"{claim_yaml.strip()}\n"
        "```\n\n"
        "## Evidence digest\n\n"
        f"{digest_rendered}\n\n"
        "## Your task\n\n"
        "Read the claim and the digest. Apply the six checks from the "
        "framing. Call the `submit_review` tool with your verdict, "
        "per-check verdicts, and a rationale that cites specific "
        "values from the digest. Default to Dissent if any check is "
        "fail or unknown."
    )


def build_messages(claim_yaml: str, digest_rendered: str) -> list[dict[str, Any]]:
    """Build the Anthropic ``messages`` parameter for one review call."""
    return [
        {"role": "user", "content": build_user_message(claim_yaml, digest_rendered)},
    ]


def build_request(
    *,
    model: str,
    claim_yaml: str,
    digest_rendered: str,
    max_tokens: int = 1024,
) -> dict[str, Any]:
    """Build the full Anthropic ``messages.create`` keyword payload.

    Forces tool use via ``tool_choice``; sets ``temperature=0`` so the
    only non-determinism is server-side. ``max_tokens`` caps the
    rationale length — 1024 is enough for a careful citation, not
    enough for an essay.
    """
    return {
        "model": model,
        "max_tokens": max_tokens,
        "temperature": 0,
        "system": SYSTEM_FRAMING,
        "tools": [TOOL_DEFINITION],
        "tool_choice": {"type": "tool", "name": "submit_review"},
        "messages": build_messages(claim_yaml, digest_rendered),
    }


def schema_field_required(field: str) -> bool:
    """Tiny helper for tests — return True iff ``field`` is required
    by the submit_review schema."""
    return field in TOOL_DEFINITION["input_schema"]["required"]


def schema_check_keys() -> list[str]:
    """The list of per-check keys in the submit_review schema. Tests
    assert these match what review.py enforces."""
    return list(
        TOOL_DEFINITION["input_schema"]["properties"]["checks"]["properties"].keys()
    )
