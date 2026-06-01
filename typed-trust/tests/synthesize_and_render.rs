//! End-to-end tests for `synthesize()` + `render_augmented()`.
//!
//! Translator produces a Claim + Criteria + Evidence from a YAML
//! manifest. Synthesizer turns that into a TrustReport with
//! per-criterion Pass/Fail computed from the rerun observations.
//! Renderer adds consumer convenience fields and writes a golden
//! file to tests/fixtures/ for inspection.

use std::fs;
use std::path::Path;

use typed_trust::translate::{
    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
    TranslationContext,
};
use typed_trust::*;

/// proteon-style release-tier claim. Hand-crafted YAML matching the
/// real proteon SASA release shape (one oracle to keep `against`
/// translation working) with last_verified populated.
const PROTEON_SASA_RELEASE_YAML: &str = r#"
claims:
  - id: proteon-sasa-vs-biopython-release-1k-pdbs
    title: Proteon SASA tracks Biopython on 1000 PDBs
    kind: measurement
    subsystem: sasa
    case: claims/sasa.md
    source: ..
    tier: release
    trust_strategy:
      - validation
    claim: >
      Across 1000 PDBs proteon total SASA agrees with Biopython.
    tolerances:
      - metric: median_relative_error
        op: "<"
        value: 0.005
        output: total_sasa
        prose: |
          Median(|proteon - biopython| / biopython) < 0.005
    evidence:
      oracle:
        - Biopython
      command: python validation/run_validation.py --n-structures 1000
      artifact: validation/results.json
    provenance: human
    last_verified:
      commit: "4d6ddbec"
      date: "2026-05-11"
      value: 0.0017
      corpus_sha: "b319c47c"
    assumptions: []
    failure_modes: []
"#;

/// Same claim but with last_verified value set to 0.01 (over the
/// 0.005 tolerance) — synthesis should produce Fail.
const PROTEON_SASA_FAILING_YAML: &str = r#"
claims:
  - id: proteon-sasa-failing-hypothetical
    title: Hypothetical failing variant
    kind: measurement
    subsystem: sasa
    case: x.md
    source: ..
    tier: release
    trust_strategy:
      - validation
    claim: hypothetical
    tolerances:
      - metric: median_relative_error
        op: "<"
        value: 0.005
        output: total_sasa
        prose: median_relative_error < 0.005
    evidence:
      oracle:
        - Biopython
      command: python validation/run_validation.py
      artifact: validation/results.json
    provenance: human
    last_verified:
      commit: "abc"
      date: "2026-05-11"
      value: 0.012
      corpus_sha: "xyz"
    assumptions: []
    failure_modes: []
"#;

fn ctx() -> TranslationContext {
    TranslationContext {
        now: "2026-06-01T00:00:00Z".into(),
        manifest_path: "proteon/evident/claims/sasa.yaml".into(),
    }
}

fn translate_to_pieces(yaml: &str) -> (Claim, Vec<typed_trust::translate::TranslatedCriterion>, Evidence) {
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let claim_attested = translate_claim(&ctx(), mc, "claims[0]").unwrap();
    let criteria = translate_tolerances(mc).unwrap();
    let evidence = translate_evidence(&ctx(), mc, &criteria).unwrap();
    (claim_attested.value, criteria, evidence)
}

#[test]
fn synthesize_pass_when_observed_value_meets_tolerance() {
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);

    let report = synthesize(
        claim.id.clone(),
        criteria,
        &[evidence],
        &[],
        &[],
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Current);
    assert_eq!(report.criteria.len(), 1);
    assert_eq!(report.criteria[0].result.value, CriterionResult::Pass);
    assert!(report.challenges.is_empty());
}

#[test]
fn synthesize_fail_when_observed_value_exceeds_tolerance() {
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_FAILING_YAML);

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        &[],
        &[],
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.criteria[0].result.value, CriterionResult::Fail);
    // No challenges, so a Fail does not by itself produce Contested —
    // the status field reflects review state, not pass/fail of the
    // criterion.
    assert_eq!(report.status, RenderStatus::Current);
}

#[test]
fn synthesize_not_assessed_when_evidence_has_no_observations() {
    // Strip last_verified so the rerun has no observations.
    let yaml = PROTEON_SASA_RELEASE_YAML.replace(
        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
    );
    let (claim, criteria, evidence) = translate_to_pieces(&yaml);

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        &[],
        &[],
        "2026-06-01T00:00:00Z".into(),
    );

    let r = &report.criteria[0].result.value;
    assert!(matches!(r, CriterionResult::NotAssessed { .. }), "got {r:?}");
}

#[test]
fn synthesize_contested_when_substantive_challenge_targets_criterion() {
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let crit_id = criteria[0].id.clone();

    // A substantive challenge backed by a SUSTAINED claim (Current status)
    // → Currency moves to Contested per invariant 6.
    let backing_id = ClaimId::new("backing-claim");
    let challenge = ReviewEvent {
        id: EventId::new("rev-challenge-1"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("proteon-peer-review-v1".into()),
        rationale: "Tolerance too lax for this corpus.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(backing_id.clone()),
        },
    };

    // A backing report whose status is Current sustains the challenge.
    let backing_report = TrustReport {
        claim: backing_id,
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        &[challenge.clone()],
        std::slice::from_ref(&backing_report),
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Contested);
    assert_eq!(report.challenges, vec![EventId::new("rev-challenge-1")]);
}

#[test]
fn synthesize_substantive_challenge_with_missing_backing_does_not_contest() {
    // Codex review #2 / invariant 6: if the backed_by claim has no
    // matching backing report (or the report is not Current), the
    // challenge does NOT sustain and status stays Current.
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let crit_id = criteria[0].id.clone();

    let challenge = ReviewEvent {
        id: EventId::new("rev-no-backing"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("proteon-peer-review-v1".into()),
        rationale: "Backing claim doesn't actually exist in our world.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(ClaimId::new("nonexistent-backing-claim")),
        },
    };

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        std::slice::from_ref(&challenge),
        &[], // no backing reports supplied
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Current);
}

#[test]
fn synthesize_substantive_challenge_with_contested_backing_does_not_contest() {
    // The backed_by claim's TrustReport has status != Current → the
    // challenge does NOT sustain the parent.
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let crit_id = criteria[0].id.clone();

    let backing_id = ClaimId::new("contested-backing");
    let challenge = ReviewEvent {
        id: EventId::new("rev-contested-backing"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("proteon-peer-review-v1".into()),
        rationale: "Backing claim itself is contested.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(backing_id.clone()),
        },
    };

    let contested_backing = TrustReport {
        claim: backing_id,
        status: RenderStatus::Contested,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        std::slice::from_ref(&challenge),
        std::slice::from_ref(&contested_backing),
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Current);
}

#[test]
fn synthesize_unbacked_substantive_challenge_does_not_move_status() {
    // Unbacked WeakStatistics challenge — informational, doesn't move
    // status (invariant 6).
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let crit_id = criteria[0].id.clone();

    let challenge = ReviewEvent {
        id: EventId::new("rev-informational"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("proteon-peer-review-v1".into()),
        rationale: "Concern noted but no backing claim authored yet.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: None,
        },
    };

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        &[challenge],
        &[],
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Current);
}

#[test]
fn synthesize_procedural_challenge_moves_status_without_backing() {
    // HashMismatch is in the closed procedural list → moves status
    // even without backing (invariant 6).
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let crit_id = criteria[0].id.clone();

    let challenge = ReviewEvent {
        id: EventId::new("rev-hash-mismatch"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Automated,
            name: "release-verifier".into(),
            details: vec![],
        },
        protocol: Some("release-integrity-check".into()),
        rationale: "Artifact sha256 does not match manifest corpus_sha.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::HashMismatch,
            backed_by: None,
        },
    };

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        &[challenge],
        &[],
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Contested);
}

#[test]
fn render_augmented_adds_observed_value_and_criterion_status() {
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let evidence_vec = vec![evidence];

    let report = synthesize(
        claim.id,
        criteria,
        &evidence_vec,
        &[],
        &[],
        "2026-06-01T00:00:00Z".into(),
    );

    let json = render_augmented(&RenderInput {
        report: &report,
        evidence: &evidence_vec,
        related_events: &[],
        backing_reports: &[],
    });

    let crit0 = &json["criteria"][0];
    assert_eq!(crit0["result"]["observed_value"], 0.0017);
    assert_eq!(crit0["result"]["criterion_status"], "current");
    assert!(crit0["result"].get("contested_by").is_none());

    // No _graph block when there are no related events or backing reports.
    assert!(json.get("_graph").is_none());

    write_fixture(
        "augmented_sasa_release.trustreport.json",
        &serde_json::to_string_pretty(&json).unwrap(),
    );
}

#[test]
fn render_augmented_contested_includes_graph_and_contested_by() {
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let evidence_vec = vec![evidence];
    let crit_id = criteria[0].id.clone();
    let backing_id = ClaimId::new("backing-claim-id");

    let challenge = ReviewEvent {
        id: EventId::new("rev-challenge-electrostatic"),
        target: Target::Criterion(crit_id.clone()),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![IdentityDetail {
                key: "orcid".into(),
                value: "0000-0000-0000-0001".into(),
            }],
        },
        protocol: Some("proteon-peer-review-v1".into()),
        rationale: "Tolerance band too wide.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(backing_id.clone()),
        },
    };

    let backing_report = TrustReport {
        claim: backing_id,
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };

    let report = synthesize(
        claim.id,
        criteria,
        &evidence_vec,
        std::slice::from_ref(&challenge),
        std::slice::from_ref(&backing_report),
        "2026-06-01T00:00:00Z".into(),
    );

    let json = render_augmented(&RenderInput {
        report: &report,
        evidence: &evidence_vec,
        related_events: std::slice::from_ref(&challenge),
        backing_reports: std::slice::from_ref(&backing_report),
    });

    // Report-level Contested.
    assert_eq!(json["status"], "contested");
    assert_eq!(json["challenges"][0], "rev-challenge-electrostatic");

    // Per-criterion render-aux.
    let crit0 = &json["criteria"][0];
    assert_eq!(crit0["result"]["criterion_status"], "contested");
    assert_eq!(
        crit0["result"]["contested_by"][0],
        "rev-challenge-electrostatic"
    );

    // _graph block carries the related event inline.
    assert_eq!(
        json["_graph"]["review_events"][0]["id"],
        "rev-challenge-electrostatic"
    );
    assert_eq!(
        json["_graph"]["review_events"][0]["kind"]["type"],
        "challenge"
    );

    write_fixture(
        "augmented_contested.trustreport.json",
        &serde_json::to_string_pretty(&json).unwrap(),
    );
}

fn write_fixture(filename: &str, body: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join(filename), body).expect("write fixture");
}

// ---------- Recursive backing-report synthesis ----------

use std::collections::HashMap;

struct InMemoryLookup {
    claims: HashMap<ClaimId, BackingClaimInputs>,
}

impl ClaimLookup for InMemoryLookup {
    fn lookup(&self, claim_id: &ClaimId) -> Option<BackingClaimInputs> {
        self.claims.get(claim_id).cloned()
    }
}

fn empty_inputs() -> BackingClaimInputs {
    BackingClaimInputs {
        criteria: vec![],
        evidence: vec![],
        review_events: vec![],
    }
}

fn challenge_targeting_any(backed_by: Option<ClaimId>) -> ReviewEvent {
    ReviewEvent {
        id: EventId::new("rev-x"),
        target: Target::Claim(ClaimId::new("dummy-target")),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("p".into()),
        rationale: "r".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by,
        },
    }
}

#[test]
fn compute_backing_reports_returns_empty_when_no_backed_events() {
    let lookup = InMemoryLookup {
        claims: HashMap::new(),
    };
    let challenge = challenge_targeting_any(None);
    let backing = compute_backing_reports(
        std::slice::from_ref(&challenge),
        &lookup,
        "2026-06-01T00:00:00Z",
        5,
    );
    assert!(backing.is_empty());
}

#[test]
fn compute_backing_reports_synthesizes_one_backing_claim() {
    let backing_id = ClaimId::new("backing-claim-1");
    let mut claims = HashMap::new();
    claims.insert(backing_id.clone(), empty_inputs());
    let lookup = InMemoryLookup { claims };

    let challenge = challenge_targeting_any(Some(backing_id.clone()));
    let backing = compute_backing_reports(
        std::slice::from_ref(&challenge),
        &lookup,
        "2026-06-01T00:00:00Z",
        5,
    );

    assert_eq!(backing.len(), 1);
    assert_eq!(backing[0].claim, backing_id);
    // No criteria, no events on the backing claim → empty Current report.
    assert_eq!(backing[0].status, RenderStatus::Current);
    assert!(backing[0].criteria.is_empty());
}

#[test]
fn compute_backing_reports_detects_cycles() {
    // claim-A is backed by claim-B; claim-B is backed by claim-A.
    let a = ClaimId::new("claim-A");
    let b = ClaimId::new("claim-B");

    let a_event = challenge_targeting_any(Some(b.clone()));
    let b_event = challenge_targeting_any(Some(a.clone()));

    let mut claims = HashMap::new();
    claims.insert(
        a.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![a_event.clone()],
        },
    );
    claims.insert(
        b.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![b_event],
        },
    );
    let lookup = InMemoryLookup { claims };

    // Start with a challenge targeting A; A backs B; B backs A.
    let backing = compute_backing_reports(
        std::slice::from_ref(&a_event),
        &lookup,
        "2026-06-01T00:00:00Z",
        10,
    );

    // Each claim visited at most once.
    assert_eq!(backing.len(), 2);
    let ids: Vec<&ClaimId> = backing.iter().map(|r| &r.claim).collect();
    assert!(ids.contains(&&b));
    assert!(ids.contains(&&a));
}

#[test]
fn compute_backing_reports_respects_max_depth() {
    // Chain: claim-0 → claim-1 → claim-2 → claim-3 (no cycles).
    let ids: Vec<ClaimId> = (0..4).map(|i| ClaimId::new(format!("claim-{i}"))).collect();
    let mut claims = HashMap::new();
    for i in 0..3 {
        let event = challenge_targeting_any(Some(ids[i + 1].clone()));
        claims.insert(
            ids[i].clone(),
            BackingClaimInputs {
                criteria: vec![],
                evidence: vec![],
                review_events: vec![event],
            },
        );
    }
    claims.insert(ids[3].clone(), empty_inputs());
    let lookup = InMemoryLookup { claims };

    // Initial event points at claim-0, which begins the chain.
    let initial = challenge_targeting_any(Some(ids[0].clone()));

    // max_depth = 2: should synthesize claim-0 (depth 0) and claim-1
    // (depth 1) only. Walking is depth-first now (children before
    // parent) so the synthesized order is claim-1 then claim-0 — the
    // set is what matters, not the order.
    let backing = compute_backing_reports(
        std::slice::from_ref(&initial),
        &lookup,
        "2026-06-01T00:00:00Z",
        2,
    );

    assert_eq!(backing.len(), 2);
    let visited_ids: Vec<&ClaimId> = backing.iter().map(|r| &r.claim).collect();
    assert!(visited_ids.contains(&&ids[0]));
    assert!(visited_ids.contains(&&ids[1]));
    assert!(!visited_ids.contains(&&ids[2]));
    assert!(!visited_ids.contains(&&ids[3]));
}
