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

Do NOT infer missing evidence. Do NOT treat a successful command exit, a headline summary, or the claim's own prose as proof — only the digest content counts. If any of your `checks` is `fail` or `unknown`, your verdict MUST be `dissent`.

Escalate from Dissent to **Challenge** only when the digest contains a specific observed value that violates one of the target claim's stated tolerances. Report the violation as `{ target_criterion_id, metric, observed_value, bound, comparator, citation }` using the target's own metric name, bound, and comparator. The agent — not you — constructs the backing claim from your report. Do NOT propose new metrics, looser bounds, or trivial predicates like `observed > 0`, `row exists`, or `value is numeric` as a way to escalate. If you cannot cite a specific row/field/value that contradicts the target tolerance with its own bound, stay with Dissent.

Multi-criterion target: name which criterion you are contradicting via `target_criterion_id`. If you cannot identify a specific criterion, that is Dissent, not Challenge.

Negative examples of invalid Challenge violations (each one must remain Dissent):
- target says `error < 0.02`; you report `bound: 0.0, comparator: ">"` — trivial threshold drift.
- target says `error < 0.02`; you report `metric: "rmsd"` — metric drift.
- target says `error < 0.02`; you report `observed_value: "0.025"` — stringified, not numeric.
- target says `error < 0.02`; you report `observed_value: 0.008` — satisfies the tolerance, not a contradiction.

Positive example: target says `electrostatic_error < 0.02`; digest row 47 shows `electrostatic_error = 0.025`. Valid violation: `{metric: "electrostatic_error", observed_value: 0.025, bound: 0.02, comparator: "<", citation: "row 47 of bench/electrostatic_results.csv"}`."""


PROCEDURAL_CATEGORIES = (
    "command_failure",
    "hash_mismatch",
    "artifact_unavailable",
    "conflict_of_interest",
    "peer_review_unverifiable",
)

SUBSTANTIVE_CATEGORIES = (
    "missing_control",
    "weak_statistics",
    "confound",
    "unverifiable_assumption",
    "missing_benchmark",
    "reproducibility_risk",
)

CHALLENGE_CATEGORIES = SUBSTANTIVE_CATEGORIES + PROCEDURAL_CATEGORIES


TOOL_DEFINITION: dict[str, Any] = {
    "name": "submit_review",
    "description": (
        "Submit your reviewer verdict on the cited claim. Endorse only "
        "if you can cite the verifying evidence by row/field/value; "
        "Dissent if the evidence is insufficient; Challenge only if you "
        "can cite a specific value that violates the target tolerance "
        "with its own bound and comparator."
    ),
    "input_schema": {
        "type": "object",
        "properties": {
            "verdict": {
                "type": "string",
                "enum": ["endorse", "dissent", "challenge"],
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
            "challenge": {
                "type": ["object", "null"],
                "description": (
                    "Required when verdict == \"challenge\". For "
                    "substantive categories, supply a violation tuple. "
                    "For procedural categories, supply only the "
                    "category. The agent constructs the backing claim "
                    "from your violation; do NOT author the backing "
                    "claim's tolerance directly."
                ),
                "properties": {
                    "category": {
                        "type": "string",
                        "enum": list(CHALLENGE_CATEGORIES),
                    },
                    "target_criterion_id": {
                        "type": ["string", "null"],
                        "description": (
                            "The id (metric name) of the target's "
                            "criterion you are contradicting. Required "
                            "for substantive categories."
                        ),
                    },
                    "violation": {
                        "type": ["object", "null"],
                        "description": (
                            "Required for substantive categories. "
                            "Report the target's own metric, bound, "
                            "and comparator — not your inverse."
                        ),
                        "properties": {
                            "metric": {"type": "string"},
                            "observed_value": {"type": "number"},
                            "bound": {"type": "number"},
                            "comparator": {
                                "type": "string",
                                "enum": ["<", "<=", ">", ">="],
                            },
                            "citation": {"type": "string"},
                        },
                        "required": [
                            "metric",
                            "observed_value",
                            "bound",
                            "comparator",
                            "citation",
                        ],
                    },
                },
                "required": ["category"],
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
