"""Curator rephrase tests.

Covers:

- happy path: editor returns modified YAML, manifest is updated,
  fields_changed reflects the diff
- no-op: editor returns unchanged text → fields_changed is empty
- locked-field rejection (id/tier/provenance can't change via
  rephrase — those need typed events)
- non-allowlisted-field rejection (any field outside the explicit
  editable set is rejected)
- invalid-yaml rejection
- unknown-claim-id rejection
- editor returning non-dict rejected
- pre/post sha discipline (load-bearing for audit trail)
"""

from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from evident_agent.curator import (
    CuratorError,
    RephraseResult,
    rephrase_claim,
)


def _sample_manifest(tmp_path: Path) -> Path:
    body = {
        "version": "0.1",
        "project": "extracted/test",
        "claims": [
            {
                "id": "test-claim-one",
                "title": "Original title",
                "kind": "measurement",
                "tier": "research",
                "source": "source/cited.md",
                "case": "source/cited.md#test-claim-one",
                "claim": "Our method achieves rmsd < 0.5.",
                "tolerances": [
                    {
                        "metric": "rmsd",
                        "op": "<",
                        "value": 0.5,
                        "prose": "original prose",
                    }
                ],
                "evidence": {
                    "oracle": ["Paper-Authority"],
                    "command": "no-replay-path",
                    "artifact": "source/cited.md#test-claim-one",
                    "replay_status": "unavailable_artifacts",
                    "replay_reason": "code_private",
                },
                "provenance": {
                    "kind": "extracted-from-paper",
                    "source_id": "arxiv:2501.99999v1",
                    "extractor": {
                        "model": "claude-opus-4-7",
                        "extracted_at": "2026-05-01T10:00:00Z",
                    },
                    "curator": None,
                },
            },
        ],
    }
    path = tmp_path / "evident.yaml"
    path.write_text(yaml.safe_dump(body, sort_keys=False))
    return path


# ---------------------------------------------------------------------
# Happy paths
# ---------------------------------------------------------------------


def test_rephrase_updates_title_and_records_change(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)

    def _editor(initial: str) -> str:
        parsed = yaml.safe_load(initial)
        parsed["title"] = "Edited title"
        return yaml.safe_dump(parsed, sort_keys=False)

    result = rephrase_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        editor=_editor,
    )
    assert result.fields_changed == ["title"]
    assert result.pre_edit_sha != result.post_edit_sha
    parsed = yaml.safe_load(manifest_path.read_text())
    assert parsed["claims"][0]["title"] == "Edited title"
    # Locked fields untouched.
    assert parsed["claims"][0]["id"] == "test-claim-one"
    assert parsed["claims"][0]["tier"] == "research"


def test_rephrase_updates_claim_prose_and_tolerance(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)

    def _editor(initial: str) -> str:
        parsed = yaml.safe_load(initial)
        parsed["claim"] = "rephrased claim text"
        parsed["tolerances"][0]["prose"] = "rephrased tolerance prose"
        return yaml.safe_dump(parsed, sort_keys=False)

    result = rephrase_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        editor=_editor,
    )
    assert sorted(result.fields_changed) == ["claim", "tolerances"]


def test_rephrase_noop_returns_empty_fields_changed(tmp_path: Path):
    """Curator opened the editor but didn't change anything. The
    walkthrough downstream records this as a typed no-change record."""
    manifest_path = _sample_manifest(tmp_path)

    def _editor(initial: str) -> str:
        return initial  # unchanged

    result = rephrase_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        editor=_editor,
    )
    assert result.fields_changed == []
    assert result.pre_edit_sha == result.post_edit_sha


# ---------------------------------------------------------------------
# Locked-field rejection
# ---------------------------------------------------------------------


@pytest.mark.parametrize(
    "field_to_change",
    ["id", "tier", "kind", "evidence", "provenance"],
)
def test_rephrase_rejects_locked_field_change(
    tmp_path: Path, field_to_change: str,
):
    """Locked fields must go through typed paths (PromoteFromExtracted
    events, drop_claim, schema work) — never a free-form rephrase
    edit."""
    manifest_path = _sample_manifest(tmp_path)

    def _editor(initial: str) -> str:
        parsed = yaml.safe_load(initial)
        if field_to_change == "tier":
            parsed["tier"] = "ci"
        elif field_to_change == "id":
            parsed["id"] = "renamed-claim-id"
        elif field_to_change == "kind":
            parsed["kind"] = "policy"
        elif field_to_change == "evidence":
            parsed["evidence"]["command"] = "echo NEW"
        elif field_to_change == "provenance":
            parsed["provenance"]["source_id"] = "doi:NEW"
        return yaml.safe_dump(parsed, sort_keys=False)

    with pytest.raises(CuratorError) as exc:
        rephrase_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            editor=_editor,
        )
    assert field_to_change in str(exc.value)
    # Manifest must be unchanged after a rejected rephrase.
    parsed = yaml.safe_load(manifest_path.read_text())
    if field_to_change == "id":
        assert parsed["claims"][0]["id"] == "test-claim-one"
    elif field_to_change == "tier":
        assert parsed["claims"][0]["tier"] == "research"


def test_rephrase_rejects_new_field_outside_allowlist(tmp_path: Path):
    """Adding a brand-new field (e.g. `summary: ...`) outside the
    editable allowlist is rejected — the rephrase contract is
    closed."""
    manifest_path = _sample_manifest(tmp_path)

    def _editor(initial: str) -> str:
        parsed = yaml.safe_load(initial)
        parsed["summary"] = "a new field that isn't in the allowlist"
        return yaml.safe_dump(parsed, sort_keys=False)

    with pytest.raises(CuratorError) as exc:
        rephrase_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            editor=_editor,
        )
    assert "allowlist" in str(exc.value)


# ---------------------------------------------------------------------
# Editor error paths
# ---------------------------------------------------------------------


def test_rephrase_rejects_invalid_yaml(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)

    def _editor(_initial: str) -> str:
        return "this: is: not: valid: yaml: ["

    with pytest.raises(CuratorError) as exc:
        rephrase_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            editor=_editor,
        )
    assert "valid YAML" in str(exc.value)


def test_rephrase_rejects_non_dict_result(tmp_path: Path):
    """If the curator edits the file down to a YAML scalar or list,
    we can't replace the claim object — rejected."""
    manifest_path = _sample_manifest(tmp_path)

    def _editor(_initial: str) -> str:
        return "- just a list\n- not a claim mapping\n"

    with pytest.raises(CuratorError) as exc:
        rephrase_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            editor=_editor,
        )
    assert "mapping" in str(exc.value) or "YAML" in str(exc.value)


def test_rephrase_rejects_unknown_claim_id(tmp_path: Path):
    manifest_path = _sample_manifest(tmp_path)

    def _editor(initial: str) -> str:
        return initial

    with pytest.raises(CuratorError) as exc:
        rephrase_claim(
            manifest_path=manifest_path,
            claim_id="does-not-exist",
            editor=_editor,
        )
    assert "does-not-exist" in str(exc.value)


# ---------------------------------------------------------------------
# Sha discipline
# ---------------------------------------------------------------------


def test_rephrase_rejects_adding_null_to_locked_field(tmp_path: Path):
    """Codex F-REPHRASE-CR1 P1: a curator adding ``last_verified:
    null`` (where the field was absent in the original) is a CHANGE
    on a locked field. Using ``dict.get(k)`` alone would have
    treated absent and null as equal — the sentinel comparison
    catches it."""
    manifest_path = _sample_manifest(tmp_path)

    def _editor(initial: str) -> str:
        parsed = yaml.safe_load(initial)
        assert "last_verified" not in parsed
        parsed["last_verified"] = None  # add null
        return yaml.safe_dump(parsed, sort_keys=False)

    with pytest.raises(CuratorError) as exc:
        rephrase_claim(
            manifest_path=manifest_path,
            claim_id="test-claim-one",
            editor=_editor,
        )
    assert "last_verified" in str(exc.value)


def test_rephrase_semantic_noop_does_not_rewrite_manifest(tmp_path: Path):
    """Codex F-REPHRASE-CR P2: editor reorders keys but the parsed
    dict compares equal. After validation returns fields_changed=[],
    the manifest must NOT be re-serialized (which would change
    bytes and post_edit_sha). Verify by checking the file's bytes
    are identical."""
    import hashlib

    manifest_path = _sample_manifest(tmp_path)
    pre_bytes = manifest_path.read_bytes()
    pre_sha = hashlib.sha256(pre_bytes).hexdigest()

    def _editor(initial: str) -> str:
        parsed = yaml.safe_load(initial)
        # Reorder keys — yaml.safe_dump output bytes differ but
        # parsed dict equality is unchanged.
        reordered = {k: parsed[k] for k in reversed(list(parsed.keys()))}
        return yaml.safe_dump(reordered, sort_keys=False)

    result = rephrase_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        editor=_editor,
    )
    assert result.fields_changed == []
    assert result.pre_edit_sha == result.post_edit_sha
    # Manifest bytes unchanged on disk.
    assert manifest_path.read_bytes() == pre_bytes
    assert (
        hashlib.sha256(manifest_path.read_bytes()).hexdigest()
        == pre_sha
    )


def test_rephrase_pre_edit_sha_records_what_curator_reviewed(tmp_path: Path):
    """Load-bearing for the audit trail: the curator's `pre_edit_sha`
    must match the bytes that were on disk BEFORE the editor opened.
    """
    import hashlib

    manifest_path = _sample_manifest(tmp_path)
    pre_bytes_sha = hashlib.sha256(manifest_path.read_bytes()).hexdigest()

    def _editor(initial: str) -> str:
        parsed = yaml.safe_load(initial)
        parsed["title"] = "Edited"
        return yaml.safe_dump(parsed, sort_keys=False)

    result = rephrase_claim(
        manifest_path=manifest_path,
        claim_id="test-claim-one",
        editor=_editor,
    )
    assert result.pre_edit_sha == pre_bytes_sha
