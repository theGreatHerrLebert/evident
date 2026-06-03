//! End-to-end HTML + Mermaid graph rendering tests.

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
fn render_contested_sasa_report_as_html_with_graph() {
    // Build the same contested-shape report end-to-end and render to
    // HTML. The output is written to tests/fixtures/contested.html for
    // a human reader to open in a browser.

    let manifest = parse_manifest_file(PROTEON_SASA_RELEASE_YAML).unwrap();
    let mc = &manifest.claims[0];
    let ctx = TranslationContext {
        now: "2026-06-01T00:00:00Z".into(),
        manifest_path: "proteon/evident/claims/sasa.yaml".into(),
    };
    let criteria = translate_tolerances(mc).unwrap();
    let evidence: Vec<Evidence> = translate_evidence(&ctx, mc, &criteria)
        .unwrap()
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
            name: "Background drift study bounds convention drift at 0.3%".into(),
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

    // Render the Mermaid graph standalone.
    let mermaid = render_mermaid_graph(&augmented);
    assert!(mermaid.starts_with("graph TD"));
    assert!(mermaid.contains("Claim"));
    assert!(mermaid.contains("Criterion"));
    assert!(mermaid.contains("Challenge"));
    assert!(mermaid.contains("Backing"));
    assert!(mermaid.contains("backed_by"));

    // Render the full HTML document.
    let html = render_html(&augmented);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("Trust Report"));
    assert!(html.contains("Contested"));
    assert!(html.contains("Jane Doe"));
    assert!(html.contains("Pass"));
    assert!(html.contains("class=\"mermaid\""));
    assert!(html.contains("graph TD"));
    assert!(html.contains("mermaid@10"));
    assert!(html.contains("Active challenges"));

    // Write fixtures for a human to open in a browser.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("contested.html"), html).expect("write contested.html");
    fs::write(dir.join("contested.mermaid"), mermaid).expect("write contested.mermaid");
}

#[test]
fn mermaid_graph_sanitizes_dashed_claim_ids() {
    // proteon's claim ids carry hyphens; Mermaid node ids must be
    // alphanumeric/underscore. Verify dashes collapse to underscores.
    let claim_id = ClaimId::new("proteon-sasa-vs-biopython-ci");
    let report = TrustReport {
        claim: claim_id,
        status: RenderStatus::Current,
        criteria: vec![],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    };
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &[],
        backing_reports: &[],
        cycle_contested: &std::collections::HashSet::new(),
        metadata: None,
    });
    let mermaid = render_mermaid_graph(&augmented);

    // Node id is sanitized; the original (with hyphens) survives in
    // the quoted label only.
    assert!(mermaid.contains("proteon_sasa_vs_biopython_ci["));
    assert!(mermaid.contains("proteon-sasa-vs-biopython-ci<br/>"));
}
