//! Render a contested TrustReport to markdown and write it to the
//! fixtures directory so a human can inspect what the rendered form
//! looks like end-to-end.

use std::fs;
use std::path::Path;

use typed_trust::translate::{
    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
};
use typed_trust::*;

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
      Across a 1000-PDB validation corpus, proteon total SASA agrees
      with Biopython's Shrake-Rupley implementation; median rel err < 0.5%.
    tolerances:
      - metric: median_relative_error
        op: "<"
        value: 0.005
        output: total_sasa
        prose: |
          Median(|proteon_total - biopython_total| / biopython_total) < 0.005
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

#[test]
fn render_contested_sasa_report_as_markdown() {
    // Translate + synthesize + augment + render the contested shape
    // end-to-end. Output is written to tests/fixtures/contested.md
    // for a human reader to inspect.

    let manifest = parse_manifest_file(PROTEON_SASA_RELEASE_YAML).unwrap();
    let mc = &manifest.claims[0];
    let ctx = TranslationContext {
        now: "2026-06-01T00:00:00Z".into(),
        manifest_path: "proteon/evident/claims/sasa.yaml".into(),
    };
    let criteria = translate_tolerances(mc).unwrap();
    let evidence: Vec<Evidence> = translate_evidence(&ctx, mc, &criteria).unwrap()
        .into_iter()
        .collect();
    let crit_id = criteria[0].id.clone();

    let backing_id = ClaimId::new("synthetic-tolerance-too-wide");

    let challenge = ReviewEvent {
        id: EventId::new("rev-jane-doe-tolerance-too-wide"),
        target: Target::Criterion(crit_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "Jane Doe".into(),
            details: vec![
                IdentityDetail {
                    key: "orcid".into(),
                    value: "0000-0000-0000-0001".into(),
                },
                IdentityDetail {
                    key: "affiliation".into(),
                    value: "Example University".into(),
                },
            ],
        },
        protocol: Some("proteon-release-peer-review-v1".into()),
        rationale: "Median 0.5% absorbs FreeSASA convention drift but \
                    leaves no room to detect proteon-side regressions of \
                    similar magnitude. Recommend tightening to 0.2%."
            .into(),
        at: "2026-06-01T12:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(backing_id.clone()),
        },
    };

    // A backing report that satisfies the sustain rule: status Current,
    // criteria non-empty, every criterion Pass.
    let synth_runner = Identity {
        kind: IdentityKind::Automated,
        name: "evident-synthesizer".into(),
        details: vec![],
    };
    let backing_report = TrustReport {
        claim: backing_id,
        status: RenderStatus::Current,
        criteria: vec![Criterion {
            id: CriterionId::new("synthetic-tolerance-too-wide-crit-0"),
            name: "BackgroundDrift study shows convention drift is bounded at 0.3%".into(),
            tolerance: None,
            result: Attested {
                value: CriterionResult::Pass,
                derivation: Derivation::Verified {
                    method: ToolInvocation {
                        command: "python validation/compare_freesasa_drift.py".into(),
                        tool_version: "drift-study-2026-05".into(),
                        env: vec![],
                    },
                    ran_by: synth_runner,
                    reruns: vec![],
                },
                at: "2026-06-01T12:00:00Z".into(),
            },
        }],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };

    let report = synthesize(
        ClaimId::new(&mc.id),
        criteria,
        &evidence,
        std::slice::from_ref(&challenge),
        std::slice::from_ref(&backing_report),
        &std::collections::HashSet::new(),
        "2026-06-01T12:00:00Z".into(),
    );

    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &evidence,
        related_events: std::slice::from_ref(&challenge),
        backing_reports: std::slice::from_ref(&backing_report),
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
    });

    let markdown = render_markdown(&augmented);

    // Sanity check the rendered form
    assert!(markdown.contains("# Trust Report"));
    assert!(markdown.contains("Contested"));
    assert!(markdown.contains("Jane Doe"));
    assert!(markdown.contains("Pass ✓"));
    assert!(markdown.contains("Biopython"));
    assert!(markdown.contains("0.0017"));
    assert!(markdown.contains("0.005"));

    // Write to fixtures for inspection.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("contested.md"), markdown).expect("write contested.md");
}

#[test]
fn endorse_event_not_rendered_under_active_challenges() {
    // Codex round 8: when related_events contains non-Challenge kinds
    // (Endorse, Dissent, Supersede), the markdown renderer must NOT
    // include them under "## Active Challenges" — that would
    // mis-represent normal review activity as objections.
    use typed_trust::Attested;

    let claim_id = ClaimId::new("test-claim");
    let report = TrustReport {
        claim: claim_id.clone(),
        status: RenderStatus::Current,
        criteria: vec![Criterion {
            id: CriterionId::new("c0"),
            name: "metric < 0.5".into(),
            tolerance: None,
            result: Attested {
                value: CriterionResult::Pass,
                derivation: Derivation::Verified {
                    method: ToolInvocation {
                        command: "x".into(),
                        tool_version: "x".into(),
                        env: vec![],
                    },
                    ran_by: Identity {
                        kind: IdentityKind::Automated,
                        name: "synth".into(),
                        details: vec![],
                    },
                    reruns: vec![],
                },
                at: "2026-06-01T00:00:00Z".into(),
            },
        }],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };

    // An Endorse event in related_events.
    let endorse = ReviewEvent {
        id: EventId::new("rev-endorse"),
        target: Target::Claim(claim_id),
        by: Identity {
            kind: IdentityKind::Human,
            name: "Approver".into(),
            details: vec![],
        },
        protocol: Some("p".into()),
        rationale: "Looks good to me.".into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Endorse,
    };

    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: std::slice::from_ref(&endorse),
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
    });
    let markdown = render_markdown(&augmented);

    // The endorse event is in _graph.review_events but must NOT appear
    // under "## Active Challenges". Since this is the only event, the
    // "Active Challenges" section should not appear at all.
    assert!(
        !markdown.contains("## Active Challenges"),
        "endorse event leaked into Active Challenges section:\n{markdown}"
    );
    // Endorse is still in the graph payload — render_augmented owns
    // that, render_markdown just doesn't surface it as a challenge.
    assert_eq!(
        augmented["_graph"]["review_events"][0]["kind"]["type"],
        "endorse"
    );
}
