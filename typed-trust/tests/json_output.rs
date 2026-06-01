//! Builds a TrustReport in code (the proteon SASA release-tier shape)
//! and serializes it to pretty JSON. Asserts the JSON shape matches the
//! sketch in `concepts/typed-trust-json-shape.md` and writes a copy to
//! `tests/fixtures/proteon_sasa_release.trustreport.json` for inspection.

use std::fs;
use std::path::Path;

use typed_trust::*;

fn build_sasa_release_report() -> TrustReport {
    // Identities used throughout.
    let synth_runner = Identity {
        kind: IdentityKind::Automated,
        name: "evident-synthesizer".into(),
        details: vec![],
    };
    let maintainer = Identity {
        kind: IdentityKind::Human,
        name: "proteon maintainer".into(),
        details: vec![],
    };

    // The three criteria from the SASA release-tier YAML.
    let crit0_id = CriterionId::new("proteon-sasa-vs-biopython-release-1k-pdbs-criterion-0");
    let crit1_id = CriterionId::new("proteon-sasa-vs-biopython-release-1k-pdbs-criterion-1");
    let crit2_id = CriterionId::new("proteon-sasa-vs-biopython-release-1k-pdbs-criterion-2");

    // Pure-function "rule" derivation for synthesis-internal Pass/Fail
    // determinations. See proteon_walkthrough's note on this — v0.9
    // candidate.
    let rule_derivation = |rule: &str| Derivation::Verified {
        method: ToolInvocation {
            command: rule.into(),
            tool_version: "typed-trust-synth 0.1.0".into(),
            env: vec![],
        },
        ran_by: synth_runner.clone(),
        reruns: vec![],
    };

    let at = "2026-05-11T00:00:00Z".to_string();

    let criteria = vec![
        Criterion {
            id: crit0_id,
            name: "Median rel err vs Biopython < 0.5%".into(),
            tolerance: Some(Tolerance {
                metric: "median_relative_error".into(),
                op: ComparisonOp::Lt,
                value: 0.005,
                output: Some("total_sasa".into()),
                against: Some("Biopython".into()),
                prose: "Median(|proteon - biopython| / biopython) < 0.005".into(),
            }),
            result: Attested {
                value: CriterionResult::Pass,
                derivation: rule_derivation("rule:Lt(observed, tolerance)"),
                at: at.clone(),
            },
        },
        Criterion {
            id: crit1_id,
            name: "Median rel err vs FreeSASA < 2%".into(),
            tolerance: Some(Tolerance {
                metric: "median_relative_error".into(),
                op: ComparisonOp::Lt,
                value: 0.02,
                output: Some("total_sasa".into()),
                against: Some("FreeSASA".into()),
                prose: "Median(|proteon - freesasa| / freesasa) < 0.02".into(),
            }),
            result: Attested {
                value: CriterionResult::Pass,
                derivation: rule_derivation("rule:Lt(observed, tolerance)"),
                at: at.clone(),
            },
        },
        Criterion {
            id: crit2_id,
            name: "Pass rate >= 95%".into(),
            tolerance: Some(Tolerance {
                metric: "pass_rate".into(),
                op: ComparisonOp::GtEq,
                value: 0.95,
                output: Some("total_sasa".into()),
                against: None,
                prose: "pass / (pass + warn + fail - loading.fail) >= 0.95".into(),
            }),
            result: Attested {
                value: CriterionResult::Pass,
                derivation: rule_derivation("rule:GtEq(observed, tolerance)"),
                at: at.clone(),
            },
        },
    ];

    let _ = &maintainer; // identity for downstream tests if needed

    TrustReport {
        claim: ClaimId::new("proteon-sasa-vs-biopython-release-1k-pdbs"),
        status: RenderStatus::Current,
        criteria,
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
    }
}

fn build_contested_report() -> TrustReport {
    // Minimal contested-shape: one criterion with a Challenge target
    // recorded in the report's challenges list and status: contested.
    let synth_runner = Identity {
        kind: IdentityKind::Automated,
        name: "evident-synthesizer".into(),
        details: vec![],
    };

    let at = "2026-05-02T00:00:00Z".to_string();
    let criterion = Criterion {
        id: CriterionId::new("charmm19-electrostatic-vs-ball-rel-err"),
        name: "Electrostatic relative_error < 25%".into(),
        tolerance: Some(Tolerance {
            metric: "relative_error".into(),
            op: ComparisonOp::Lt,
            value: 0.25,
            output: Some("electrostatic".into()),
            against: Some("BALL".into()),
            prose: "|proteon - BALL| / |BALL| < 25% on electrostatic at NoCutoff.".into(),
        }),
        result: Attested {
            value: CriterionResult::Pass,
            derivation: Derivation::Verified {
                method: ToolInvocation {
                    command: "rule:Lt(observed, tolerance)".into(),
                    tool_version: "typed-trust-synth 0.1.0".into(),
                    env: vec![],
                },
                ran_by: synth_runner,
                reruns: vec![],
            },
            at,
        },
    };

    TrustReport {
        claim: ClaimId::new("proteon-charmm19-vs-ball-ci"),
        status: RenderStatus::Contested,
        criteria: vec![criterion],
        challenges: vec![EventId::new("rev-synthetic-electrostatic-band-too-wide")],
        gaps: vec![Gap {
            description: "Tolerance band may not discriminate proteon-side \
                          convention errors from BALL convention divergence \
                          of comparable magnitude."
                .into(),
            would_satisfy: vec![
                "tighter electrostatic tolerance".into(),
                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
            ],
            author_actionable: true,
        }],
        aggregate: None,
    }
}

#[test]
fn sasa_release_report_serializes_to_expected_json_shape() {
    let report = build_sasa_release_report();
    let json = serde_json::to_value(&report).unwrap();

    // Top-level fields.
    assert_eq!(
        json["claim"],
        "proteon-sasa-vs-biopython-release-1k-pdbs"
    );
    assert_eq!(json["status"], "current");
    assert_eq!(json["criteria"].as_array().unwrap().len(), 3);

    // The first criterion exposes the F-PR3 win: tolerance.against
    // populated to the oracle name.
    let crit0 = &json["criteria"][0];
    assert_eq!(crit0["tolerance"]["metric"], "median_relative_error");
    assert_eq!(crit0["tolerance"]["op"], "<");
    assert_eq!(crit0["tolerance"]["against"], "Biopython");
    assert_eq!(crit0["tolerance"]["value"], 0.005);

    // Adjacent-tagged CriterionResult: unit variant Pass produces a
    // single-field "type" object so consumers can dispatch on .type
    // consistently across unit and struct variants.
    assert_eq!(crit0["result"]["value"]["type"], "pass");

    // Derivation::Verified is adjacent-tagged with content under "data".
    assert_eq!(crit0["result"]["derivation"]["type"], "verified");
    assert_eq!(
        crit0["result"]["derivation"]["data"]["method"]["tool_version"],
        "typed-trust-synth 0.1.0"
    );

    // Empty Vec<Rerun> is omitted (skip_serializing_if).
    assert!(crit0["result"]["derivation"]["data"]["reruns"].is_null());

    // Empty challenges/gaps and None aggregate are omitted.
    assert!(json["challenges"].is_null());
    assert!(json["gaps"].is_null());
    assert!(json["aggregate"].is_null());

    // Write the full pretty JSON to a fixtures file for inspection.
    write_fixture(
        "proteon_sasa_release.trustreport.json",
        &serde_json::to_string_pretty(&report).unwrap(),
    );
}

#[test]
fn contested_report_includes_challenges_and_gaps() {
    let report = build_contested_report();
    let json = serde_json::to_value(&report).unwrap();

    assert_eq!(json["status"], "contested");
    assert_eq!(
        json["challenges"][0],
        "rev-synthetic-electrostatic-band-too-wide"
    );
    assert_eq!(json["gaps"][0]["author_actionable"], true);
    assert_eq!(json["gaps"][0]["would_satisfy"].as_array().unwrap().len(), 2);
    // Tolerance.against populated with BALL for single-oracle case.
    assert_eq!(json["criteria"][0]["tolerance"]["against"], "BALL");

    write_fixture(
        "proteon_charmm19_contested.trustreport.json",
        &serde_json::to_string_pretty(&report).unwrap(),
    );
}

#[test]
fn review_event_serializes_with_typed_target_and_kind() {
    // Exercises Target::Criterion (the F-PR14 stable form) and
    // ReviewKind::Challenge with adjacent tagging.
    let event = ReviewEvent {
        id: EventId::new("rev-synthetic-electrostatic-band-too-wide"),
        target: Target::Criterion(CriterionId::new(
            "charmm19-electrostatic-vs-ball-rel-err",
        )),
        by: Identity {
            kind: IdentityKind::Human,
            name: "synthetic-reviewer".into(),
            details: vec![IdentityDetail {
                key: "context".into(),
                value: "fit-test Part 4".into(),
            }],
        },
        protocol: Some("typed-trust-fit-test-synthetic-review-v1".into()),
        rationale: "25% band conflates BALL convention divergence with \
                    proteon-side convention errors of comparable magnitude."
            .into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(ClaimId::new(
                "synthetic-charmm19-electrostatic-band-conflates-sources",
            )),
        },
    };
    let json = serde_json::to_value(&event).unwrap();

    assert_eq!(json["target"]["type"], "criterion");
    assert_eq!(json["target"]["data"], "charmm19-electrostatic-vs-ball-rel-err");

    assert_eq!(json["kind"]["type"], "challenge");
    assert_eq!(json["kind"]["data"]["category"]["type"], "weak_statistics");
    assert_eq!(
        json["kind"]["data"]["backed_by"],
        "synthetic-charmm19-electrostatic-band-conflates-sources"
    );

    assert_eq!(json["by"]["kind"], "human");
    assert_eq!(json["by"]["details"][0]["key"], "context");

    write_fixture(
        "synthetic_challenge.event.json",
        &serde_json::to_string_pretty(&event).unwrap(),
    );
}

fn write_fixture(filename: &str, body: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(filename);
    fs::write(&path, body).expect("write fixture");
}
