#!/usr/bin/env python3
"""Validate the EVIDENT manifest schema.

See workflow/SCHEMA.md for the full schema. This module checks structure
and vocabulary consistency, not scientific truth. Domain-specific claims
still need their own oracle or benchmark commands.
"""

from __future__ import annotations

import argparse
import math
import pathlib
import sys
from typing import Any

try:
    import yaml
except ImportError as exc:  # pragma: no cover - only hit in incomplete images
    raise SystemExit("Missing dependency: PyYAML") from exc


BASE_VOCABULARIES: dict[str, set[str]] = {
    "tolerance_metric": {
        "relative_error",
        "median_relative_error",
        "absolute_error",
        "pass_rate",
        "recall",
        "precision",
        "f1",
        "drift",
    },
    "tolerance_op": {"<", "<=", ">=", ">", "=="},
    "input_class": {
        "single-chain",
        "multi-chain",
        "random-sample",
        "synthetic",
        "fixture",
    },
    "subsystem": set(),
    "oracle": set(),
    "capability": set(),
}

PLACEHOLDER_PREFIXES = ("PENDING-", "TODO", "TBD")

VALID_TIERS = {"ci", "release", "research"}
VALID_TRUST_STRATEGIES = {"understanding", "validation", "proof"}
VALID_KINDS = {
    "implementation",
    "behavioral_concordance",
    "measurement",
    "metadata_compatibility",
    "performance",
    "pipeline",
    "policy",
    "reference",
    "release_gate",
    "scientific",
    "third_party_observation",
}
VALID_PROVENANCE = {
    "automatic",
    "human",
    "peer-reviewed",
    "author",
    "maintainer",
    "external",
    "generated",
    "ported",
    "third-party",
    "extracted-from-paper",
    "extracted-from-repo",
}
VALID_REVIEW_STATUS = {
    "automated",
    "human-reviewed",
    "externally-reviewed",
    "peer-reviewed",
}
VALID_REVIEWER_SOURCES = {"author", "maintainer", "external", "venue"}

REQUIRED_FIELDS_COMMON = {
    "id",
    "title",
    "tier",
    "claim",
}
REQUIRED_FIELDS_WORKFLOW = {
    "case",
    "source",
    "trust_strategy",
    "evidence",
    "assumptions",
    "failure_modes",
}
REQUIRED_FIELDS_MEASUREMENT = {
    "subsystem",
    "inputs",
    "pinned_versions",
    "tolerances",
}


def is_placeholder_string(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    stripped = value.strip().upper()
    return any(stripped.startswith(prefix) for prefix in PLACEHOLDER_PREFIXES)


def require_not_placeholder(value: Any, field: str, claim_id: str) -> None:
    if is_placeholder_string(value):
        fail(f"claim {claim_id}: {field} must not be a placeholder")


def fail(message: str) -> None:
    raise ValueError(message)


def require_non_empty_string(value: Any, field: str, claim_id: str) -> None:
    if not isinstance(value, str) or not value.strip():
        fail(f"claim {claim_id}: {field} must be a non-empty string")


def require_string_list(value: Any, field: str, claim_id: str) -> list[str]:
    if not isinstance(value, list) or not value:
        fail(f"claim {claim_id}: {field} must be a non-empty list")
    for item in value:
        if not isinstance(item, str) or not item.strip():
            fail(f"claim {claim_id}: {field} must contain only non-empty strings")
    return value


def validate_mapping(value: Any, field: str, claim_id: str) -> None:
    if not isinstance(value, dict) or not value:
        fail(f"claim {claim_id}: {field} must be a non-empty mapping")


def require_in_vocab(
    value: str,
    vocab_name: str,
    vocabularies: dict[str, set[str]],
    field: str,
    claim_id: str,
) -> None:
    vocab = vocabularies[vocab_name]
    if not vocab:
        fail(
            f"claim {claim_id}: vocabulary {vocab_name!r} is empty; "
            f"declare it in the manifest's vocabularies: block before using {field}"
        )
    if value not in vocab:
        fail(
            f"claim {claim_id}: {field} {value!r} not in vocabulary "
            f"{vocab_name}; allowed: {sorted(vocab)}"
        )


def validate_existing_path(
    root: pathlib.Path, value: Any, field: str, claim_id: str
) -> None:
    require_non_empty_string(value, field, claim_id)
    path = root / value
    if "#" in str(path):
        path = pathlib.Path(str(path).split("#", 1)[0])
    if not path.exists():
        fail(f"claim {claim_id}: {field} path does not exist: {value}")


def validate_evidence(
    value: Any,
    claim_id: str,
    vocabularies: dict[str, set[str]],
    *,
    require_oracle: bool = True,
    allow_unknown_vocab: bool = False,
) -> list[str]:
    if not isinstance(value, dict):
        fail(f"claim {claim_id}: evidence must be a mapping")
    required_fields = ["command", "artifact"]
    if require_oracle:
        required_fields.insert(0, "oracle")
    for field in required_fields:
        if field not in value:
            fail(f"claim {claim_id}: evidence.{field} is required")
    if "tolerance" in value:
        fail(
            f"claim {claim_id}: evidence.tolerance is no longer supported; "
            f"move tolerance text into the top-level tolerances: list "
            f"(see workflow/SCHEMA.md)"
        )
    if "oracle" not in value:
        oracles = []
    elif require_oracle:
        oracles = require_string_list(value["oracle"], "evidence.oracle", claim_id)
    else:
        raw_oracles = value["oracle"]
        if raw_oracles is None:
            oracles = []
        elif isinstance(raw_oracles, list):
            oracles = raw_oracles
            for item in oracles:
                if not isinstance(item, str) or not item.strip():
                    fail(
                        f"claim {claim_id}: evidence.oracle must contain only "
                        f"non-empty strings when present"
                    )
        else:
            fail(f"claim {claim_id}: evidence.oracle must be a list when present")
    for oracle_name in oracles:
        if not allow_unknown_vocab:
            require_in_vocab(
                oracle_name, "oracle", vocabularies, "evidence.oracle[]", claim_id
            )
    require_non_empty_string(value["command"], "evidence.command", claim_id)
    require_non_empty_string(value["artifact"], "evidence.artifact", claim_id)
    return oracles


def validate_tolerances(
    value: Any,
    claim_id: str,
    vocabularies: dict[str, set[str]],
    tier: str | None = None,
    allow_unknown_vocab: bool = False,
) -> None:
    if not isinstance(value, list) or not value:
        fail(f"claim {claim_id}: tolerances must be a non-empty list")
    for i, entry in enumerate(value):
        if not isinstance(entry, dict):
            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
        prose = entry.get("prose")
        if not isinstance(prose, str) or not prose.strip():
            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
        has_metric = "metric" in entry
        has_op = "op" in entry
        has_value = "value" in entry
        if tier in {"ci", "release"} and not all([has_metric, has_op, has_value]):
            fail(
                f"claim {claim_id}: tolerances[{i}] must contain metric, "
                f"op, and value for tier {tier}"
            )
        if any([has_metric, has_op, has_value]) and not all(
            [has_metric, has_op, has_value]
        ):
            fail(
                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
                f"all-or-nothing"
            )
        if has_metric:
            if not allow_unknown_vocab:
                require_in_vocab(
                    entry["metric"],
                    "tolerance_metric",
                    vocabularies,
                    f"tolerances[{i}].metric",
                    claim_id,
                )
                require_in_vocab(
                    entry["op"],
                    "tolerance_op",
                    vocabularies,
                    f"tolerances[{i}].op",
                    claim_id,
                )
            elif entry["op"] not in vocabularies["tolerance_op"]:
                fail(f"claim {claim_id}: tolerances[{i}].op is invalid: {entry['op']!r}")
            if not isinstance(entry["value"], (int, float)) or isinstance(
                entry["value"], bool
            ):
                fail(f"claim {claim_id}: tolerances[{i}].value must be numeric")


def validate_inputs(
    value: Any,
    claim_id: str,
    vocabularies: dict[str, set[str]],
    tier: str,
    *,
    strict_release_pins: bool = False,
) -> None:
    if not isinstance(value, dict):
        fail(f"claim {claim_id}: inputs must be a mapping")
    if "class" in value:
        require_in_vocab(
            value["class"], "input_class", vocabularies, "inputs.class", claim_id
        )
    if "classes" in value:
        for input_class in require_string_list(
            value["classes"], "inputs.classes", claim_id
        ):
            require_in_vocab(
                input_class,
                "input_class",
                vocabularies,
                "inputs.classes[]",
                claim_id,
            )
    n = value.get("n", 0) or 0
    if not isinstance(n, int):
        fail(f"claim {claim_id}: inputs.n must be an integer")
    if tier == "release" and n > 1 and not value.get("corpus_sha"):
        fail(
            f"claim {claim_id}: inputs.corpus_sha is required for "
            f"tier=release with n>1"
        )
    if strict_release_pins and tier == "release":
        require_not_placeholder(
            value.get("corpus_sha"), "inputs.corpus_sha", claim_id
        )


def validate_pinned_versions(
    value: Any,
    oracle_names: list[str],
    project: str,
    claim_id: str,
    *,
    tier: str | None = None,
    strict_release_pins: bool = False,
) -> None:
    if not isinstance(value, dict) or not value:
        fail(f"claim {claim_id}: pinned_versions must be a non-empty mapping")
    for k, v in value.items():
        if not isinstance(k, str) or not k.strip():
            fail(f"claim {claim_id}: pinned_versions keys must be non-empty strings")
        if not isinstance(v, str) or not v.strip():
            fail(
                f"claim {claim_id}: pinned_versions[{k!r}] must be a non-empty "
                f"string (quote numeric versions like \"1.83\")"
            )
        if strict_release_pins and tier == "release":
            require_not_placeholder(v, f"pinned_versions[{k!r}]", claim_id)
    if project not in value:
        fail(
            f"claim {claim_id}: pinned_versions must include the project under "
            f"test ({project!r})"
        )
    missing_oracles = [o for o in oracle_names if o not in value]
    if missing_oracles:
        fail(
            f"claim {claim_id}: pinned_versions must include every oracle "
            f"named in evidence.oracle; missing: {missing_oracles}"
        )


def validate_outputs(value: Any, claim_id: str) -> None:
    if isinstance(value, list):
        require_string_list(value, "outputs", claim_id)
        return
    if not isinstance(value, dict) or not value:
        fail(f"claim {claim_id}: outputs must be a non-empty mapping")
    for name, body in value.items():
        if not isinstance(name, str) or not name.strip():
            fail(f"claim {claim_id}: outputs keys must be non-empty strings")
        if not isinstance(body, dict):
            fail(f"claim {claim_id}: outputs[{name!r}] must be a mapping")


def validate_reviewers(value: Any, claim_id: str) -> list[dict[str, Any]]:
    if value is None:
        return []
    if not isinstance(value, list) or not value:
        fail(f"claim {claim_id}: reviewers must be a non-empty list")
    reviewers: list[dict[str, Any]] = []
    for i, entry in enumerate(value):
        if not isinstance(entry, dict):
            fail(f"claim {claim_id}: reviewers[{i}] must be a mapping")
        name = entry.get("name")
        if not isinstance(name, str) or not name.strip():
            fail(f"claim {claim_id}: reviewers[{i}].name is required")
        source = entry.get("source")
        if source is not None:
            require_non_empty_string(source, f"reviewers[{i}].source", claim_id)
            if source not in VALID_REVIEWER_SOURCES:
                fail(
                    f"claim {claim_id}: reviewers[{i}].source is invalid: "
                    f"{source!r}"
                )
        for opt in ("orcid", "affiliation", "date"):
            if opt in entry and entry[opt] is not None:
                if not isinstance(entry[opt], str) or not entry[opt].strip():
                    fail(
                        f"claim {claim_id}: reviewers[{i}].{opt} must be a "
                        f"non-empty string when present"
                    )
        unknown = set(entry) - {
            "name",
            "source",
            "orcid",
            "affiliation",
            "date",
        }
        if unknown:
            fail(
                f"claim {claim_id}: reviewers[{i}] has unknown keys: "
                f"{sorted(unknown)}"
            )
        reviewers.append(entry)
    return reviewers


def validate_provenance_and_reviewers(claim: dict, claim_id: str) -> None:
    provenance = claim.get("provenance", "automatic")
    if isinstance(provenance, str):
        provenance_values = [provenance]
    elif isinstance(provenance, dict):
        kind = provenance.get("kind")
        require_non_empty_string(kind, "provenance.kind", claim_id)
        provenance_values = [kind]
        allowed = {
            "kind",
            "source_id",
            "source_sha",
            "source_context",
            "extractor",
            "curator",
        }
        unknown = set(provenance) - allowed
        if unknown:
            fail(f"claim {claim_id}: provenance has unknown keys: {sorted(unknown)}")
        if "source_context" in provenance and provenance["source_context"] is not None:
            if provenance["source_context"] not in {
                "repo_authored",
                "copied_external_text",
                "unknown",
            }:
                fail(
                    f"claim {claim_id}: invalid provenance.source_context "
                    f"{provenance['source_context']!r}"
                )
        extractor = provenance.get("extractor")
        if extractor is not None:
            if not isinstance(extractor, dict):
                fail(f"claim {claim_id}: provenance.extractor must be a mapping")
            unknown_extractor = set(extractor) - {"model", "model_version", "extracted_at"}
            if unknown_extractor:
                fail(
                    f"claim {claim_id}: provenance.extractor has unknown keys: "
                    f"{sorted(unknown_extractor)}"
                )
            for key in ("model", "model_version", "extracted_at"):
                if key in extractor and extractor[key] is not None:
                    require_non_empty_string(
                        extractor[key], f"provenance.extractor.{key}", claim_id
                    )
    else:
        provenance_values = require_string_list(provenance, "provenance", claim_id)

    invalid = sorted(set(provenance_values) - VALID_PROVENANCE)
    if invalid:
        fail(f"claim {claim_id}: invalid provenance: {', '.join(invalid)}")

    review_status = claim.get("review_status")
    if review_status is not None:
        require_non_empty_string(review_status, "review_status", claim_id)
        if review_status not in VALID_REVIEW_STATUS:
            fail(f"claim {claim_id}: invalid review_status {review_status!r}")

    reviewers = validate_reviewers(claim.get("reviewers"), claim_id)
    needs_peer_reviewer = (
        "peer-reviewed" in provenance_values or review_status == "peer-reviewed"
    )
    if needs_peer_reviewer and not reviewers:
        fail(
            f"claim {claim_id}: peer-reviewed claims require a non-empty "
            f"reviewers list"
        )
    if review_status == "peer-reviewed":
        reviewer_sources = {reviewer.get("source") for reviewer in reviewers}
        if reviewer_sources and not reviewer_sources.intersection({"external", "venue"}):
            fail(
                f"claim {claim_id}: peer-reviewed status requires an external "
                f"or venue reviewer"
            )
    if not needs_peer_reviewer and reviewers:
        fail(
            f"claim {claim_id}: reviewers may only be set for peer-reviewed "
            f"provenance or review_status"
        )


def provenance_kind(claim: dict) -> str:
    provenance = claim.get("provenance", "automatic")
    if isinstance(provenance, str):
        return provenance
    if isinstance(provenance, dict) and isinstance(provenance.get("kind"), str):
        return provenance["kind"]
    return "automatic"


def validate_last_verified(
    value: Any,
    claim_id: str,
    *,
    tier: str | None = None,
    strict_release_pins: bool = False,
) -> None:
    if not isinstance(value, dict):
        fail(f"claim {claim_id}: last_verified must be a mapping")
    for key in ("commit", "date", "value", "corpus_sha"):
        if key not in value or value[key] is None:
            continue
        if key == "value":
            if not isinstance(value[key], (int, float)) or isinstance(
                value[key], bool
            ):
                fail(f"claim {claim_id}: last_verified.value must be numeric or null")
        elif not isinstance(value[key], str):
            fail(f"claim {claim_id}: last_verified.{key} must be a string or null")
    if strict_release_pins and tier == "release":
        for key in ("commit", "corpus_sha"):
            if key not in value or value[key] is None:
                fail(f"claim {claim_id}: last_verified.{key} is required")
            require_not_placeholder(value[key], f"last_verified.{key}", claim_id)


def validate_metadata_block(value: Any, claim_id: str) -> None:
    if not isinstance(value, dict):
        fail(f"claim {claim_id}: metadata must be a mapping")
    required = {"field", "declared_value", "source_file", "source_path"}
    missing = sorted(required - value.keys())
    if missing:
        fail(f"claim {claim_id}: metadata missing required fields: {', '.join(missing)}")
    unknown = set(value) - required
    if unknown:
        fail(f"claim {claim_id}: metadata has unknown keys: {sorted(unknown)}")
    require_non_empty_string(value["field"], "metadata.field", claim_id)
    require_non_empty_string(
        value["declared_value"], "metadata.declared_value", claim_id
    )
    require_non_empty_string(value["source_file"], "metadata.source_file", claim_id)
    require_non_empty_string(value["source_path"], "metadata.source_path", claim_id)


def _require_finite_number(value: Any, field: str, claim_id: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        fail(f"claim {claim_id}: {field} must be numeric")
    as_float = float(value)
    if not math.isfinite(as_float):
        fail(f"claim {claim_id}: {field} must be finite")
    return as_float


def _validate_pattern(value: Any, claim_id: str, *, observed_value: bool) -> None:
    if not isinstance(value, dict):
        fail(f"claim {claim_id}: pattern must be a mapping")
    kind = value.get("pattern_kind")
    require_non_empty_string(kind, "pattern.pattern_kind", claim_id)
    value_field = "observed_value" if observed_value else "prior_value"

    if kind == "numeric_band":
        required = {"pattern_kind", "metric_path", "epsilon", value_field}
        unknown = set(value) - required
        missing = sorted(required - value.keys())
        if missing:
            fail(f"claim {claim_id}: pattern missing required fields: {', '.join(missing)}")
        if unknown:
            fail(f"claim {claim_id}: pattern has unknown keys: {sorted(unknown)}")
        require_non_empty_string(value["metric_path"], "pattern.metric_path", claim_id)
        epsilon = _require_finite_number(value["epsilon"], "pattern.epsilon", claim_id)
        if epsilon <= 0.0:
            fail(f"claim {claim_id}: pattern.epsilon must be > 0")
        _require_finite_number(value[value_field], f"pattern.{value_field}", claim_id)
        return

    if kind == "relative_band":
        required = {"pattern_kind", "metric_path", "ratio", value_field}
        unknown = set(value) - required
        missing = sorted(required - value.keys())
        if missing:
            fail(f"claim {claim_id}: pattern missing required fields: {', '.join(missing)}")
        if unknown:
            fail(f"claim {claim_id}: pattern has unknown keys: {sorted(unknown)}")
        require_non_empty_string(value["metric_path"], "pattern.metric_path", claim_id)
        ratio = _require_finite_number(value["ratio"], "pattern.ratio", claim_id)
        if ratio <= 1.0:
            fail(f"claim {claim_id}: pattern.ratio must be > 1.0")
        _require_finite_number(value[value_field], f"pattern.{value_field}", claim_id)
        return

    if kind == "same_order_of_magnitude":
        required = {"pattern_kind", "metric_path", value_field}
        allowed = required | {"zero_policy"}
        unknown = set(value) - allowed
        missing = sorted(required - value.keys())
        if missing:
            fail(f"claim {claim_id}: pattern missing required fields: {', '.join(missing)}")
        if unknown:
            fail(f"claim {claim_id}: pattern has unknown keys: {sorted(unknown)}")
        require_non_empty_string(value["metric_path"], "pattern.metric_path", claim_id)
        prior = _require_finite_number(value[value_field], f"pattern.{value_field}", claim_id)
        if prior <= 0.0:
            fail(f"claim {claim_id}: pattern.{value_field} must be > 0")
        if value.get("zero_policy", "not_assessed") not in {"reject", "not_assessed"}:
            fail(f"claim {claim_id}: invalid pattern.zero_policy")
        return

    if kind == "ordinal_match":
        required = {"pattern_kind", "entity_to_path", "direction", value_field}
        allowed = required | {"tie_policy"}
        unknown = set(value) - allowed
        missing = sorted(required - value.keys())
        if missing:
            fail(f"claim {claim_id}: pattern missing required fields: {', '.join(missing)}")
        if unknown:
            fail(f"claim {claim_id}: pattern has unknown keys: {sorted(unknown)}")
        entity_to_path = value["entity_to_path"]
        values = value[value_field]
        if not isinstance(entity_to_path, dict) or not entity_to_path:
            fail(f"claim {claim_id}: pattern.entity_to_path must be a non-empty mapping")
        if not isinstance(values, dict) or not values:
            fail(f"claim {claim_id}: pattern.{value_field} must be a non-empty mapping")
        if set(entity_to_path) != set(values):
            fail(
                f"claim {claim_id}: pattern.{value_field} keys must match "
                "pattern.entity_to_path keys"
            )
        for key, path in entity_to_path.items():
            require_non_empty_string(key, "pattern.entity_to_path key", claim_id)
            require_non_empty_string(path, f"pattern.entity_to_path[{key!r}]", claim_id)
        for key, item in values.items():
            _require_finite_number(item, f"pattern.{value_field}[{key!r}]", claim_id)
        if value["direction"] not in {"lower_is_better", "higher_is_better"}:
            fail(f"claim {claim_id}: invalid pattern.direction")
        if value.get("tie_policy", "strict") not in {"strict", "adjacent_swap_ok"}:
            fail(f"claim {claim_id}: invalid pattern.tie_policy")
        return

    if kind == "monotone_with":
        required = {"pattern_kind", "metric_path", "parameter_path", "direction"}
        unknown = set(value) - required
        missing = sorted(required - value.keys())
        if missing:
            fail(f"claim {claim_id}: pattern missing required fields: {', '.join(missing)}")
        if unknown:
            fail(f"claim {claim_id}: pattern has unknown keys: {sorted(unknown)}")
        require_non_empty_string(value["metric_path"], "pattern.metric_path", claim_id)
        require_non_empty_string(
            value["parameter_path"], "pattern.parameter_path", claim_id
        )
        if value["direction"] not in {"increasing", "decreasing"}:
            fail(f"claim {claim_id}: invalid pattern.direction")
        return

    fail(f"claim {claim_id}: invalid pattern.pattern_kind {kind!r}")


def validate_concordance_block(value: Any, claim_id: str) -> None:
    if not isinstance(value, dict):
        fail(f"claim {claim_id}: concordance must be a mapping")
    required = {"pattern", "paper_locator", "prior_binding"}
    missing = sorted(required - value.keys())
    unknown = set(value) - required
    if missing:
        fail(f"claim {claim_id}: concordance missing required fields: {', '.join(missing)}")
    if unknown:
        fail(f"claim {claim_id}: concordance has unknown keys: {sorted(unknown)}")
    require_non_empty_string(
        value["paper_locator"], "concordance.paper_locator", claim_id
    )
    _validate_pattern(value["pattern"], claim_id, observed_value=False)
    prior_binding = value["prior_binding"]
    if not isinstance(prior_binding, dict):
        fail(f"claim {claim_id}: concordance.prior_binding must be a mapping")
    prior_required = {
        "prior_unit",
        "prior_metric_definition",
        "locator",
        "prior_extraction_note",
        "source_id",
    }
    missing_prior = sorted(prior_required - prior_binding.keys())
    unknown_prior = set(prior_binding) - prior_required
    if missing_prior:
        fail(
            f"claim {claim_id}: concordance.prior_binding missing required "
            f"fields: {', '.join(missing_prior)}"
        )
    if unknown_prior:
        fail(
            f"claim {claim_id}: concordance.prior_binding has unknown keys: "
            f"{sorted(unknown_prior)}"
        )
    for field in prior_required:
        require_non_empty_string(
            prior_binding[field], f"concordance.prior_binding.{field}", claim_id
        )


def validate_observation_block(value: Any, claim_id: str) -> None:
    if not isinstance(value, dict):
        fail(f"claim {claim_id}: observation must be a mapping")
    required = {"third_party_tool", "metric_definition", "pattern", "paper_locator"}
    missing = sorted(required - value.keys())
    unknown = set(value) - required
    if missing:
        fail(f"claim {claim_id}: observation missing required fields: {', '.join(missing)}")
    if unknown:
        fail(f"claim {claim_id}: observation has unknown keys: {sorted(unknown)}")
    require_non_empty_string(
        value["third_party_tool"], "observation.third_party_tool", claim_id
    )
    require_non_empty_string(
        value["metric_definition"], "observation.metric_definition", claim_id
    )
    require_non_empty_string(value["paper_locator"], "observation.paper_locator", claim_id)
    _validate_pattern(value["pattern"], claim_id, observed_value=True)


def merge_vocabularies(declared: Any) -> dict[str, set[str]]:
    merged: dict[str, set[str]] = {k: set(v) for k, v in BASE_VOCABULARIES.items()}
    if declared is None:
        return merged
    if not isinstance(declared, dict):
        fail("vocabularies must be a mapping")
    for axis, items in declared.items():
        if axis not in merged:
            fail(
                f"unknown vocabulary axis: {axis!r} "
                f"(allowed: {sorted(merged)})"
            )
        if not isinstance(items, list):
            fail(f"vocabularies.{axis} must be a list of strings")
        for item in items:
            if not isinstance(item, str) or not item.strip():
                fail(f"vocabularies.{axis} must contain only non-empty strings")
        merged[axis].update(items)
    return merged


def _load_yaml_mapping(path: pathlib.Path, label: str) -> dict:
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        fail(f"{label} must be a mapping: {path}")
    return data


def _collect(
    top_path: pathlib.Path,
) -> tuple[str, dict[str, set[str]], list[Any]]:
    """Return (project, vocabularies, claims) from the manifest and includes."""
    root = top_path.parent
    data = _load_yaml_mapping(top_path, "manifest")
    if data.get("version") is None:
        fail("version is required")
    project = data.get("project")
    if not isinstance(project, str) or not project.strip():
        fail("project is required (top-level non-empty string)")

    vocabularies = merge_vocabularies(data.get("vocabularies"))

    claims: list[Any] = list(data.get("claims") or [])
    includes = data.get("include") or []
    if not isinstance(includes, list):
        fail("include must be a list of paths")

    for raw in includes:
        if not isinstance(raw, str) or not raw.strip():
            fail("include entries must be non-empty strings")
        included_path = root / raw
        if not included_path.exists():
            fail(f"include path does not exist: {raw}")
        included = _load_yaml_mapping(included_path, f"included manifest {raw}")
        included_claims = included.get("claims")
        if not isinstance(included_claims, list) or not included_claims:
            fail(f"include {raw}: claims must be a non-empty list")
        claims.extend(included_claims)

    if not claims and not project.startswith("extracted/"):
        fail("claims must be a non-empty list (inline or via include:)")
    return project, vocabularies, claims


def _collect_claims(top_path: pathlib.Path) -> list[Any]:
    """Backward-compatible helper for callers, including evident.py."""
    _, _, claims = _collect(top_path)
    return claims


def validate_manifest(path: pathlib.Path, *, strict_release_pins: bool = False) -> None:
    root = path.parent
    project, vocabularies, claims = _collect(path)

    seen_ids: set[str] = set()
    for index, claim in enumerate(claims):
        if not isinstance(claim, dict):
            fail(f"claim at index {index} must be a mapping")
        claim_id = str(claim.get("id", f"<index:{index}>"))

        missing_common = sorted(REQUIRED_FIELDS_COMMON - claim.keys())
        if missing_common:
            fail(
                f"claim {claim_id}: missing required fields: "
                f"{', '.join(missing_common)}"
            )
        require_non_empty_string(claim["id"], "id", claim_id)
        if claim["id"] in seen_ids:
            fail(f"duplicate claim id: {claim['id']}")
        seen_ids.add(claim["id"])

        kind = claim.get("kind", "measurement")
        if kind not in VALID_KINDS:
            fail(f"claim {claim_id}: invalid kind {kind!r}")

        require_non_empty_string(claim["title"], "title", claim_id)
        require_non_empty_string(claim["claim"], "claim", claim_id)

        require_non_empty_string(claim["tier"], "tier", claim_id)
        if claim["tier"] not in VALID_TIERS:
            fail(f"claim {claim_id}: invalid tier {claim['tier']!r}")

        is_extracted_research_draft = (
            claim["tier"] == "research"
            and provenance_kind(claim) in {"extracted-from-paper", "extracted-from-repo"}
        )
        is_typed_declaration = kind in {
            "metadata_compatibility",
            "behavioral_concordance",
            "third_party_observation",
        }
        if not is_typed_declaration:
            required_workflow = (
                {"case", "source", "evidence"}
                if is_extracted_research_draft
                else REQUIRED_FIELDS_WORKFLOW
            )
            missing_workflow = sorted(required_workflow - claim.keys())
            if missing_workflow:
                fail(
                    f"claim {claim_id}: missing required fields: "
                    f"{', '.join(missing_workflow)}"
                )
            validate_existing_path(root, claim["case"], "case", claim_id)
            validate_existing_path(root, claim["source"], "source", claim_id)
            if "trust_strategy" in claim:
                strategies = require_string_list(
                    claim["trust_strategy"], "trust_strategy", claim_id
                )
                invalid = sorted(set(strategies) - VALID_TRUST_STRATEGIES)
                if invalid:
                    fail(
                        f"claim {claim_id}: invalid trust strategies: "
                        f"{', '.join(invalid)}"
                    )
            if "assumptions" in claim:
                require_string_list(claim["assumptions"], "assumptions", claim_id)
            if "failure_modes" in claim:
                require_string_list(claim["failure_modes"], "failure_modes", claim_id)
            oracles = validate_evidence(
                claim["evidence"],
                claim_id,
                vocabularies,
                allow_unknown_vocab=is_extracted_research_draft,
            )
        else:
            oracles = []
            if "source" in claim:
                require_non_empty_string(claim["source"], "source", claim_id)
            if "case" in claim:
                require_non_empty_string(claim["case"], "case", claim_id)

        if "pattern" in claim:
            validate_existing_path(root, claim["pattern"], "pattern", claim_id)

        if "capabilities" in claim:
            caps = require_string_list(
                claim["capabilities"], "capabilities", claim_id
            )
            for cap in caps:
                require_in_vocab(
                    cap, "capability", vocabularies, "capabilities[]", claim_id
                )

        if "subsystem" in claim:
            require_non_empty_string(claim["subsystem"], "subsystem", claim_id)
            require_in_vocab(
                claim["subsystem"], "subsystem", vocabularies, "subsystem", claim_id
            )
        if "inputs" in claim:
            validate_inputs(
                claim["inputs"],
                claim_id,
                vocabularies,
                claim["tier"],
                strict_release_pins=strict_release_pins,
            )
        if "outputs" in claim:
            validate_outputs(claim["outputs"], claim_id)
        if "pinned_versions" in claim:
            validate_mapping(claim["pinned_versions"], "pinned_versions", claim_id)
        if "tolerances" in claim:
            validate_tolerances(
                claim["tolerances"],
                claim_id,
                vocabularies,
                claim["tier"],
                allow_unknown_vocab=is_extracted_research_draft,
            )
        if "last_verified" in claim:
            validate_last_verified(
                claim["last_verified"],
                claim_id,
                tier=claim["tier"],
                strict_release_pins=strict_release_pins,
            )
        validate_provenance_and_reviewers(claim, claim_id)

        if kind == "measurement":
            required_measurement = (
                set() if is_extracted_research_draft else REQUIRED_FIELDS_MEASUREMENT
            )
            missing_meas = sorted(required_measurement - claim.keys())
            if missing_meas:
                fail(
                    f"claim {claim_id}: kind=measurement missing required "
                    f"fields: {', '.join(missing_meas)}"
                )
            if "pinned_versions" in claim:
                validate_pinned_versions(
                    claim["pinned_versions"],
                    oracles,
                    project,
                    claim_id,
                    tier=claim["tier"],
                    strict_release_pins=strict_release_pins,
                )
            for disallowed in ("metadata", "concordance", "observation"):
                if disallowed in claim:
                    fail(
                        f"claim {claim_id}: kind=measurement must not carry "
                        f"{disallowed}"
                    )
        elif kind == "metadata_compatibility":
            if "metadata" not in claim:
                fail(f"claim {claim_id}: kind=metadata_compatibility requires metadata")
            validate_metadata_block(claim["metadata"], claim_id)
            for disallowed in ("evidence", "tolerances", "concordance", "observation"):
                if disallowed in claim:
                    fail(
                        f"claim {claim_id}: kind=metadata_compatibility must not "
                        f"carry {disallowed}"
                    )
        elif kind == "behavioral_concordance":
            if "concordance" not in claim:
                fail(
                    f"claim {claim_id}: kind=behavioral_concordance requires "
                    "concordance"
                )
            validate_concordance_block(claim["concordance"], claim_id)
            for disallowed in ("source", "tolerances", "metadata", "observation"):
                if disallowed in claim:
                    fail(
                        f"claim {claim_id}: kind=behavioral_concordance must not "
                        f"carry {disallowed}"
                    )
            if claim["tier"] in {"ci", "release"} and "evidence" not in claim:
                fail(
                    f"claim {claim_id}: kind=behavioral_concordance requires "
                    f"evidence at tier {claim['tier']}"
                )
            if "evidence" in claim:
                concordance_oracles = validate_evidence(
                    claim["evidence"],
                    claim_id,
                    vocabularies,
                    require_oracle=False,
                )
                if concordance_oracles:
                    fail(
                        f"claim {claim_id}: kind=behavioral_concordance evidence "
                        "must not carry oracle"
                    )
        elif kind == "third_party_observation":
            if "observation" not in claim:
                fail(
                    f"claim {claim_id}: kind=third_party_observation requires "
                    "observation"
                )
            validate_observation_block(claim["observation"], claim_id)
            for disallowed in (
                "source",
                "case",
                "last_verified",
                "tolerances",
                "metadata",
                "concordance",
            ):
                if disallowed in claim:
                    fail(
                        f"claim {claim_id}: kind=third_party_observation must not "
                        f"carry {disallowed}"
                    )
            if claim["tier"] in {"ci", "release"} and "evidence" not in claim:
                fail(
                    f"claim {claim_id}: kind=third_party_observation requires "
                    f"evidence at tier {claim['tier']}"
                )
            if "evidence" in claim:
                observation_oracles = validate_evidence(
                    claim["evidence"],
                    claim_id,
                    vocabularies,
                    require_oracle=False,
                )
                if observation_oracles:
                    fail(
                        f"claim {claim_id}: kind=third_party_observation evidence "
                        "must not carry oracle"
                    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", nargs="?", default="evident.yaml")
    parser.add_argument(
        "--strict-release-pins",
        action="store_true",
        help=(
            "reject placeholder corpus hashes, pinned versions, and "
            "last_verified release pins (release prep)"
        ),
    )
    args = parser.parse_args()

    try:
        validate_manifest(
            pathlib.Path(args.manifest),
            strict_release_pins=args.strict_release_pins,
        )
    except Exception as exc:
        print(f"manifest invalid: {exc}", file=sys.stderr)
        return 1
    print(f"manifest valid: {args.manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
