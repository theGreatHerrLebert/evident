"""Phase 5 PR4: Anthropic tool schema + system prompt for extraction.

The framing mirrors Phase 2a's anti-sycophancy pattern: default-deny,
structured tool schema, explicit refusal language. The model defaults
to "I cannot extract a structured claim from this sentence" unless
all four of metric / op / value / source_span are directly present
in the cited section.

The prompt explicitly forbids:

- inferring a bound from a reported value (``ours = 0.42`` → ``< 0.5``)
- inferring a numeric comparator from ranking language ("better than",
  "outperforms", "state-of-the-art")
- inventing a metric name not in the source

The validator (see ``validator.py``) is the load-bearing layer; this
prompt reduces the *rate* at which the model proposes bad tolerances
but the validator is what guarantees none slip through.
"""

from __future__ import annotations

from typing import Any


SYSTEM_FRAMING = """You are an extractor of structured empirical \
claims from a scientific paper or code repository.

**Default to "cannot extract"** unless the source directly states \
all of the following, in the SAME local context (the same sentence, \
the same table row, or the same table cell):

1. The exact metric being measured (e.g. ``median_rmsd``, \
``electrostatic_error``, ``throughput``).
2. A comparator the source actually uses (e.g. ``<``, ``less than``, \
``below``, ``at most``, ``no more than``; or the ``>`` family). \
Acceptable English phrasings: less than, less than or equal to, no \
more than, no greater than, at most, below, under, bounded by, \
bounded above by, does not exceed, up to, capped at; greater than, \
at least, no less than, above, exceeds, bounded below by, etc.
3. The bound's specific numeric value, exactly as the source states \
it (``0.5``, ``1000``, ``12.4%``).
4. The claimed subject of the measurement (the system being claimed \
about — e.g. "our method", "we", "ours", or the paper's named system).

If any of these is missing, **emit no tolerance for that claim**.

Specifically forbidden — these MUST be refused:

- Inferring a bound from a reported value. If the paper reports \
``ours = 0.42``, do NOT emit a tolerance such as ``< 0.5``. You may \
emit an equality observation only if the schema supports it.
- Inferring a numeric comparator from ranking language. "Better \
than", "outperforms", "improves over", "state-of-the-art", \
"significantly improves", "competitive with" never produce a numeric \
comparator unless the source directly states the comparator with \
both sides (e.g. ``ours 0.42 vs baseline 0.61`` or ``improves by \
12.4%``).
- Attributing a bound to the wrong subject. If the source says \
"baseline error is below 0.5; our method reports 0.42", do NOT emit \
``ours < 0.5`` — the comparator ``below 0.5`` is bound to the \
*baseline*, not to your subject.
- Extracting when the source's subject is ambiguous. If the source \
only says "the method" or "the system" and you cannot tie that \
phrase to the supplied artifact's named system, emit no tolerance.
- Extracting from a different paper / repo / artifact than the one \
supplied. Bibliography citations are out of scope.

Output discipline:

- Every tolerance you emit MUST carry a ``source_span``: a verbatim \
substring of the source text containing the metric, comparator, \
value, and subject. The validator checks this character-by-character.
- When you cannot extract a candidate, record it in your rejection \
list with a structured reason: ``bound_not_stated``, \
``comparator_bound_to_wrong_subject``, ``value_only_in_image_table``, \
``metric_not_named``, etc.

You will be evaluated on:

1. ZERO false-positive extractions (a tolerance the source doesn't \
support). One invented bound poisons the corpus's honesty guarantee.
2. Coverage of claims the source DOES state cleanly.

Optimise hard for (1). A paper with zero extracted claims is a \
valid output; a paper with one invented bound is a failed extraction \
even if it has nine real ones."""


TOOL_DEFINITION: dict[str, Any] = {
    "name": "submit_extracted_claims",
    "description": (
        "Submit the structured claims you extracted from the source, "
        "plus a list of rejected candidates with structured reasons. "
        "Default to fewer extractions: a paper with zero claims and "
        "honest rejection reasons is preferred over a paper with one "
        "invented bound."
    ),
    "input_schema": {
        "type": "object",
        "properties": {
            "claims": {
                "type": "array",
                "description": (
                    "Each accepted claim. Must carry every field "
                    "the validator requires; the validator will reject "
                    "the entry otherwise."
                ),
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": (
                                "Kebab-case identifier derived from "
                                "the source-id and the claim subject."
                            ),
                        },
                        "title": {"type": "string"},
                        "claim": {
                            "type": "string",
                            "description": (
                                "Prose statement of the claim, "
                                "verbatim or near-verbatim from the "
                                "source."
                            ),
                        },
                        "subject_aliases": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": (
                                "Strings the source uses to refer "
                                "to the claimed subject (\"ours\", "
                                "\"we\", \"the proposed method\", "
                                "named system, etc.). The validator "
                                "requires at least one alias to "
                                "appear in each tolerance's "
                                "source_span."
                            ),
                        },
                        "tolerances": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "metric": {"type": "string"},
                                    "op": {
                                        "type": "string",
                                        "enum": ["<", "<=", ">", ">="],
                                    },
                                    "value": {"type": "number"},
                                    "source_span": {
                                        "type": "string",
                                        "description": (
                                            "Verbatim substring of "
                                            "the source containing "
                                            "the metric, comparator, "
                                            "value, and subject in "
                                            "the same local context "
                                            "(same sentence, same "
                                            "table cell, or same "
                                            "table row)."
                                        ),
                                    },
                                    "prose": {
                                        "type": "string",
                                        "description": (
                                            "Plain-English summary "
                                            "of what the bound "
                                            "means for downstream "
                                            "reviewers."
                                        ),
                                    },
                                },
                                "required": [
                                    "metric",
                                    "op",
                                    "value",
                                    "source_span",
                                    "prose",
                                ],
                            },
                        },
                    },
                    "required": [
                        "id",
                        "title",
                        "claim",
                        "subject_aliases",
                        "tolerances",
                    ],
                },
            },
            "rejections": {
                "type": "array",
                "description": (
                    "Candidate sentences/spans that LOOKED claim-like "
                    "but were not extracted. Lets reviewers see what "
                    "the extractor decided not to commit to."
                ),
                "items": {
                    "type": "object",
                    "properties": {
                        "candidate_text": {"type": "string"},
                        "locator": {
                            "type": "string",
                            "description": (
                                "Section / page / line locator the "
                                "reviewer can use to find the "
                                "candidate in the source."
                            ),
                        },
                        "reason": {
                            "type": "string",
                            "enum": [
                                "bound_not_stated",
                                "comparator_bound_to_wrong_subject",
                                "value_only_in_image_table",
                                "metric_not_named",
                                "ranking_language_only",
                                "hedged_qualitative_only",
                                "cited_external_artifact",
                            ],
                        },
                        "rationale": {"type": "string"},
                    },
                    "required": [
                        "candidate_text",
                        "locator",
                        "reason",
                        "rationale",
                    ],
                },
            },
        },
        "required": ["claims", "rejections"],
    },
}


def build_user_message(source_text: str, source_id: str) -> str:
    """Assemble the user-turn content for a single extract call.

    ``source_text`` is the full text the model is extracting from
    (paper markdown, repo README, etc.). ``source_id`` is the
    canonical identifier (``arxiv:2501.12345`` /
    ``github:org/repo@sha``) that will appear in
    ``provenance.source_id``.
    """
    return (
        f"## Source ({source_id})\n\n"
        f"{source_text}\n\n"
        "## Your task\n\n"
        "Read the source and extract every structured empirical "
        "claim it directly states. Apply the default-deny rules from "
        "the system framing — if any of (metric, comparator, value, "
        "subject) is missing or attached to a different subject, "
        "emit no tolerance and record the candidate in your "
        "rejections list with a structured reason.\n\n"
        "Call the `submit_extracted_claims` tool with your accepted "
        "claims and rejections."
    )


def build_messages(source_text: str, source_id: str) -> list[dict[str, Any]]:
    """Build the message array for an Anthropic completion call."""
    return [
        {
            "role": "user",
            "content": build_user_message(source_text, source_id),
        }
    ]


def build_request(
    *,
    source_text: str,
    source_id: str,
    model: str,
    max_tokens: int = 4096,
) -> dict[str, Any]:
    """Build a complete Anthropic completion request dict for
    ``client.messages.create(**request)``.
    """
    return {
        "model": model,
        "max_tokens": max_tokens,
        "system": SYSTEM_FRAMING,
        "tools": [TOOL_DEFINITION],
        "tool_choice": {"type": "tool", "name": TOOL_DEFINITION["name"]},
        "messages": build_messages(source_text, source_id),
    }
