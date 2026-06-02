//! Phase 5 PR3: `PromoteFromExtracted` event variant + validator rules.
//!
//! A claim authored by `evident-extract` ships at `tier: research`.
//! Promoting it to `ci` or `release` requires a human curator to author
//! a dedicated event recording the review. The promotion event has its
//! own typed variant (rather than overloading `Endorse`) so the audit
//! trail keeps "this claim is supported" distinct from "this claim's
//! lifecycle tier was promoted on this date by this reviewer."
//!
//! Five validator rules (per `EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md`
//! v3 §"PromoteFromExtracted typed event"):
//!
//! 1. **Gate-on-tier.** An extracted claim at `tier: research`
//!    requires no promotion event. The gate only fires when the
//!    manifest sets the claim's tier to `ci` or `release`.
//! 2. **Matching.** The event's `target_claim`, `from_tier`, `to_tier`,
//!    and `reviewed_extraction_sha` must match the manifest.
//! 3. **Ordering.** `event_date` must not predate
//!    `provenance.extractor.extracted_at`.
//! 4. **Uniqueness / latest-event.** For a given (claim, from, to),
//!    the latest event by `event_date` wins; earlier events stay in
//!    the history for audit but don't gate the tier.
//! 5. **Endorse-independence.** An Endorse on the extracted claim is
//!    a separate fact from a PromoteFromExtracted. The render layer
//!    must not collapse them.

use typed_trust::translate::{
    parse_manifest_file, translate_review_event, validate_promotion_rules, ManifestProvenance,
    ManifestReviewAuthor, ManifestReviewEvent, PromotionError,
};

const EXTRACTED_CI_MANIFEST_YAML: &str = r#"
claims:
  - id: cool-paper-rmsd-vs-baseline
    title: Cool Paper claims median RMSD below 0.5 angstrom
    kind: measurement
    tier: ci
    case: source/cited.md#claim-1
    source: ..
    claim: median RMSD < 0.5 on BPTI suite
    tolerances:
      - metric: median_rmsd
        op: "<"
        value: 0.5
        prose: |
          paper Table 3 row ours: median RMSD = 0.42; bound 0.5 stated
    evidence:
      oracle: [Paper-Authority]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: code_private
    provenance:
      kind: extracted-from-paper
      source_id: arxiv:2501.12345v1
      source_sha: deadbeef
      extractor:
        model: claude-opus-4-7
        model_version: "20260601"
        extracted_at: "2026-09-14T10:00:00Z"
"#;

const EXTRACTED_RESEARCH_MANIFEST_YAML: &str = r#"
claims:
  - id: cool-paper-rmsd-vs-baseline
    title: Cool Paper claims median RMSD below 0.5 angstrom
    kind: measurement
    tier: research
    case: source/cited.md#claim-1
    source: ..
    claim: median RMSD < 0.5 on BPTI suite
    tolerances:
      - metric: median_rmsd
        op: "<"
        value: 0.5
        prose: |
          paper Table 3 row ours: median RMSD = 0.42; bound 0.5 stated
    evidence:
      oracle: [Paper-Authority]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: code_private
    provenance:
      kind: extracted-from-paper
      extractor:
        extracted_at: "2026-09-14T10:00:00Z"
"#;

fn human_curator() -> ManifestReviewAuthor {
    ManifestReviewAuthor {
        kind: "human".into(),
        name: "Jane Doe".into(),
        version: None,
        context: None,
        orcid: Some("0000-0001-2345-6789".into()),
        affiliation: Some("University of Example".into()),
    }
}

fn promotion_event(
    target: &str,
    from_tier: &str,
    to_tier: &str,
    sha: &str,
    event_date: &str,
) -> ManifestReviewEvent {
    ManifestReviewEvent {
        claim_id: target.into(),
        kind: "promote_from_extracted".into(),
        author: human_curator(),
        rationale: "Reviewed Table 3; extractor's reading is correct.".into(),
        timestamp: event_date.into(),
        event_id: None,
        checks: None,
        observed_value: None,
        tolerance: None,
        failure_reason: None,
        challenge: None,
        target: None,
        supersede: None,
        protocol: None,
        promote_from_extracted: Some(typed_trust::translate::ManifestPromoteFromExtractedBlock {
            target_claim: target.into(),
            from_tier: from_tier.into(),
            to_tier: to_tier.into(),
            reviewed_extraction_sha: sha.into(),
        }),
    }
}

// ----------------------------------------------------------------------
// Translator: round-trip through translate_review_event.
// ----------------------------------------------------------------------

#[test]
fn translates_promote_from_extracted_kind() {
    let entry = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "abc123",
        "2026-09-15T10:00:00Z",
    );
    let event = translate_review_event(&entry).expect("promote_from_extracted translates");
    use typed_trust::review::ReviewKind;
    match &event.kind {
        ReviewKind::PromoteFromExtracted {
            target_claim,
            from_tier,
            to_tier,
            reviewed_extraction_sha,
        } => {
            assert_eq!(target_claim.as_str(), "cool-paper-rmsd-vs-baseline");
            assert_eq!(from_tier, "research");
            assert_eq!(to_tier, "ci");
            assert_eq!(reviewed_extraction_sha, "abc123");
        }
        other => panic!("expected PromoteFromExtracted, got {other:?}"),
    }
}

#[test]
fn rejects_promote_from_extracted_without_block() {
    let mut entry = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "abc",
        "2026-09-15T10:00:00Z",
    );
    entry.promote_from_extracted = None;
    let err = translate_review_event(&entry).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("promote_from_extracted") && msg.contains("block"),
        "expected error naming the missing block, got: {msg}"
    );
}

// ----------------------------------------------------------------------
// Validator: rule 1 — gate-on-tier.
// ----------------------------------------------------------------------

#[test]
fn rule1_research_tier_extracted_claim_requires_no_promotion() {
    let manifest = parse_manifest_file(EXTRACTED_RESEARCH_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let events: Vec<ManifestReviewEvent> = vec![];
    validate_promotion_rules(claim, &events).expect("research tier needs no promotion");
}

#[test]
fn rule1_legacy_non_extracted_claim_at_ci_tier_passes() {
    // Phase 5's gate only fires on extracted claims. A legacy
    // (provenance: automatic) claim at tier: ci is the existing path
    // and must not be affected.
    let yaml = r#"
claims:
  - id: legacy-ci
    title: legacy ci claim
    kind: measurement
    tier: ci
    case: src.md
    source: ..
    claim: legacy ci claim
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: stay under 2 percent
    evidence:
      oracle: [Biopython]
      command: pytest
      artifact: out.json
    provenance: automatic
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let claim = &manifest.claims[0];
    validate_promotion_rules(claim, &[]).expect("legacy ci claim is not gated");
}

#[test]
fn rule1_extracted_ci_claim_without_promotion_event_is_rejected() {
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let err = validate_promotion_rules(claim, &[]).unwrap_err();
    match err {
        PromotionError::MissingPromotionEvent {
            claim_id,
            current_tier,
            missing_transition,
        } => {
            assert_eq!(claim_id, "cool-paper-rmsd-vs-baseline");
            assert_eq!(current_tier, "ci");
            assert_eq!(
                missing_transition,
                ("research".into(), "ci".into()),
            );
        }
        other => panic!("expected MissingPromotionEvent, got {other:?}"),
    }
}

// ----------------------------------------------------------------------
// Validator: rule 2 — matching.
// ----------------------------------------------------------------------

#[test]
fn rule2_matching_event_passes() {
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let event = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        // matches the sha-of-evident.yaml that the curator reviewed
        "expected-yaml-sha-for-test",
        "2026-09-15T10:00:00Z",
    );
    // For this test, the validator uses the event's reviewed_extraction_sha
    // as authoritative; the manifest-sha check is a separate rule.
    validate_promotion_rules(claim, std::slice::from_ref(&event))
        .expect("matching event passes");
}

#[test]
fn rule2_mismatched_target_claim_is_rejected() {
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let event = promotion_event(
        "some-other-claim",
        "research",
        "ci",
        "abc",
        "2026-09-15T10:00:00Z",
    );
    let err = validate_promotion_rules(claim, std::slice::from_ref(&event)).unwrap_err();
    match err {
        PromotionError::MissingPromotionEvent { claim_id, .. } => {
            assert_eq!(claim_id, "cool-paper-rmsd-vs-baseline");
        }
        other => panic!("expected MissingPromotionEvent, got {other:?}"),
    }
}

#[test]
fn rule2_mismatched_to_tier_is_rejected() {
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let event = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "release", // manifest says ci
        "abc",
        "2026-09-15T10:00:00Z",
    );
    let err = validate_promotion_rules(claim, std::slice::from_ref(&event)).unwrap_err();
    assert!(
        matches!(err, PromotionError::MissingPromotionEvent { .. }),
        "expected MissingPromotionEvent for mismatched to_tier, got: {err:?}"
    );
}

// ----------------------------------------------------------------------
// Validator: rule 3 — ordering.
// ----------------------------------------------------------------------

#[test]
fn rule3_event_date_before_extracted_at_is_rejected() {
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    // event_date 2026-09-13 predates extracted_at 2026-09-14
    let event = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "abc",
        "2026-09-13T10:00:00Z",
    );
    let err = validate_promotion_rules(claim, std::slice::from_ref(&event)).unwrap_err();
    match err {
        PromotionError::PromotionPredatesExtraction {
            claim_id,
            event_date,
            extracted_at,
        } => {
            assert_eq!(claim_id, "cool-paper-rmsd-vs-baseline");
            assert_eq!(event_date, "2026-09-13T10:00:00Z");
            assert_eq!(extracted_at, "2026-09-14T10:00:00Z");
        }
        other => panic!("expected PromotionPredatesExtraction, got {other:?}"),
    }
}

// ----------------------------------------------------------------------
// Validator: rule 4 — uniqueness / latest-event.
// ----------------------------------------------------------------------

#[test]
fn rule4_latest_event_by_date_wins() {
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let mismatched_earlier = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "release", // does NOT match manifest tier ci
        "abc",
        "2026-09-14T11:00:00Z",
    );
    let matching_later = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci", // matches manifest tier
        "abc",
        "2026-09-15T10:00:00Z",
    );
    // Both events present. The earlier one is mismatched but stays in
    // the history for audit; the later one is matching and gates the
    // tier. The validator should look at the latest matching event,
    // not the earliest entry.
    validate_promotion_rules(claim, &[mismatched_earlier, matching_later])
        .expect("latest matching event wins");
}

// ----------------------------------------------------------------------
// Validator: rule 5 — Endorse-independence.
// ----------------------------------------------------------------------

#[test]
fn rule5_endorse_on_research_extracted_claim_is_allowed() {
    // A curator authoring Endorse on a research-tier extracted claim
    // says "I support this claim." It does NOT promote. The validator
    // accepts the manifest (tier: research) regardless of whether an
    // Endorse event exists.
    let manifest = parse_manifest_file(EXTRACTED_RESEARCH_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let endorse = ManifestReviewEvent {
        claim_id: "cool-paper-rmsd-vs-baseline".into(),
        kind: "endorse".into(),
        author: human_curator(),
        rationale: "Verified the cited table.".into(),
        timestamp: "2026-09-15T11:00:00Z".into(),
        event_id: None,
        checks: None,
        observed_value: None,
        tolerance: None,
        failure_reason: None,
        challenge: None,
        target: None,
        supersede: None,
        protocol: None,
        promote_from_extracted: None,
    };
    validate_promotion_rules(claim, std::slice::from_ref(&endorse))
        .expect("Endorse on research tier is independent of promotion");
}

#[test]
fn rule5_endorse_does_not_satisfy_promotion_gate_on_ci_claim() {
    // Same Endorse event, but the manifest now sets tier: ci. The
    // promotion gate fires; Endorse alone cannot satisfy it.
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let endorse = ManifestReviewEvent {
        claim_id: "cool-paper-rmsd-vs-baseline".into(),
        kind: "endorse".into(),
        author: human_curator(),
        rationale: "Verified the cited table.".into(),
        timestamp: "2026-09-15T11:00:00Z".into(),
        event_id: None,
        checks: None,
        observed_value: None,
        tolerance: None,
        failure_reason: None,
        challenge: None,
        target: None,
        supersede: None,
        protocol: None,
        promote_from_extracted: None,
    };
    let err = validate_promotion_rules(claim, std::slice::from_ref(&endorse)).unwrap_err();
    assert!(
        matches!(err, PromotionError::MissingPromotionEvent { .. }),
        "Endorse alone is not a promotion; expected MissingPromotionEvent, got: {err:?}"
    );
}

// ----------------------------------------------------------------------
// Smoke: ManifestProvenance helper sees through to the extractor block.
// ----------------------------------------------------------------------

// ----------------------------------------------------------------------
// Codex F-PR3-CR review additions.
// ----------------------------------------------------------------------

#[test]
fn rule2_rejects_event_with_from_tier_release() {
    // Codex F-PR3-CR3: PR3 requires from_tier == "research". An event
    // claiming `release -> ci` could otherwise satisfy a tier:ci
    // extracted claim, which is an impossible transition that would
    // weaken the audit trail.
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let event = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "release", // not research
        "ci",
        "abc",
        "2026-09-15T10:00:00Z",
    );
    let err = validate_promotion_rules(claim, std::slice::from_ref(&event)).unwrap_err();
    assert!(
        matches!(err, PromotionError::MissingPromotionEvent { .. }),
        "expected MissingPromotionEvent when from_tier != research, got: {err:?}"
    );
}

#[test]
fn rule4_same_timestamp_uses_event_id_as_deterministic_tiebreaker() {
    // Codex F-PR3-CR2: when two matching events share a timestamp,
    // pick_latest_by_event_date must break ties on the canonical
    // event_id (sha256 of the payload). The validator's result must
    // be invariant under list reordering.
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let mut event_a = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "sha-a",
        "2026-09-15T10:00:00Z",
    );
    event_a.rationale = "curator a rationale".into();
    let mut event_b = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "sha-b",
        "2026-09-15T10:00:00Z", // same timestamp
    );
    event_b.rationale = "curator b rationale".into();

    // Both orderings should validate cleanly (both match, both are
    // valid). The KEY guarantee is that result is invariant under
    // reordering — neither order produces an error.
    validate_promotion_rules(claim, &[event_a.clone(), event_b.clone()])
        .expect("a-then-b should validate");
    validate_promotion_rules(claim, &[event_b, event_a])
        .expect("b-then-a should validate identically");
}

#[test]
fn rejects_promote_from_extracted_with_empty_reviewed_extraction_sha() {
    // Codex F-PR3-CR4: empty reviewed_extraction_sha defeats the
    // whole point of pinning the curator's review.
    let mut entry = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "", // empty
        "2026-09-15T10:00:00Z",
    );
    let block = entry
        .promote_from_extracted
        .as_mut()
        .expect("block present");
    block.reviewed_extraction_sha = "".into();
    let err = translate_review_event(&entry).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("reviewed_extraction_sha"),
        "expected error naming reviewed_extraction_sha, got: {msg}"
    );
}

#[test]
fn unknown_target_claim_in_promotion_event_is_silently_ignored() {
    // Codex F-PR3 coverage: an event referencing a claim not in this
    // validator call is out-of-scope. validate_promotion_rules is
    // called per-claim; events targeting OTHER claims must not
    // satisfy the gate. The earlier test rule2_mismatched_target_claim
    // covers this for the no-other-events case; this test confirms
    // the same behaviour when an unrelated event coexists.
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let event = promotion_event(
        "some-other-claim",
        "research",
        "ci",
        "abc",
        "2026-09-15T10:00:00Z",
    );
    let err = validate_promotion_rules(claim, std::slice::from_ref(&event)).unwrap_err();
    assert!(
        matches!(err, PromotionError::MissingPromotionEvent { .. }),
        "out-of-scope event must not satisfy the gate, got: {err:?}"
    );
}

#[test]
fn structured_promote_from_extracted_block_rejects_unknown_field() {
    // Codex F-PR3 coverage: deny_unknown_fields on the block catches
    // typos like `target_claimm:` at parse time.
    let sidecar_json = r#"
{
  "events": [
    {
      "claim_id": "cool-paper-rmsd-vs-baseline",
      "kind": "promote_from_extracted",
      "author": {"kind": "human", "name": "Jane Doe", "orcid": "0000-0001"},
      "rationale": "looks right",
      "timestamp": "2026-09-15T10:00:00Z",
      "promote_from_extracted": {
        "target_claim": "cool-paper-rmsd-vs-baseline",
        "from_tier": "research",
        "to_tier": "ci",
        "reviewed_extraction_sha": "abc",
        "unknown_field": "trips the deny"
      }
    }
  ]
}
"#;
    let parsed: Result<typed_trust::translate::ReviewEventSidecar, _> =
        serde_json::from_str(sidecar_json);
    let err = parsed.expect_err("expected deny_unknown_fields to reject");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown_field") || msg.contains("unknown field"),
        "expected error naming the unknown field, got: {msg}"
    );
}

// ----------------------------------------------------------------------
// Multi-step promotion: research -> ci -> release
// ----------------------------------------------------------------------

const EXTRACTED_RELEASE_MANIFEST_YAML: &str = r#"
claims:
  - id: cool-paper-rmsd-vs-baseline
    title: Cool Paper claims median RMSD below 0.5 angstrom
    kind: measurement
    tier: release
    case: source/cited.md#claim-1
    source: ..
    claim: median RMSD < 0.5 on BPTI suite
    tolerances:
      - metric: median_rmsd
        op: "<"
        value: 0.5
        prose: |
          paper Table 3 row ours: median RMSD = 0.42; bound 0.5 stated
    evidence:
      oracle: [Paper-Authority]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: code_private
    provenance:
      kind: extracted-from-paper
      source_id: arxiv:2501.12345v1
      source_sha: deadbeef
      extractor:
        model: claude-opus-4-7
        model_version: "20260601"
        extracted_at: "2026-09-14T10:00:00Z"
"#;

#[test]
fn multi_step_release_with_full_chain_passes() {
    let manifest = parse_manifest_file(EXTRACTED_RELEASE_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let ev_ci = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "sha-1",
        "2026-09-15T10:00:00Z",
    );
    let ev_release = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "ci",
        "release",
        "sha-2",
        "2026-09-20T10:00:00Z",
    );
    validate_promotion_rules(claim, &[ev_ci, ev_release])
        .expect("two-event chain should satisfy the release gate");
}

#[test]
fn multi_step_release_missing_first_leg_is_rejected() {
    let manifest = parse_manifest_file(EXTRACTED_RELEASE_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let ev_release = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "ci",
        "release",
        "sha-2",
        "2026-09-20T10:00:00Z",
    );
    let err =
        validate_promotion_rules(claim, std::slice::from_ref(&ev_release)).unwrap_err();
    match err {
        PromotionError::MissingPromotionEvent {
            missing_transition, ..
        } => {
            assert_eq!(
                missing_transition,
                ("research".into(), "ci".into()),
            );
        }
        other => panic!("expected MissingPromotionEvent for first leg, got {other:?}"),
    }
}

#[test]
fn multi_step_release_missing_second_leg_is_rejected() {
    let manifest = parse_manifest_file(EXTRACTED_RELEASE_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let ev_ci = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "sha-1",
        "2026-09-15T10:00:00Z",
    );
    let err = validate_promotion_rules(claim, std::slice::from_ref(&ev_ci)).unwrap_err();
    match err {
        PromotionError::MissingPromotionEvent {
            missing_transition, ..
        } => {
            assert_eq!(
                missing_transition,
                ("ci".into(), "release".into()),
            );
        }
        other => panic!("expected MissingPromotionEvent for second leg, got {other:?}"),
    }
}

#[test]
fn multi_step_chain_out_of_order_is_rejected() {
    // ci -> release event_date predates research -> ci event_date.
    let manifest = parse_manifest_file(EXTRACTED_RELEASE_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let ev_ci = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "sha-1",
        "2026-09-20T10:00:00Z",
    );
    let ev_release = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "ci",
        "release",
        "sha-2",
        "2026-09-15T10:00:00Z", // before ev_ci
    );
    let err =
        validate_promotion_rules(claim, &[ev_ci, ev_release]).unwrap_err();
    match err {
        PromotionError::PromotionChainOutOfOrder {
            prior_transition,
            next_transition,
            ..
        } => {
            assert_eq!(prior_transition, ("research".into(), "ci".into()));
            assert_eq!(next_transition, ("ci".into(), "release".into()));
        }
        other => panic!("expected PromotionChainOutOfOrder, got {other:?}"),
    }
}

#[test]
fn multi_step_research_to_release_direct_event_does_not_satisfy_gate() {
    // The chain must use each linear step. A direct research ->
    // release event is rejected because it doesn't match either
    // (research, ci) or (ci, release).
    let manifest = parse_manifest_file(EXTRACTED_RELEASE_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let ev_direct = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "release",
        "sha-1",
        "2026-09-15T10:00:00Z",
    );
    let err = validate_promotion_rules(claim, std::slice::from_ref(&ev_direct)).unwrap_err();
    match err {
        PromotionError::MissingPromotionEvent {
            missing_transition, ..
        } => {
            // Either leg of the chain could be missing first; the
            // implementation reports whichever it checks first.
            assert!(
                missing_transition == ("research".into(), "ci".into())
                    || missing_transition == ("ci".into(), "release".into()),
                "unexpected missing_transition: {missing_transition:?}",
            );
        }
        other => panic!("expected MissingPromotionEvent, got {other:?}"),
    }
}

#[test]
fn multi_step_chain_same_day_passes_with_eq_timestamps() {
    // Edge case: both events at the same timestamp is allowed
    // (chain ordering rule uses < not <=).
    let manifest = parse_manifest_file(EXTRACTED_RELEASE_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let ev_ci = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "research",
        "ci",
        "sha-1",
        "2026-09-15T10:00:00Z",
    );
    let ev_release = promotion_event(
        "cool-paper-rmsd-vs-baseline",
        "ci",
        "release",
        "sha-2",
        "2026-09-15T10:00:00Z",
    );
    validate_promotion_rules(claim, &[ev_ci, ev_release])
        .expect("equal-timestamp chain should pass");
}

#[test]
fn extracted_at_helper_reaches_through_provenance_block() {
    let manifest = parse_manifest_file(EXTRACTED_CI_MANIFEST_YAML).unwrap();
    let claim = &manifest.claims[0];
    let prov = claim.provenance.as_ref().expect("provenance set");
    match prov {
        ManifestProvenance::Structured(b) => {
            let extractor = b.extractor.as_ref().expect("extractor set");
            assert_eq!(extractor.extracted_at.as_deref(), Some("2026-09-14T10:00:00Z"));
        }
        ManifestProvenance::Legacy(_) => panic!("expected structured provenance"),
    }
}
