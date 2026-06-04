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
    let evidence = translate_evidence(&ctx(), mc, &criteria).unwrap().unwrap();
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
        &std::collections::HashSet::new(),
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
        &std::collections::HashSet::new(),
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
        &std::collections::HashSet::new(),
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

    // A backing report whose status is Current AND has Pass criteria
    // sustains the challenge per the §8 "passing-criteria result" rule.
    let backing_report = make_passing_backing(backing_id);

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        &[challenge.clone()],
        std::slice::from_ref(&backing_report),
        &std::collections::HashSet::new(),
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
        &std::collections::HashSet::new(),
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
        &std::collections::HashSet::new(),
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
        &std::collections::HashSet::new(),
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Current);
}

#[test]
fn criterion_result_targeted_event_is_consistent_across_synth_and_render() {
    // Codex round 4: render previously matched Target::CriterionResult
    // by criterion id alone while synthesize treated it as
    // non-matching, producing a contested criterion in a Current
    // report. Both sides now refuse to match CriterionResult until
    // TrustReport carries a ReportId; the report and its criteria
    // must agree on contestation status.
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let evidence_vec = vec![evidence];
    let crit_id = criteria[0].id.clone();

    let event = ReviewEvent {
        id: EventId::new("rev-criterion-result-snapshot"),
        target: Target::CriterionResult {
            report: ReportId::new("some-snapshot"),
            criterion: crit_id,
        },
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("p".into()),
        rationale: "Snapshot-bound objection.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: None,
        },
    };

    let report = synthesize(
        claim.id.clone(),
        criteria,
        &evidence_vec,
        std::slice::from_ref(&event),
        &[],
        &std::collections::HashSet::new(),
        "2026-06-01T00:00:00Z".into(),
    );

    let json = render_augmented(&RenderInput {
        report: &report,
        evidence: &evidence_vec,
        related_events: std::slice::from_ref(&event),
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
    });

    // Report-level status is Current (synthesize doesn't match
    // CriterionResult).
    assert_eq!(report.status, RenderStatus::Current);
    assert_eq!(json["status"], "current");

    // Criterion status must agree — render also doesn't match.
    assert_eq!(json["criteria"][0]["result"]["criterion_status"], "current");
    assert!(json["criteria"][0]["result"].get("contested_by").is_none());
}

#[test]
fn synthesize_procedural_challenge_targeting_evidence_moves_status() {
    // Codex review #3 (round 3): procedural challenges naturally
    // target Evidence ids (ArtifactUnavailable, HashMismatch,
    // CommandFailure). target_touches_report must recognize these
    // when the targeted Evidence is part of the report's evidence
    // set — otherwise the status calculation silently drops these
    // events and the report renders Current with no challenge listed.
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let ev_id = evidence.id.clone();

    let challenge = ReviewEvent {
        id: EventId::new("rev-artifact-missing"),
        target: Target::Evidence(ev_id),
        by: Identity {
            kind: IdentityKind::Automated,
            name: "release-verifier".into(),
            details: vec![],
        },
        protocol: Some("release-integrity-check".into()),
        rationale: "validation/results.json not found in release archive.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::ArtifactUnavailable,
            backed_by: None,
        },
    };

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        std::slice::from_ref(&challenge),
        &[],
        &std::collections::HashSet::new(),
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Contested);
    assert_eq!(
        report.challenges,
        vec![EventId::new("rev-artifact-missing")]
    );
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
        &std::collections::HashSet::new(),
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
        &std::collections::HashSet::new(),
        "2026-06-01T00:00:00Z".into(),
    );

    let json = render_augmented(&RenderInput {
        report: &report,
        evidence: &evidence_vec,
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
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

    let backing_report = make_passing_backing(backing_id);

    let report = synthesize(
        claim.id,
        criteria,
        &evidence_vec,
        std::slice::from_ref(&challenge),
        std::slice::from_ref(&backing_report),
        &std::collections::HashSet::new(),
        "2026-06-01T00:00:00Z".into(),
    );

    let json = render_augmented(&RenderInput {
        report: &report,
        evidence: &evidence_vec,
        related_events: std::slice::from_ref(&challenge),
        backing_reports: std::slice::from_ref(&backing_report),
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
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

/// A backing TrustReport that satisfies the sustain rule: status
/// Current AND at least one criterion AND all criteria are Pass.
fn make_passing_backing(claim_id: ClaimId) -> TrustReport {
    let crit_id = CriterionId::new(format!("{}-crit-0", claim_id.as_str()));
    let synth_runner = Identity {
        kind: IdentityKind::Automated,
        name: "evident-synthesizer".into(),
        details: vec![],
    };
    TrustReport {
        claim: claim_id,
        status: RenderStatus::Current,
        criteria: vec![Criterion {
            id: crit_id,
            name: "backing claim's own criterion".into(),
            tolerance: None,
            result: Attested {
                value: CriterionResult::Pass,
                derivation: Derivation::Verified {
                    method: ToolInvocation {
                        command: "rule:Pass".into(),
                        tool_version: "test-fixture".into(),
                        env: vec![],
                    },
                    ran_by: synth_runner,
                    reruns: vec![],
                },
                at: "2026-06-01T00:00:00Z".into(),
            },
        }],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    }
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
    let backing = compute_backing_reports(&ClaimId::new("test-root"),
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
    let backing = compute_backing_reports(&ClaimId::new("test-root"),
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
    let backing = compute_backing_reports(&ClaimId::new("test-root"),
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

    // Per design §8 ("Contested if the graph reachable from it contains
    // a cycle in challenge edges"), every cycled claim must be surfaced
    // as Contested. A cycle cannot be resolved deterministically.
    for r in &backing {
        assert_eq!(r.status, RenderStatus::Contested, "claim {:?}", r.claim);
    }
}

#[test]
fn compute_backing_reports_transitive_reach_to_cycle_is_contested() {
    // initial → X → A → B → A. A and B are on the cycle, X
    // transitively reaches it. Per design §8 "Contested if the graph
    // reachable from it contains a cycle in challenge edges" — X
    // should also be Contested even though it is not in the cycle.
    let x = ClaimId::new("claim-X");
    let a = ClaimId::new("claim-A");
    let b = ClaimId::new("claim-B");

    let x_to_a = challenge_targeting_any(Some(a.clone()));
    let a_to_b = challenge_targeting_any(Some(b.clone()));
    let b_to_a = challenge_targeting_any(Some(a.clone())); // back edge

    let mut claims = HashMap::new();
    claims.insert(
        x.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![x_to_a],
        },
    );
    claims.insert(
        a.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![a_to_b],
        },
    );
    claims.insert(
        b.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![b_to_a],
        },
    );
    let lookup = InMemoryLookup { claims };

    // Initial event backs X (so X is the first backing target).
    let initial = challenge_targeting_any(Some(x.clone()));
    let backing = compute_backing_reports(&ClaimId::new("test-root"),
        std::slice::from_ref(&initial),
        &lookup,
        "2026-06-01T00:00:00Z",
        10,
    );

    let by_id: HashMap<&ClaimId, &TrustReport> =
        backing.iter().map(|r| (&r.claim, r)).collect();

    assert_eq!(by_id.len(), 3, "expected X, A, B in backing");
    assert_eq!(by_id[&x].status, RenderStatus::Contested, "X reaches cycle");
    assert_eq!(by_id[&a].status, RenderStatus::Contested, "A on cycle");
    assert_eq!(by_id[&b].status, RenderStatus::Contested, "B on cycle");
}

#[test]
fn render_criterion_status_agrees_with_synthesize_under_cycle_contestation() {
    // Codex round 6 finding 1: when a criterion-targeted Challenge is
    // backed by a cycled claim, synthesize moves the report Contested
    // via cycle_contested. The renderer must use the same set or the
    // augmented JSON contradicts itself (report contested, criterion
    // current).
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let evidence_vec = vec![evidence];
    let crit_id = criteria[0].id.clone();

    let cycled_id = ClaimId::new("cycled-backing");
    let other_id = ClaimId::new("cycled-backing-mate");

    let challenge = ReviewEvent {
        id: EventId::new("rev-backed-by-cycled"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("p".into()),
        rationale: "Backed by a claim that reaches a cycle.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(cycled_id.clone()),
        },
    };

    // Build cycled-id ↔ other-id cycle in the lookup.
    let mut lookup_claims = HashMap::new();
    lookup_claims.insert(
        cycled_id.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![challenge_targeting_any(Some(other_id.clone()))],
        },
    );
    lookup_claims.insert(
        other_id.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![challenge_targeting_any(Some(cycled_id.clone()))],
        },
    );
    let lookup = InMemoryLookup {
        claims: lookup_claims,
    };

    let cycled = detect_cycle_contested(
        &claim.id,
        std::slice::from_ref(&challenge),
        &lookup,
    );
    let backing = compute_backing_reports(
        &claim.id,
        std::slice::from_ref(&challenge),
        &lookup,
        "2026-06-01T00:00:00Z",
        10,
    );

    let report = synthesize(
        claim.id.clone(),
        criteria,
        &evidence_vec,
        std::slice::from_ref(&challenge),
        &backing,
        &cycled,
        "2026-06-01T00:00:00Z".into(),
    );

    let json = render_augmented(&RenderInput {
        report: &report,
        evidence: &evidence_vec,
        related_events: std::slice::from_ref(&challenge),
        backing_reports: &backing,
        cycle_contested: &cycled,
        metadata: None,
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
    });

    // Report is Contested by synthesize.
    assert_eq!(report.status, RenderStatus::Contested);
    assert_eq!(json["status"], "contested");
    // Criterion must agree — render now also uses the cycle set.
    assert_eq!(json["criteria"][0]["result"]["criterion_status"], "contested");
}

#[test]
fn shared_backing_claim_is_reused_across_sibling_branches() {
    // Codex round 7 finding 1: when sibling backings share a nested
    // backing claim, walk_backing's `visited` set skips the second
    // recursion, but the second sibling still needs to see the
    // already-synthesized shared report in its nested_backing so its
    // sustain check finds it. Otherwise the sibling stays Current
    // when it should sustain via the shared backing.
    let root = ClaimId::new("root");
    let b = ClaimId::new("claim-B");
    let c = ClaimId::new("claim-C");
    let d = ClaimId::new("claim-D");

    let root_to_b = challenge_targeting_any(Some(b.clone()));
    let root_to_c = challenge_targeting_any(Some(c.clone()));
    let b_to_d = challenge_targeting_any(Some(d.clone()));
    let c_to_d = challenge_targeting_any(Some(d.clone()));

    // D is a passing backing claim (Current + Pass criteria).
    let d_inputs = BackingClaimInputs {
        criteria: vec![],
        evidence: vec![],
        review_events: vec![],
    };
    // B and C each have one Challenge backed by D.
    let b_inputs = BackingClaimInputs {
        criteria: vec![],
        evidence: vec![],
        review_events: vec![b_to_d],
    };
    let c_inputs = BackingClaimInputs {
        criteria: vec![],
        evidence: vec![],
        review_events: vec![c_to_d],
    };

    let mut claims_map = HashMap::new();
    claims_map.insert(b.clone(), b_inputs);
    claims_map.insert(c.clone(), c_inputs);
    claims_map.insert(d.clone(), d_inputs);
    let lookup = InMemoryLookup { claims: claims_map };

    let backing = compute_backing_reports(
        &root,
        &[root_to_b, root_to_c],
        &lookup,
        "2026-06-01T00:00:00Z",
        10,
    );

    // D, B, C all appear.
    let by_id: HashMap<&ClaimId, &TrustReport> =
        backing.iter().map(|r| (&r.claim, r)).collect();
    assert!(by_id.contains_key(&b), "B must be synthesized");
    assert!(by_id.contains_key(&c), "C must be synthesized");
    assert!(by_id.contains_key(&d), "D must be synthesized");
    assert_eq!(by_id.len(), 3, "exactly three reports");

    // D has no criteria (no challenges and no evidence). It is Current
    // but does not sustain any parent challenge (empty criteria fails
    // backing_report_sustains). What matters for THIS test is that
    // both B AND C see D's report in their nested_backing. We can't
    // directly inspect that here, but the second sibling's behavior
    // would diverge from the first under the bug.
    //
    // To make the test diagnostic, give D a passing criterion so its
    // sustain holds and verify both B and C are Contested.
}

#[test]
fn shared_passing_backing_contests_both_sibling_branches() {
    // Sharper variant: D has a Pass criterion. B and C both back D
    // and both should be Contested.
    use typed_trust::Attested;

    let root = ClaimId::new("root");
    let b = ClaimId::new("claim-B");
    let c = ClaimId::new("claim-C");
    let d = ClaimId::new("claim-D-passes");

    // Construct challenges whose target matches the holding claim so
    // target_touches_report passes — without that, even a sustained
    // backing wouldn't move status.
    let challenge_against = |target_claim: ClaimId, id: &str, backing: ClaimId| ReviewEvent {
        id: EventId::new(id),
        target: Target::Claim(target_claim),
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
            backed_by: Some(backing),
        },
    };
    let root_to_b = challenge_against(root.clone(), "rev-root-to-b", b.clone());
    let root_to_c = challenge_against(root.clone(), "rev-root-to-c", c.clone());
    let b_to_d = challenge_against(b.clone(), "rev-b-to-d", d.clone());
    let c_to_d = challenge_against(c.clone(), "rev-c-to-d", d.clone());

    // D synthesizes to Current + Pass. We can get that by giving D a
    // single Pass criterion via its inputs — synthesize generates the
    // criterion from a tolerance. Use a tolerance + a matching
    // observation in evidence.
    let crit_id = CriterionId::new("d-criterion-0");
    let synth_runner = Identity {
        kind: IdentityKind::Automated,
        name: "synth".into(),
        details: vec![],
    };
    let d_evidence = Evidence {
        id: EvidenceId::new("ev-d"),
        for_claim: d.clone(),
        kind: EvidenceKind::Benchmark,
        locator: Locator::Artifact("x".into()),
        extraction: Derivation::Verified {
            method: ToolInvocation {
                command: "x".into(),
                tool_version: "x".into(),
                env: vec![],
            },
            ran_by: synth_runner.clone(),
            reruns: vec![Rerun {
                at: "2026-06-01T00:00:00Z".into(),
                by: synth_runner.clone(),
                observed: vec![MetricObservation {
                    criterion: crit_id.clone(),
                    value: 0.001,
                    unit: None,
                }],
                corpus_sha: None,
                outcome: ReproductionOutcome::Matched,
            }],
        },
        supports: Attested {
            value: SupportRelation::Supports {
                strength: Strength::Strong,
            },
            derivation: Derivation::Judged {
                by: Identity {
                    kind: IdentityKind::Human,
                    name: "u".into(),
                    details: vec![],
                },
                protocol: Some("p".into()),
                rationale: "r".into(),
                confidence: Confidence::High,
            },
            at: "2026-06-01T00:00:00Z".into(),
        },
        replay_status: Default::default(),
        replay_reason: None,
    };
    let d_inputs = BackingClaimInputs {
        criteria: vec![typed_trust::translate::TranslatedCriterion {
            id: crit_id.clone(),
            tolerance: Some(Tolerance {
                metric: "relative_error".into(),
                op: ComparisonOp::Lt,
                value: 0.01,
                output: None,
                against: None,
                prose: "rel < 0.01".into(),
            }),
            prose: "rel < 0.01".into(),
        }],
        evidence: vec![d_evidence],
        review_events: vec![],
    };
    let b_inputs = BackingClaimInputs {
        criteria: vec![],
        evidence: vec![],
        review_events: vec![b_to_d],
    };
    let c_inputs = BackingClaimInputs {
        criteria: vec![],
        evidence: vec![],
        review_events: vec![c_to_d],
    };

    let mut claims_map = HashMap::new();
    claims_map.insert(b.clone(), b_inputs);
    claims_map.insert(c.clone(), c_inputs);
    claims_map.insert(d.clone(), d_inputs);
    let lookup = InMemoryLookup { claims: claims_map };

    let backing = compute_backing_reports(
        &root,
        &[root_to_b, root_to_c],
        &lookup,
        "2026-06-01T00:00:00Z",
        10,
    );

    let by_id: HashMap<&ClaimId, &TrustReport> =
        backing.iter().map(|r| (&r.claim, r)).collect();

    // D is the leaf — passes its own criterion, status Current.
    assert_eq!(by_id[&d].status, RenderStatus::Current);
    assert_eq!(
        by_id[&d].criteria[0].result.value,
        CriterionResult::Pass
    );

    // B and C must BOTH be Contested — D's report sustains the
    // challenge in both branches. Without the shared-backing fix, the
    // second sibling (C) misses D in its nested_backing and stays
    // Current.
    assert_eq!(by_id[&b].status, RenderStatus::Contested, "B contested");
    assert_eq!(by_id[&c].status, RenderStatus::Contested, "C contested");
}

#[test]
fn root_involving_cycle_detected_when_lookup_lacks_root() {
    // Codex round 6 finding 2: a cycle that includes the root claim
    // (root → B → root) must be detected even when the ClaimLookup
    // only contains backing-claim inputs and not the root's. The DFS
    // is now seeded with the root claim on the stack so the back edge
    // from B → root is observed.
    let root = ClaimId::new("the-root-claim");
    let b = ClaimId::new("claim-B");

    let root_event = challenge_targeting_any(Some(b.clone()));
    let b_event_back_to_root = challenge_targeting_any(Some(root.clone()));

    // Lookup contains ONLY B — not the root. This matches the
    // ClaimLookup contract ("backing-claim inputs").
    let mut claims_map = HashMap::new();
    claims_map.insert(
        b.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![b_event_back_to_root],
        },
    );
    let lookup = InMemoryLookup { claims: claims_map };

    let cycled = detect_cycle_contested(
        &root,
        std::slice::from_ref(&root_event),
        &lookup,
    );

    assert!(cycled.contains(&root), "root must be in cycle set");
    assert!(cycled.contains(&b), "B must be in cycle set");
}

#[test]
fn top_level_report_contested_when_backed_by_claim_reaches_cycle() {
    // Codex round 5: per design §8 a top-level claim whose challenge
    // graph reaches a cycle should itself be Contested. Previously the
    // sustain check only treated Current backing reports as
    // sustaining, so a Contested-by-cycle backing didn't move the
    // parent — leaving the top-level Current despite reaching a cycle.
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let crit_id = criteria[0].id.clone();

    // X is the top-level claim's challenge target; X is itself on a
    // cycle (X → Y → X).
    let x = ClaimId::new("claim-X");
    let y = ClaimId::new("claim-Y");

    let top_challenge = ReviewEvent {
        id: EventId::new("rev-top-backed-by-cycled"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("p".into()),
        rationale: "Backed by a claim that reaches a cycle.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(x.clone()),
        },
    };

    // Build the backing-claim graph: X → Y → X.
    let mut claims_map = HashMap::new();
    claims_map.insert(
        x.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![challenge_targeting_any(Some(y.clone()))],
        },
    );
    claims_map.insert(
        y.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![challenge_targeting_any(Some(x.clone()))],
        },
    );
    let lookup = InMemoryLookup { claims: claims_map };

    // Precompute cycle set + backing reports with claim.id as root.
    let cycled = detect_cycle_contested(&claim.id, std::slice::from_ref(&top_challenge), &lookup);
    let backing = compute_backing_reports(
        &claim.id,
        std::slice::from_ref(&top_challenge),
        &lookup,
        "2026-06-01T00:00:00Z",
        10,
    );

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        std::slice::from_ref(&top_challenge),
        &backing,
        &cycled,
        "2026-06-01T00:00:00Z".into(),
    );

    // X and Y are cycled; the top-level reaches them through its
    // challenge, so it inherits Contested.
    assert!(cycled.contains(&x));
    assert!(cycled.contains(&y));
    assert_eq!(report.status, RenderStatus::Contested);
}

#[test]
fn compute_backing_reports_off_cycle_branch_stays_current() {
    // Chain: ROOT → SAFE, ROOT → A, A → B → A. SAFE has no cycle on
    // its branch and no Pass criteria, so it should stay Current
    // (no challenges against it). A and B are cycle members; ROOT
    // reaches a cycle via the A branch and is contested.
    let root = ClaimId::new("claim-ROOT");
    let safe = ClaimId::new("claim-SAFE");
    let a = ClaimId::new("claim-A");
    let b = ClaimId::new("claim-B");

    let root_to_safe = challenge_targeting_any(Some(safe.clone()));
    let root_to_a = challenge_targeting_any(Some(a.clone()));
    let a_to_b = challenge_targeting_any(Some(b.clone()));
    let b_to_a = challenge_targeting_any(Some(a.clone()));

    let mut claims = HashMap::new();
    claims.insert(
        root.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![root_to_safe, root_to_a.clone()],
        },
    );
    claims.insert(safe.clone(), empty_inputs());
    claims.insert(
        a.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![a_to_b],
        },
    );
    claims.insert(
        b.clone(),
        BackingClaimInputs {
            criteria: vec![],
            evidence: vec![],
            review_events: vec![b_to_a],
        },
    );
    let lookup = InMemoryLookup { claims };

    let initial = challenge_targeting_any(Some(root.clone()));
    let backing = compute_backing_reports(&ClaimId::new("test-root"),
        std::slice::from_ref(&initial),
        &lookup,
        "2026-06-01T00:00:00Z",
        10,
    );

    let by_id: HashMap<&ClaimId, &TrustReport> =
        backing.iter().map(|r| (&r.claim, r)).collect();
    assert_eq!(by_id[&root].status, RenderStatus::Contested, "ROOT reaches cycle");
    assert_eq!(by_id[&a].status, RenderStatus::Contested, "A on cycle");
    assert_eq!(by_id[&b].status, RenderStatus::Contested, "B on cycle");
    assert_eq!(by_id[&safe].status, RenderStatus::Current, "SAFE off-cycle");
}

#[test]
fn substantive_challenge_backed_by_failing_criteria_does_not_sustain() {
    // Per codex review #2 (round 2) and design §8 "passing-criteria
    // result": a backing report with status=Current but Fail criteria
    // does NOT sustain the parent challenge.
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let crit_id = criteria[0].id.clone();
    let backing_id = ClaimId::new("failing-criteria-backing");

    let challenge = ReviewEvent {
        id: EventId::new("rev-fail-backing"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("proteon-peer-review-v1".into()),
        rationale: "Backing claim's own criteria fail.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(backing_id.clone()),
        },
    };

    // Backing report: status=Current but criterion result is Fail.
    let synth_runner = Identity {
        kind: IdentityKind::Automated,
        name: "evident-synthesizer".into(),
        details: vec![],
    };
    let failing_backing = TrustReport {
        claim: backing_id,
        status: RenderStatus::Current,
        criteria: vec![Criterion {
            id: CriterionId::new("backing-crit-0"),
            name: "backing's failing criterion".into(),
            tolerance: None,
            result: Attested {
                value: CriterionResult::Fail,
                derivation: Derivation::Verified {
                    method: ToolInvocation {
                        command: "rule:Fail".into(),
                        tool_version: "test".into(),
                        env: vec![],
                    },
                    ran_by: synth_runner,
                    reruns: vec![],
                },
                at: "2026-06-01T00:00:00Z".into(),
            },
        }],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };

    let report = synthesize(
        claim.id,
        criteria,
        &[evidence],
        std::slice::from_ref(&challenge),
        std::slice::from_ref(&failing_backing),
        &std::collections::HashSet::new(),
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Current);
}

#[test]
fn substantive_challenge_backed_by_empty_criteria_does_not_sustain() {
    // An empty-criteria backing report (no evaluable proposition) does
    // not sustain. status=Current alone is not enough.
    let (claim, criteria, evidence) = translate_to_pieces(PROTEON_SASA_RELEASE_YAML);
    let crit_id = criteria[0].id.clone();
    let backing_id = ClaimId::new("empty-criteria-backing");

    let challenge = ReviewEvent {
        id: EventId::new("rev-empty-backing"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "reviewer".into(),
            details: vec![],
        },
        protocol: Some("proteon-peer-review-v1".into()),
        rationale: "Backing claim has no criteria.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(backing_id.clone()),
        },
    };

    let empty_backing = TrustReport {
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
        std::slice::from_ref(&challenge),
        std::slice::from_ref(&empty_backing),
        &std::collections::HashSet::new(),
        "2026-06-01T00:00:00Z".into(),
    );

    assert_eq!(report.status, RenderStatus::Current);
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
    let backing = compute_backing_reports(&ClaimId::new("test-root"),
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

// ============================================================
// PR5c: metadata declaration flows through render_augmented
// ============================================================

#[test]
fn render_augmented_inlines_metadata_declaration_block_pr5c() {
    use typed_trust::{
        claim::MetadataDeclaration,
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };

    let report = TrustReport {
        claim: ClaimId::new("pdbtbx-rust-msrv"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![], gaps: vec![], aggregate: None,
    };
    let md = MetadataDeclaration {
        field: "rust_msrv".into(),
        declared_value: "1.67".into(),
        source_file: "Cargo.toml".into(),
        source_path: "package.rust-version".into(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: Some(&md),
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
    });

    let block = augmented
        .get("metadata_declaration")
        .expect("metadata_declaration inlined at top level");
    assert_eq!(block["field"], "rust_msrv");
    assert_eq!(block["declared_value"], "1.67");
    assert_eq!(block["source_file"], "Cargo.toml");
    assert_eq!(block["source_path"], "package.rust-version");

    // Sanity: a None metadata input omits the field rather than
    // serializing as null — keeps the JSON shape stable for
    // measurement claims.
    let augmented_no_md = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
    });
    assert!(augmented_no_md.get("metadata_declaration").is_none());
}

#[test]
fn human_render_emits_metadata_declaration_section_pr5c() {
    use typed_trust::{
        claim::MetadataDeclaration,
        human_render::render_markdown,
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("pkg-python-3-10"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![], gaps: vec![], aggregate: None,
    };
    let md = MetadataDeclaration {
        field: "python_version_requirement".into(),
        declared_value: ">=3.10".into(),
        source_file: "pyproject.toml".into(),
        source_path: "project.requires-python".into(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: Some(&md),
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
    });
    let md_out = render_markdown(&augmented);
    assert!(md_out.contains("## Metadata declaration"));
    assert!(md_out.contains("python_version_requirement"));
    assert!(md_out.contains(">=3.10"));
    assert!(md_out.contains("pyproject.toml"));
    assert!(md_out.contains("project.requires-python"));
}

#[test]
fn html_render_emits_metadata_declaration_dl_pr5c() {
    use typed_trust::{
        claim::MetadataDeclaration,
        html_render::render_html_fragment,
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("pkg-node"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![], gaps: vec![], aggregate: None,
    };
    let md = MetadataDeclaration {
        field: "node_engine".into(),
        declared_value: ">=18".into(),
        source_file: "package.json".into(),
        source_path: "engines.node".into(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: Some(&md),
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
    });
    let html = render_html_fragment(&augmented);
    assert!(html.contains("<h2>Metadata declaration</h2>"));
    assert!(html.contains("class=\"metadata-declaration\""));
    assert!(html.contains("node_engine"));
    assert!(html.contains("&gt;=18"));
    assert!(html.contains("package.json"));
    assert!(html.contains("engines.node"));
}

/// PR5c codex F-CR3 (P2): metadata values containing backticks or
/// newlines must not break out of the markdown code span. The
/// renderer picks a longer backtick fence when needed and collapses
/// newlines so the section stays well-formed.
#[test]
fn human_render_escapes_backticks_and_newlines_in_metadata_values_pr5c_cr3() {
    use typed_trust::{
        claim::MetadataDeclaration,
        human_render::render_markdown,
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("evil"),
        status: RenderStatus::Current,
        criteria: vec![], challenges: vec![], gaps: vec![], aggregate: None,
    };
    let md = MetadataDeclaration {
        field: "f".into(),
        // Backtick inside the value would close a single-tick span;
        // newlines would terminate the list item.
        declared_value: "value with ` and\nnewline".into(),
        source_file: "Cargo.toml".into(),
        source_path: "package.rust-version".into(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: Some(&md),
        concordance: None,
        concordance_result: None,
        observation: None,
        observation_result: None,
    });
    let out = render_markdown(&augmented);
    // The raw backtick character survives but inside a multi-backtick
    // fence (so it doesn't terminate the span). Newline replaced by
    // a space.
    assert!(
        out.contains("``value with ` and newline``"),
        "metadata value not safely fenced; got:\n{out}"
    );
    // Confirm the bullet list ends with a blank line, not a broken
    // list item.
    assert!(out.contains("\n\n## ") || out.ends_with("\n"));
}

// ============================================================
// PR5f: behavioral_concordance render augmentation
// ============================================================

#[test]
fn render_augmented_inlines_concordance_declaration_for_numeric_band_pr5f() {
    use std::collections::BTreeMap;
    use typed_trust::{
        claim::{
            ConcordanceDeclaration, ConcordancePattern, PriorBindingContext,
        },
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("rustims-fragpipe-fdr-10k-concords-meier"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let cd = ConcordanceDeclaration {
        pattern: ConcordancePattern::NumericBand {
            metric_path: "fragpipe.hla_10k.fdr_pct".into(),
            epsilon: 0.5,
            prior_value: 1.5,
        },
        paper_locator: "source/cited.md#rustims-fragpipe-fdr-10k".into(),
        prior_binding: PriorBindingContext {
            prior_unit: "percentage_points".into(),
            prior_metric_definition: "Empirical true FDR per Meier 2024.".into(),
            locator: "Meier 2024 Table 3".into(),
            prior_extraction_note: "Curator verified 2026".into(),
            source_id: "doi:10.1038/PLACEHOLDER".into(),
        },
    };
    let _ = BTreeMap::<String, String>::new();
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: Some(&cd),
        concordance_result: None,
        observation: None,
        observation_result: None,
    });
    let block = augmented
        .get("concordance_declaration")
        .expect("concordance_declaration inlined at top level");
    let pattern = &block["pattern"];
    assert_eq!(pattern["pattern_kind"], "numeric_band");
    assert_eq!(pattern["metric_path"], "fragpipe.hla_10k.fdr_pct");
    assert_eq!(pattern["epsilon"], 0.5);
    assert_eq!(pattern["prior_value"], 1.5);
    assert_eq!(block["paper_locator"], "source/cited.md#rustims-fragpipe-fdr-10k");
    assert_eq!(block["prior_binding"]["prior_unit"], "percentage_points");

    // Sanity: None metadata + Some concordance still omits the
    // metadata key, preserving the disjointness story.
    assert!(augmented.get("metadata_declaration").is_none());
}

#[test]
fn human_render_emits_concordance_section_pr5f() {
    use typed_trust::{
        claim::{
            ConcordanceDeclaration, ConcordancePattern, PriorBindingContext,
        },
        human_render::render_markdown,
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("rustims-fragpipe-fdr-10k-concords-meier"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let cd = ConcordanceDeclaration {
        pattern: ConcordancePattern::NumericBand {
            metric_path: "fragpipe.hla_10k.fdr_pct".into(),
            epsilon: 0.5,
            prior_value: 1.5,
        },
        paper_locator: "source/cited.md#rustims-fragpipe-fdr-10k".into(),
        prior_binding: PriorBindingContext {
            prior_unit: "percentage_points".into(),
            prior_metric_definition: "Empirical true FDR.".into(),
            locator: "Meier 2024 Table 3".into(),
            prior_extraction_note: "Curator verified".into(),
            source_id: "doi:10.1038/PLACEHOLDER".into(),
        },
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: Some(&cd),
        concordance_result: None,
        observation: None,
        observation_result: None,
    });
    let md = render_markdown(&augmented);
    assert!(md.contains("## Concordance"));
    assert!(md.contains("numeric_band"));
    assert!(md.contains("fragpipe.hla_10k.fdr_pct"));
    assert!(md.contains("paper_locator") || md.contains("Paper locator"));
    assert!(md.contains("percentage_points"));
    assert!(md.contains("Prior binding"));
}

// ============================================================
// PR5h: concordance_result block in augmented JSON + render
// ============================================================

#[test]
fn render_augmented_inlines_concordance_result_block_pr5h() {
    use typed_trust::{
        claim::{
            ComparisonStatus, ConcordanceDeclaration, ConcordancePattern,
            ConcordanceResult, PriorBindingContext,
        },
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("rustims-fragpipe-fdr-10k-concords-meier"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let cd = ConcordanceDeclaration {
        pattern: ConcordancePattern::NumericBand {
            metric_path: "fragpipe.hla_10k.fdr_pct".into(),
            epsilon: 0.5,
            prior_value: 1.5,
        },
        paper_locator: "src.md".into(),
        prior_binding: PriorBindingContext {
            prior_unit: "percentage_points".into(),
            prior_metric_definition: "FDR".into(),
            locator: "Meier T3".into(),
            prior_extraction_note: "curator verified".into(),
            source_id: "doi:test".into(),
        },
    };
    let mut diag = serde_json::Map::new();
    diag.insert("delta_from_prior".into(), serde_json::json!(0.1));
    diag.insert("within_band".into(), serde_json::json!(true));
    let cr = ConcordanceResult {
        comparison_status: ComparisonStatus::Pass,
        observed_value: Some(1.6),
        observed_unit: Some("percentage_points".into()),
        observed_ordering: None,
        prior_ordering: None,
        observed_series: None,
        parameter_series: None,
        image_digest: Some("sha256:abc".into()),
        produced_at: Some("2026-06-04T10:00:00Z".into()),
        diagnostics: diag,
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: Some(&cd),
        concordance_result: Some(&cr),
        observation: None,
        observation_result: None,
    });
    let block = augmented
        .get("concordance_result")
        .expect("concordance_result inlined at top level");
    assert_eq!(block["comparison_status"], "pass");
    assert_eq!(block["observed_value"], 1.6);
    assert_eq!(block["observed_unit"], "percentage_points");
    assert_eq!(block["image_digest"], "sha256:abc");
    assert_eq!(block["diagnostics"]["within_band"], true);
}

#[test]
fn human_render_emits_concordance_result_section_pr5h() {
    use typed_trust::{
        claim::{
            ComparisonStatus, ConcordanceDeclaration, ConcordancePattern,
            ConcordanceResult, PriorBindingContext,
        },
        human_render::render_markdown,
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("x"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let cd = ConcordanceDeclaration {
        pattern: ConcordancePattern::NumericBand {
            metric_path: "x.y".into(),
            epsilon: 0.5,
            prior_value: 1.5,
        },
        paper_locator: "src.md".into(),
        prior_binding: PriorBindingContext {
            prior_unit: "percentage_points".into(),
            prior_metric_definition: "FDR".into(),
            locator: "T3".into(),
            prior_extraction_note: "x".into(),
            source_id: "doi:test".into(),
        },
    };
    let cr = ConcordanceResult {
        comparison_status: ComparisonStatus::Pass,
        observed_value: Some(1.6),
        observed_unit: Some("percentage_points".into()),
        observed_ordering: None,
        prior_ordering: None,
        observed_series: None,
        parameter_series: None,
        image_digest: Some("sha256:abc".into()),
        produced_at: Some("2026-06-04T10:00:00Z".into()),
        diagnostics: serde_json::Map::new(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: Some(&cd),
        concordance_result: Some(&cr),
        observation: None,
        observation_result: None,
    });
    let md = render_markdown(&augmented);
    assert!(md.contains("## Concordance result"));
    assert!(md.contains("Pass"));
    assert!(md.contains("1.6"));
    assert!(md.contains("sha256:abc"));
}

#[test]
fn human_render_emits_concordance_result_ordering_for_ordinal_match_pr5h() {
    use typed_trust::{
        claim::{
            ComparisonStatus, ConcordanceDeclaration, ConcordancePattern,
            ConcordanceResult, PriorBindingContext, RankingDirection, TiePolicy,
        },
        human_render::render_markdown,
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("ord"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let mut e2p = std::collections::BTreeMap::new();
    e2p.insert("FragPipe".into(), "fragpipe.fdr".into());
    e2p.insert("PEAKS".into(), "peaks.fdr".into());
    let mut prior = std::collections::BTreeMap::new();
    prior.insert("FragPipe".into(), 1.5);
    prior.insert("PEAKS".into(), 1.8);
    let cd = ConcordanceDeclaration {
        pattern: ConcordancePattern::OrdinalMatch {
            entity_to_path: e2p,
            direction: RankingDirection::LowerIsBetter,
            tie_policy: TiePolicy::Strict,
            prior_value: prior,
        },
        paper_locator: "src.md".into(),
        prior_binding: PriorBindingContext {
            prior_unit: "percentage_points".into(),
            prior_metric_definition: "FDR".into(),
            locator: "T3".into(),
            prior_extraction_note: "x".into(),
            source_id: "doi:test".into(),
        },
    };
    let cr = ConcordanceResult {
        comparison_status: ComparisonStatus::Pass,
        observed_value: None,
        observed_unit: None,
        observed_ordering: Some(vec!["FragPipe".into(), "PEAKS".into()]),
        prior_ordering: Some(vec!["FragPipe".into(), "PEAKS".into()]),
        observed_series: None,
        parameter_series: None,
        image_digest: None,
        produced_at: None,
        diagnostics: serde_json::Map::new(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: Some(&cd),
        concordance_result: Some(&cr),
        observation: None,
        observation_result: None,
    });
    let md = render_markdown(&augmented);
    assert!(md.contains("## Concordance result"));
    assert!(md.contains("Observed ordering"));
    assert!(md.contains("FragPipe"));
    assert!(md.contains("PEAKS"));
    // Arrow separator used in human_render.
    assert!(md.contains("→"));
}

// ============================================================
// PR5i: third_party_observation render augmentation
// ============================================================

#[test]
fn render_augmented_inlines_observation_declaration_and_renames_prior_value_pr5i() {
    use typed_trust::{
        claim::{
            ConcordancePattern, ObservationDeclaration,
        },
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("rustims-maxquant-peak-matching"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let od = ObservationDeclaration {
        third_party_tool: "MaxQuant".into(),
        metric_definition: "Peak matching error rate per Cox 2008.".into(),
        pattern: ConcordancePattern::NumericBand {
            metric_path: "maxquant.peak_matching_error.fraction_pct".into(),
            epsilon: 5.0,
            prior_value: 30.0,
        },
        paper_locator: "source/cited.md#maxquant".into(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: None,
        concordance_result: None,
        observation: Some(&od),
        observation_result: None,
    });
    let block = augmented
        .get("observation_declaration")
        .expect("observation_declaration inlined at top level");
    assert_eq!(block["third_party_tool"], "MaxQuant");
    assert_eq!(block["paper_locator"], "source/cited.md#maxquant");
    // Codex v3 F-CR1: the pattern surfaces `observed_value`, NOT
    // `prior_value`, even though the internal Rust enum uses the
    // latter for code reuse.
    let pattern = &block["pattern"];
    assert_eq!(pattern["pattern_kind"], "numeric_band");
    assert_eq!(pattern["observed_value"], 30.0);
    assert!(
        pattern.get("prior_value").is_none(),
        "render must NOT leak prior_value externally; got {pattern:?}"
    );
}

#[test]
fn render_full_augmented_does_not_leak_prior_value_anywhere_pr5i() {
    // Codex v3 F-CR1: the rename must apply across the whole
    // augmented JSON. No path inside `observation_declaration`
    // should contain `prior_value`.
    use typed_trust::{
        claim::{ConcordancePattern, ObservationDeclaration},
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("x"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let od = ObservationDeclaration {
        third_party_tool: "X".into(),
        metric_definition: "y".into(),
        pattern: ConcordancePattern::NumericBand {
            metric_path: "x.y".into(),
            epsilon: 1.0,
            prior_value: 10.0,
        },
        paper_locator: "src.md".into(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: None,
        concordance_result: None,
        observation: Some(&od),
        observation_result: None,
    });
    let serialized = serde_json::to_string(&augmented).unwrap();
    assert!(
        !serialized.contains("prior_value"),
        "augmented JSON must NOT leak prior_value (codex v3 F-CR1); got: {serialized}"
    );
}

#[test]
fn human_render_emits_observation_section_pr5i() {
    use typed_trust::{
        claim::{ConcordancePattern, ObservationDeclaration},
        human_render::render_markdown,
        report::{RenderStatus, TrustReport},
        render::{render_augmented, RenderInput},
        ClaimId,
    };
    let report = TrustReport {
        claim: ClaimId::new("x"),
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let od = ObservationDeclaration {
        third_party_tool: "MaxQuant".into(),
        metric_definition: "Peak matching error per Cox 2008.".into(),
        pattern: ConcordancePattern::NumericBand {
            metric_path: "maxquant.error".into(),
            epsilon: 5.0,
            prior_value: 30.0,
        },
        paper_locator: "src.md".into(),
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
        concordance: None,
        concordance_result: None,
        observation: Some(&od),
        observation_result: None,
    });
    let md = render_markdown(&augmented);
    assert!(md.contains("## Observation"));
    assert!(md.contains("MaxQuant"));
    assert!(md.contains("numeric_band"));
    assert!(md.contains("Observed value"));
    assert!(!md.contains("prior_value"));
    assert!(!md.contains("Prior value"));
}
