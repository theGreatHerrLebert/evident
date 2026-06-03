//! Translator-level tests for the `--review-events-sidecar` overlay
//! (Phase 2a).
//!
//! These exercise `translate_review_event` and `canonical_event_id`
//! directly; CLI-level integration (unknown-claim-id rejection,
//! per-claim grouping, aux-decoration) is covered by the end-to-end
//! Python tests on the agent side.

use typed_trust::translate::{
    canonical_event_id, translate_review_event, ManifestReviewAuthor, ManifestReviewEvent,
    ReviewTranslateError,
};

fn endorse_event() -> ManifestReviewEvent {
    ManifestReviewEvent {
        claim_id: "proteon-sasa-vs-biopython-release-1k-pdbs".into(),
        kind: "endorse".into(),
        author: ManifestReviewAuthor {
            kind: "model".into(),
            name: "claude-opus-4-7".into(),
            version: Some("20250101".into()),
            context: Some("evident-agent review v0.2a".into()),
            orcid: None,
            affiliation: None,
        },
        rationale: "Cited evidence supports the claim within tolerance.".into(),
        timestamp: "2026-06-02T10:31:44Z".into(),
        event_id: None,
        checks: None,
        observed_value: Some("0.008".into()),
        tolerance: Some("< 0.02".into()),
        failure_reason: None,
        challenge: None,
        target: None,
        supersede: None,
        promote_from_extracted: None,
        protocol: None,
    }
}

#[test]
fn translates_endorse_with_model_author_and_canonical_event_id() {
    let entry = endorse_event();
    let event = translate_review_event(&entry).expect("endorse translates");

    // EventId is the canonical hash since entry.event_id is None.
    assert!(event.id.as_str().starts_with("sha256:"));
    assert_eq!(event.rationale, entry.rationale);
    assert_eq!(event.at, entry.timestamp);
    // Target points back at the claim id.
    use typed_trust::review::Target;
    match &event.target {
        Target::Claim(c) => assert_eq!(c.as_str(), entry.claim_id),
        other => panic!("unexpected target {other:?}"),
    }

    // Identity is preserved as Model kind with the version detail.
    use typed_trust::identity::IdentityKind;
    assert!(matches!(event.by.kind, IdentityKind::Model));
    assert_eq!(event.by.name, "claude-opus-4-7");
    let version = event
        .by
        .details
        .iter()
        .find(|d| d.key == "version")
        .expect("version detail present");
    assert_eq!(version.value, "20250101");
}

#[test]
fn translates_dissent_kind() {
    let mut entry = endorse_event();
    entry.kind = "dissent".into();
    entry.observed_value = None;
    entry.failure_reason = Some("tolerance violated on residue 47".into());
    let event = translate_review_event(&entry).expect("dissent translates");
    use typed_trust::review::ReviewKind;
    assert!(matches!(event.kind, ReviewKind::Dissent));
}

// ---------- Phase 2b: Challenge translation ----------

use typed_trust::translate::{ManifestChallengeBlock, ManifestClaim, ManifestEvidence, ManifestTolerance, ManifestViolation};

fn substantive_backing_claim(id: &str) -> ManifestClaim {
    ManifestClaim {
        id: id.into(),
        title: "Counter-evidence for the target tolerance".into(),
        kind: "measurement".into(),
        case: None,
        source: Some(".".into()),
        tier: "ci".into(),
        claim: "Counter: observed value 0.025 exceeds bound 0.02.".into(),
        tolerances: Some(vec![ManifestTolerance {
            metric: Some("electrostatic_error".into()),
            op: Some(">".into()),
            value: Some(0.02),
            output: None,
            prose: "Counter-claim: observed exceeds upper bound.".into(),
        }]),
        evidence: Some(ManifestEvidence {
            oracle: vec!["BALL".into()],
            command: "pytest".into(),
            artifact: "results.csv".into(),
            replay_status: None,
            replay_reason: None,
        }),
        provenance: None,
        last_verified: None,
        assumptions: None,
        failure_modes: None,
        metadata: None,
    }
}

fn substantive_violation() -> ManifestViolation {
    ManifestViolation {
        metric: "electrostatic_error".into(),
        observed_value: 0.025,
        bound: 0.02,
        comparator: "<".into(),
        citation: "row 47 of results.csv".into(),
    }
}

#[test]
fn translates_substantive_challenge_with_backing_claim() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = Some(ManifestChallengeBlock {
        category: "weak_statistics".into(),
        target_criterion_id: Some("electrostatic_error".into()),
        violation: Some(substantive_violation()),
        backing_claim: Some(substantive_backing_claim(
            "proteon-sasa-vs-biopython-release-1k-pdbs-counter-12345678",
        )),
    });
    let event = translate_review_event(&entry).expect("substantive challenge translates");
    use typed_trust::review::{ChallengeCategory, ReviewKind};
    match &event.kind {
        ReviewKind::Challenge { category, backed_by } => {
            assert!(matches!(category, ChallengeCategory::WeakStatistics));
            let bid = backed_by.as_ref().expect("backed_by populated");
            assert!(bid.as_str().ends_with("-counter-12345678"));
        }
        other => panic!("unexpected kind {other:?}"),
    }
}

#[test]
fn translates_procedural_challenge_without_backing_claim() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = Some(ManifestChallengeBlock {
        category: "command_failure".into(),
        target_criterion_id: Some("electrostatic_error".into()),
        violation: None,
        backing_claim: None,
    });
    let event = translate_review_event(&entry).expect("procedural challenge translates");
    use typed_trust::review::{ChallengeCategory, ReviewKind};
    match &event.kind {
        ReviewKind::Challenge { category, backed_by } => {
            assert!(matches!(category, ChallengeCategory::CommandFailure));
            assert!(backed_by.is_none());
        }
        other => panic!("unexpected kind {other:?}"),
    }
}

#[test]
fn rejects_challenge_without_challenge_block() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = None;
    let err = translate_review_event(&entry).expect_err("challenge without block must be rejected");
    assert!(matches!(
        err,
        ReviewTranslateError::ChallengeMissingBlock { .. }
    ));
}

#[test]
fn rejects_substantive_challenge_without_backing_claim() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = Some(ManifestChallengeBlock {
        category: "weak_statistics".into(),
        target_criterion_id: Some("electrostatic_error".into()),
        violation: Some(substantive_violation()),
        backing_claim: None,
    });
    let err =
        translate_review_event(&entry).expect_err("substantive without backing must be rejected");
    assert!(matches!(
        err,
        ReviewTranslateError::SubstantiveChallengeMissingBacking { .. }
    ));
}

#[test]
fn rejects_procedural_challenge_with_backing_claim() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = Some(ManifestChallengeBlock {
        category: "command_failure".into(),
        target_criterion_id: None,
        violation: None,
        backing_claim: Some(substantive_backing_claim("anything")),
    });
    let err = translate_review_event(&entry)
        .expect_err("procedural with backing must be rejected (overshoot)");
    assert!(matches!(
        err,
        ReviewTranslateError::ProceduralChallengeWithBacking { .. }
    ));
}

#[test]
fn rejects_backing_claim_with_id_matching_target() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = Some(ManifestChallengeBlock {
        category: "weak_statistics".into(),
        target_criterion_id: Some("electrostatic_error".into()),
        violation: Some(substantive_violation()),
        backing_claim: Some(substantive_backing_claim(
            "proteon-sasa-vs-biopython-release-1k-pdbs",
        )),
    });
    let err =
        translate_review_event(&entry).expect_err("self-cycle backing must be rejected");
    assert!(matches!(
        err,
        ReviewTranslateError::BackingClaimMatchesTargetId { .. }
    ));
}

#[test]
fn rejects_substantive_challenge_without_violation_codex_2b_cr1() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = Some(ManifestChallengeBlock {
        category: "weak_statistics".into(),
        target_criterion_id: Some("electrostatic_error".into()),
        violation: None,
        backing_claim: Some(substantive_backing_claim(
            "proteon-sasa-vs-biopython-release-1k-pdbs-counter-deadbeef",
        )),
    });
    let err = translate_review_event(&entry)
        .expect_err("substantive challenge with backing but no violation must be rejected");
    assert!(matches!(
        err,
        ReviewTranslateError::SubstantiveChallengeMissingViolation { .. }
    ));
}

#[test]
fn rejects_substantive_challenge_without_target_criterion_codex_2b_cr1() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = Some(ManifestChallengeBlock {
        category: "weak_statistics".into(),
        target_criterion_id: None,
        violation: Some(substantive_violation()),
        backing_claim: Some(substantive_backing_claim(
            "proteon-sasa-vs-biopython-release-1k-pdbs-counter-feedface",
        )),
    });
    let err = translate_review_event(&entry).expect_err(
        "substantive challenge without target_criterion_id must be rejected for audit",
    );
    assert!(matches!(
        err,
        ReviewTranslateError::SubstantiveChallengeMissingTargetCriterion { .. }
    ));
}

#[test]
fn unknown_category_translates_to_other_substantive() {
    let mut entry = endorse_event();
    entry.kind = "challenge".into();
    entry.challenge = Some(ManifestChallengeBlock {
        category: "domain_specific_concern".into(),
        target_criterion_id: Some("electrostatic_error".into()),
        violation: Some(substantive_violation()),
        backing_claim: Some(substantive_backing_claim(
            "proteon-sasa-vs-biopython-release-1k-pdbs-counter-99999999",
        )),
    });
    let event = translate_review_event(&entry).expect("unknown category accepted as Other");
    use typed_trust::review::{ChallengeCategory, ReviewKind};
    match &event.kind {
        ReviewKind::Challenge { category, .. } => match category {
            ChallengeCategory::Other(s) => assert_eq!(s, "domain_specific_concern"),
            other => panic!("expected Other, got {other:?}"),
        },
        other => panic!("unexpected kind {other:?}"),
    }
}

#[test]
fn rejects_unknown_kind() {
    let mut entry = endorse_event();
    entry.kind = "applaud".into();
    let err = translate_review_event(&entry).expect_err("unknown kind must be rejected");
    assert!(matches!(err, ReviewTranslateError::UnknownKind { .. }));
}

#[test]
fn rejects_unknown_author_kind() {
    let mut entry = endorse_event();
    entry.author.kind = "wizard".into();
    let err = translate_review_event(&entry).expect_err("unknown author kind rejected");
    assert!(matches!(
        err,
        ReviewTranslateError::UnknownAuthorKind { .. }
    ));
}

#[test]
fn rejects_model_author_without_version() {
    let mut entry = endorse_event();
    entry.author.version = None;
    let err = translate_review_event(&entry).expect_err("model without version rejected");
    assert!(matches!(
        err,
        ReviewTranslateError::ModelMissingVersion { .. }
    ));
}

#[test]
fn explicit_event_id_overrides_canonical_hash() {
    let mut entry = endorse_event();
    entry.event_id = Some("my-explicit-id".into());
    let event = translate_review_event(&entry).expect("translates");
    assert_eq!(event.id.as_str(), "my-explicit-id");
}

#[test]
fn canonical_event_id_is_stable_across_identical_payloads() {
    let a = endorse_event();
    let b = endorse_event();
    assert_eq!(canonical_event_id(&a), canonical_event_id(&b));
}

#[test]
fn canonical_event_id_changes_when_rationale_changes() {
    let a = endorse_event();
    let mut b = endorse_event();
    b.rationale = "different rationale".into();
    assert_ne!(canonical_event_id(&a), canonical_event_id(&b));
}

#[test]
fn canonical_event_id_changes_when_timestamp_changes() {
    let a = endorse_event();
    let mut b = endorse_event();
    b.timestamp = "2027-01-01T00:00:00Z".into();
    assert_ne!(canonical_event_id(&a), canonical_event_id(&b));
}

#[test]
fn canonical_event_id_disambiguates_same_tuple_different_payload() {
    // Same (claim_id, author, kind, timestamp) tuple but different
    // observed_value. The old-style id-from-tuple would have collided;
    // canonical hash distinguishes them.
    let a = endorse_event();
    let mut b = endorse_event();
    b.observed_value = Some("0.009".into());
    assert_ne!(canonical_event_id(&a), canonical_event_id(&b));
}

#[test]
fn deserializes_sidecar_shape_round_trip() {
    let json = r#"{
        "events": [
            {
                "event_id": "sha256:abcd",
                "claim_id": "claim-A",
                "kind": "endorse",
                "author": {
                    "kind": "model",
                    "name": "claude-opus-4-7",
                    "version": "20250101"
                },
                "rationale": "Looks good across the digest. Spot-checked three rows. No outliers above tolerance.",
                "timestamp": "2026-06-02T10:31:44Z",
                "checks": {
                    "metric_present": "pass",
                    "within_tolerance": "pass",
                    "outliers_checked": "pass",
                    "reproducible_chain": "pass"
                },
                "observed_value": "0.008",
                "tolerance": "< 0.02"
            }
        ]
    }"#;
    use typed_trust::translate::ReviewEventSidecar;
    let parsed: ReviewEventSidecar =
        serde_json::from_str(json).expect("sidecar deserializes");
    assert_eq!(parsed.events.len(), 1);
    let e = &parsed.events[0];
    assert_eq!(e.claim_id, "claim-A");
    assert_eq!(e.event_id.as_deref(), Some("sha256:abcd"));
    assert_eq!(e.checks.as_ref().and_then(|c| c.get("metric_present")).and_then(|v| v.as_str()), Some("pass"));
}

// ============================================================
// Phase 2d-i: translator schema extension tests
// ============================================================

use typed_trust::translate::{ManifestSupersedeBlock, ManifestTargetBlock};

fn supersede_event() -> ManifestReviewEvent {
    ManifestReviewEvent {
        claim_id: "ball-electrostatic-ci".into(),
        kind: "supersede".into(),
        author: ManifestReviewAuthor {
            kind: "model".into(),
            name: "claude-opus-4-7".into(),
            version: Some("20260601".into()),
            context: None,
            orcid: None,
            affiliation: None,
        },
        rationale: "Re-reviewed the digest; cited value is a known artifact. Withdraw prior Challenge.".into(),
        timestamp: "2026-06-15T09:00:00Z".into(),
        event_id: None,
        checks: None,
        observed_value: None,
        tolerance: None,
        failure_reason: None,
        challenge: None,
        target: Some(ManifestTargetBlock {
            kind: "review_event".into(),
            id: "sha256:prior-challenge-id".into(),
        }),
        supersede: Some(ManifestSupersedeBlock {
            successor: "att-successor-id".into(),
        }),
        promote_from_extracted: None,
        protocol: None,
    }
}

#[test]
fn phase2d_translates_supersede_with_review_event_target() {
    let entry = supersede_event();
    let event = translate_review_event(&entry).expect("supersede translates");
    use typed_trust::review::{ReviewKind, Target};
    match &event.target {
        Target::ReviewEvent(eid) => assert_eq!(eid.as_str(), "sha256:prior-challenge-id"),
        other => panic!("expected Target::ReviewEvent, got {other:?}"),
    }
    match &event.kind {
        ReviewKind::Supersede { successor } => {
            assert_eq!(successor.as_str(), "att-successor-id");
        }
        other => panic!("expected Supersede, got {other:?}"),
    }
}

#[test]
fn phase2d_rejects_supersede_without_target() {
    let mut entry = supersede_event();
    entry.target = None;
    let err = translate_review_event(&entry).expect_err("supersede without target must reject");
    assert!(
        matches!(err, ReviewTranslateError::SupersedeMissingTarget { .. }),
        "expected SupersedeMissingTarget, got {err:?}"
    );
}

#[test]
fn phase2d_rejects_supersede_without_successor() {
    let mut entry = supersede_event();
    entry.supersede = None;
    let err = translate_review_event(&entry).expect_err("supersede without successor must reject");
    assert!(
        matches!(err, ReviewTranslateError::SupersedeMissingSuccessor { .. }),
        "expected SupersedeMissingSuccessor, got {err:?}"
    );
}

#[test]
fn phase2d_rejects_supersede_with_empty_successor() {
    let mut entry = supersede_event();
    entry.supersede = Some(ManifestSupersedeBlock {
        successor: "   ".into(),
    });
    let err = translate_review_event(&entry).expect_err("empty successor must reject");
    assert!(matches!(err, ReviewTranslateError::SupersedeMissingSuccessor { .. }));
}

#[test]
fn phase2d_rejects_unsupported_target_type() {
    // Phase 2d-i scope: only claim + review_event are accepted.
    // CriterionResult, Evidence, etc. require schema extensions
    // deferred to Phase 2e+ (codex F-2D-13).
    let mut entry = supersede_event();
    entry.target = Some(ManifestTargetBlock {
        kind: "criterion_result".into(),
        id: "x".into(),
    });
    let err = translate_review_event(&entry).expect_err("unsupported target must reject");
    assert!(
        matches!(&err, ReviewTranslateError::UnsupportedTargetType { target_type, .. } if target_type == "criterion_result"),
        "expected UnsupportedTargetType, got {err:?}"
    );
}

#[test]
fn phase2d_endorse_with_explicit_claim_target_translates_to_claim() {
    // Explicit target.type=claim works the same as no target block.
    let mut entry = endorse_event();
    entry.target = Some(ManifestTargetBlock {
        kind: "claim".into(),
        id: "explicit-claim-id".into(),
    });
    let event = translate_review_event(&entry).expect("explicit claim target translates");
    use typed_trust::review::Target;
    match &event.target {
        Target::Claim(c) => assert_eq!(c.as_str(), "explicit-claim-id"),
        other => panic!("expected Target::Claim, got {other:?}"),
    }
}

#[test]
fn phase2d_pre_2d_sidecar_canonical_event_id_unchanged() {
    // Codex F-2D-5: pre-2d sidecars (no target/supersede fields)
    // must canonicalize to the same bytes as before. The existing
    // canonical_event_id_is_stable_across_identical_payloads test
    // covers stability; this one explicitly asserts that adding
    // None values doesn't change the hash, which is the parity
    // property.
    let entry = endorse_event();
    let id1 = canonical_event_id(&entry);

    // Building the "same" entry with explicit None for the new
    // fields should canonicalize identically.
    let mut entry2 = endorse_event();
    entry2.target = None;
    entry2.supersede = None;
    let id2 = canonical_event_id(&entry2);
    assert_eq!(id1, id2);

    // Adding a target block CHANGES the hash (because it's a
    // semantically distinguishing field).
    entry2.target = Some(ManifestTargetBlock {
        kind: "claim".into(),
        id: "explicit-claim-id".into(),
    });
    let id3 = canonical_event_id(&entry2);
    assert_ne!(id1, id3);
}
