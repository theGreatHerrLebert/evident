"""Phase 5 PR4: tests for the extract output writer.

Verifies the shape of the three output files (evident.yaml,
source/cited.md, EXTRACTION.md) the curator reviews. The manifest
must round-trip through typed-trust's parser — the Rust test
``structured_provenance_with_source_context_parses`` is the
corresponding load-bearing assertion on the framework side.
"""

from __future__ import annotations

from pathlib import Path

import yaml

from evident_agent.extract.render import (
    ExtractedClaim,
    ExtractionResult,
    RejectedCandidate,
    build_manifest_dict,
    now_utc_isoformat,
    render_cited_md,
    render_extraction_md,
    write_outputs,
)


def _example_result() -> ExtractionResult:
    return ExtractionResult(
        source_id="arxiv:2501.12345v1",
        source_sha="deadbeef0123456789",
        extractor_model="claude-opus-4-7",
        extracted_at="2026-09-14T10:00:00Z",
        claims=[
            ExtractedClaim(
                id="cool-paper-rmsd-vs-baseline",
                title=(
                    "Cool Paper claims median RMSD below 0.5 angstrom"
                ),
                claim=(
                    "Median RMSD < 0.5 Å against Baseline X on the "
                    "BPTI test suite (n=1000), per Section 4.2 "
                    "Table 3."
                ),
                subject_aliases=["our method", "we", "ours"],
                tolerances=[
                    {
                        "metric": "median_rmsd",
                        "op": "<",
                        "value": 0.5,
                        "source_span": (
                            "our method achieves median rmsd less "
                            "than 0.5 across the BPTI test suite"
                        ),
                        "prose": (
                            "paper Table 3 row 'ours': median "
                            "RMSD = 0.42 Å; bound 0.5 stated"
                        ),
                    }
                ],
            ),
        ],
        rejections=[
            RejectedCandidate(
                candidate_text=(
                    "Our method outperforms the baseline on the "
                    "BPTI suite."
                ),
                locator="§4.2 ¶3",
                reason="bound_not_stated",
                rationale=(
                    "Span uses ranking language ('outperforms') "
                    "with no numeric bound. The bound 0.42 lives "
                    "only in a separate sentence."
                ),
            ),
        ],
    )


def _empty_result() -> ExtractionResult:
    return ExtractionResult(
        source_id="arxiv:2501.99999v1",
        source_sha="cafe",
        extractor_model="claude-opus-4-7",
        extracted_at="2026-09-14T10:00:00Z",
    )


# ---------------------------------------------------------------------
# build_manifest_dict
# ---------------------------------------------------------------------


def test_manifest_uses_tier_research_for_every_claim():
    """Codex v3: extracted claims always ship at tier:research.
    Promotion requires a PromoteFromExtracted event (typed-trust PR3)."""
    manifest = build_manifest_dict(
        _example_result(), project="extracted/cool-paper"
    )
    assert all(c["tier"] == "research" for c in manifest["claims"])


def test_manifest_provenance_kind_is_extracted_from_paper():
    manifest = build_manifest_dict(
        _example_result(), project="extracted/cool-paper"
    )
    prov = manifest["claims"][0]["provenance"]
    assert prov["kind"] == "extracted-from-paper"
    assert prov["source_id"] == "arxiv:2501.12345v1"
    assert prov["source_sha"] == "deadbeef0123456789"
    assert prov["extractor"]["model"] == "claude-opus-4-7"


def test_manifest_evidence_carries_replay_status_sentinel():
    """Phase 5 PR1 fields must be emitted. The framework's
    pair-validator requires (status=unavailable_artifacts, reason set)."""
    manifest = build_manifest_dict(
        _example_result(), project="extracted/cool-paper"
    )
    evidence = manifest["claims"][0]["evidence"]
    assert evidence["replay_status"] == "unavailable_artifacts"
    assert evidence["replay_reason"] == "code_private"


def test_manifest_tolerance_carries_metric_op_value_and_prose():
    manifest = build_manifest_dict(
        _example_result(), project="extracted/cool-paper"
    )
    t = manifest["claims"][0]["tolerances"][0]
    assert t["metric"] == "median_rmsd"
    assert t["op"] == "<"
    assert t["value"] == 0.5
    # source_span travels in prose so the curator sees it without
    # extractor-internal fields polluting the manifest schema.
    assert "source_span" in t["prose"]


def test_empty_extraction_produces_manifest_with_zero_claims():
    """A paper with zero extracted claims is a valid output (codex
    v3: default-deny means honest emptiness > invented tolerances)."""
    manifest = build_manifest_dict(
        _empty_result(), project="extracted/empty-paper"
    )
    assert manifest["claims"] == []


# ---------------------------------------------------------------------
# render_cited_md
# ---------------------------------------------------------------------


def test_cited_md_anchors_each_claim_with_id():
    cited = render_cited_md(_example_result())
    assert 'id="cool-paper-rmsd-vs-baseline"' in cited


def test_cited_md_quotes_each_source_span():
    cited = render_cited_md(_example_result())
    # Citation includes the exact source_span text.
    assert "our method achieves median rmsd less than 0.5" in cited


def test_cited_md_for_empty_result_says_no_claims():
    cited = render_cited_md(_empty_result())
    assert "No claims extracted" in cited


# ---------------------------------------------------------------------
# render_extraction_md
# ---------------------------------------------------------------------


def test_extraction_md_lists_accepted_claims_with_id():
    md = render_extraction_md(_example_result())
    assert "Accepted claims (1)" in md
    assert "cool-paper-rmsd-vs-baseline" in md


def test_extraction_md_groups_rejections_by_reason():
    md = render_extraction_md(_example_result())
    assert "Rejected candidates (1)" in md
    assert "`bound_not_stated`" in md
    # The candidate text and rationale both appear so the curator
    # can audit without re-fetching the source.
    assert "outperforms" in md


def test_extraction_md_for_empty_result_says_default_deny_rejected_all():
    md = render_extraction_md(_empty_result())
    assert "Accepted claims (0)" in md
    assert "Default-deny" in md


# ---------------------------------------------------------------------
# write_outputs (integration)
# ---------------------------------------------------------------------


def test_write_outputs_creates_the_three_files(tmp_path: Path):
    out = tmp_path / "extracted" / "cool-paper"
    write_outputs(
        _example_result(),
        output_dir=out,
        project="extracted/cool-paper",
    )
    assert (out / "evident.yaml").is_file()
    assert (out / "source" / "cited.md").is_file()
    assert (out / "EXTRACTION.md").is_file()


def test_write_outputs_evident_yaml_parses_back_as_yaml(tmp_path: Path):
    """The draft manifest must be valid YAML (so typed-trust can
    parse it). Round-trip through yaml.safe_load to confirm."""
    out = tmp_path / "extracted" / "cool-paper"
    write_outputs(
        _example_result(),
        output_dir=out,
        project="extracted/cool-paper",
    )
    parsed = yaml.safe_load((out / "evident.yaml").read_text())
    assert parsed["project"] == "extracted/cool-paper"
    assert parsed["claims"][0]["id"] == "cool-paper-rmsd-vs-baseline"


def test_write_outputs_is_idempotent(tmp_path: Path):
    """Running the extractor twice on the same source produces the
    same byte-for-byte output (so re-runs don't churn the curator's
    review diff)."""
    out = tmp_path / "extracted" / "cool-paper"
    result = _example_result()
    write_outputs(result, output_dir=out, project="extracted/cool-paper")
    first = (out / "evident.yaml").read_text()
    write_outputs(result, output_dir=out, project="extracted/cool-paper")
    second = (out / "evident.yaml").read_text()
    assert first == second


# ---------------------------------------------------------------------
# now_utc_isoformat helper
# ---------------------------------------------------------------------


def test_now_utc_isoformat_ends_with_z():
    ts = now_utc_isoformat()
    assert ts.endswith("Z"), f"got {ts!r}"
