//! End-to-end construction of two real shapes from the proteon fit-test:
//! the SASA release-tier claim, and the synthetic Challenge against the
//! CHARMM19+BALL electrostatic band.
//!
//! Purpose: act as the forcing function. Any type that's awkward to
//! construct against a real claim surfaces a design need.

use typed_trust::*;

/// proteon-sasa-vs-biopython-release-1k-pdbs — the rich release-tier
/// claim walked end-to-end in `concepts/typed-trust-proteon-fit.md`
/// Part 1.
#[test]
fn proteon_sasa_release_walkthrough() {
    // §1 — Identities
    let maintainer = Identity {
        kind: IdentityKind::Human,
        name: "proteon maintainer".into(),
        details: vec![], // shipping YAML has no ORCID for provenance: human
    };
    let release_runner = Identity {
        kind: IdentityKind::Automated,
        name: "proteon-release-validator".into(),
        details: vec![IdentityDetail {
            key: "ci_run".into(),
            value: "release-v0.2.0".into(),
        }],
    };

    let pass_at = "2026-05-11T00:00:00Z".to_string();

    // §5 — Claim
    let claim_id = ClaimId::new("proteon-sasa-vs-biopython-release-1k-pdbs");
    let claim = Claim {
        id: claim_id.clone(),
        text: "Across 1000 PDBs proteon total SASA agrees with both \
               Biopython and FreeSASA Shrake-Rupley implementations \
               within tier-specific tolerances."
            .into(),
        kind: ClaimKind::Comparison,
        source: SourceSpan {
            path: "evident/claims/sasa.yaml".into(),
            span: "claims[1]".into(),
        },
        explicit: true,
        decomposes_into: vec![],
        requires_assumptions: vec![],
        metadata: None,
        concordance: None,
    };

    // §7 — Three Criteria from the YAML's three tolerances
    let crit_biopy = CriterionId::new("sasa-biopython-median-rel-err");
    let crit_freesasa = CriterionId::new("sasa-freesasa-median-rel-err");
    let crit_passrate = CriterionId::new("sasa-pass-rate");

    // NOTE (forcing-function finding, see test summary below):
    // CriterionResult attestations have a contrived Derivation::Verified.
    // The synthesizer comparing 0.0017 < 0.005 is reproducible but it's
    // not really a ToolInvocation. Filed as a v0.9 candidate, not blocking.
    let synth_method = ToolInvocation {
        command: "rule:Lt(observed, tolerance)".into(),
        tool_version: "typed-trust-synth 0.1.0".into(),
        env: vec![],
    };
    let synth_verified = |reruns: Vec<Rerun>| Derivation::Verified {
        method: synth_method.clone(),
        ran_by: release_runner.clone(),
        reruns,
    };

    let criteria = vec![
        Criterion {
            id: crit_biopy.clone(),
            name: "Median rel err vs Biopython < 0.5%".into(),
            tolerance: Some(Tolerance {
                metric: "median_relative_error".into(),
                op: ComparisonOp::Lt,
                value: 0.005,
                output: Some("total_sasa".into()),
                against: Some("Biopython".into()), // <-- F-PR3 win
                prose: "Median(|proteon - biopython| / biopython) < 0.005"
                    .into(),
            }),
            result: Attested {
                value: CriterionResult::Pass,
                derivation: synth_verified(vec![]),
                at: pass_at.clone(),
            },
        },
        Criterion {
            id: crit_freesasa.clone(),
            name: "Median rel err vs FreeSASA < 2%".into(),
            tolerance: Some(Tolerance {
                metric: "median_relative_error".into(),
                op: ComparisonOp::Lt,
                value: 0.02,
                output: Some("total_sasa".into()),
                against: Some("FreeSASA".into()), // same metric, different oracle
                prose: "Median(|proteon - freesasa| / freesasa) < 0.02".into(),
            }),
            result: Attested {
                value: CriterionResult::Pass,
                derivation: synth_verified(vec![]),
                at: pass_at.clone(),
            },
        },
        Criterion {
            id: crit_passrate.clone(),
            name: "Pass rate >= 95%".into(),
            tolerance: Some(Tolerance {
                metric: "pass_rate".into(),
                op: ComparisonOp::GtEq,
                value: 0.95,
                output: Some("total_sasa".into()),
                against: None, // pass rate isn't oracle-specific
                prose: "pass / (pass + warn + fail - loading.fail) >= 0.95".into(),
            }),
            result: Attested {
                value: CriterionResult::Pass,
                derivation: synth_verified(vec![]),
                at: pass_at.clone(),
            },
        },
    ];

    // §2/§6 — Evidence (one per oracle, sharing the artifact)
    let evidence_biopython = Evidence {
        id: EvidenceId::new("ev-sasa-biopython-1k"),
        for_claim: claim_id.clone(),
        kind: EvidenceKind::Benchmark,
        locator: Locator::ArtifactInRelease {
            archive: "v0.2.0-evidence.tar.gz".into(),
            path: "validation/results.json".into(),
            sha256: "b319c47c59871ed3990f81fb025c6ae90abba6adcff0b91ff7f118e41c730a53"
                .into(),
        },
        extraction: Derivation::Verified {
            method: ToolInvocation {
                command: "python validation/run_validation.py \
                          --n-structures 1000 --pdb-dir validation/pdbs/ \
                          --output validation/results.json"
                    .into(),
                tool_version: "validation/run_validation.py @ 4d6ddbec".into(),
                env: vec![
                    ("proteon".into(), "0.2.0".into()),
                    ("Biopython".into(), "1.87".into()),
                    ("FreeSASA".into(), "2.2.1".into()),
                    ("python".into(), "3.12".into()),
                ],
            },
            ran_by: release_runner.clone(),
            reruns: vec![Rerun {
                at: pass_at.clone(),
                by: release_runner.clone(),
                observed: vec![MetricObservation {
                    criterion: crit_biopy.clone(), // bind to specific criterion
                    value: 0.0017,
                    unit: None,
                }],
                corpus_sha: Some(
                    "b319c47c59871ed3990f81fb025c6ae90abba6adcff0b91ff7f118e41c730a53"
                        .into(),
                ),
                outcome: ReproductionOutcome::Matched,
            }],
        },
        supports: Attested {
            value: SupportRelation::Supports {
                strength: Strength::Strong,
            },
            derivation: Derivation::Judged {
                by: maintainer.clone(),
                protocol: Some("proteon-release-review-v1".into()),
                rationale: "Observed 0.0017 < tolerance 0.005 on 1000 PDBs."
                    .into(),
                confidence: Confidence::High,
            },
            at: pass_at.clone(),
        },
        replay_status: Default::default(),
        replay_reason: None,
    };

    // §6 — Peer endorsement of the SupportRelation
    let endorsement = ReviewEvent {
        id: EventId::new("rev-maintainer-supports-2026-05-11"),
        target: Target::SupportRelation(evidence_biopython.id.clone()),
        by: maintainer.clone(),
        protocol: Some("proteon-release-review-v1".into()),
        rationale: "Verified observed value against artifact on 4d6ddbec.".into(),
        at: pass_at.clone(),
        kind: ReviewKind::Endorse,
    };

    // §7 — TrustReport
    let report = TrustReport {
        claim: claim_id.clone(),
        criteria,
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
        status: RenderStatus::Current,
    };

    // Sanity
    assert_eq!(report.criteria.len(), 3);
    assert_eq!(report.status, RenderStatus::Current);
    assert_eq!(endorsement.kind, ReviewKind::Endorse);
    assert!(matches!(evidence_biopython.kind, EvidenceKind::Benchmark));
    assert_eq!(claim.kind, ClaimKind::Comparison);

    // The F-PR3 win: each tolerance binds to its oracle.
    let biopy_tol = report.criteria[0].tolerance.as_ref().unwrap();
    let freesasa_tol = report.criteria[1].tolerance.as_ref().unwrap();
    assert_eq!(biopy_tol.against.as_deref(), Some("Biopython"));
    assert_eq!(freesasa_tol.against.as_deref(), Some("FreeSASA"));
    assert_eq!(biopy_tol.metric, freesasa_tol.metric); // same metric name…
    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
}

/// Synthetic Challenge against the CHARMM19+BALL electrostatic
/// tolerance band. Exercises:
/// - `Challenge { backed_by: Some(...) }` + backing claim coupling
/// - `Target::Criterion(CriterionId)` — the F-PR14 stable form, NOT
///   the ReportId-bound CriterionResult form
/// - `ChallengeCategory::WeakStatistics` for tolerance-calibration objections
#[test]
fn synthetic_challenge_against_charmm_electrostatic_band() {
    let challenger = Identity {
        kind: IdentityKind::Human,
        name: "synthetic-reviewer".into(),
        details: vec![IdentityDetail {
            key: "context".into(),
            value: "fit-test Part 4".into(),
        }],
    };

    // Backing claim
    let backing_claim_id =
        ClaimId::new("synthetic-charmm19-electrostatic-band-conflates-sources");
    let _backing_claim = Claim {
        id: backing_claim_id.clone(),
        text: "A 25% relative_error band on CHARMM19+BALL electrostatic \
               cannot distinguish BALL's documented convention divergence \
               from a proteon-side convention error of comparable magnitude."
            .into(),
        kind: ClaimKind::Causal,
        source: SourceSpan {
            path: "concepts/typed-trust-proteon-fit.md".into(),
            span: "Part 4 synthetic challenge".into(),
        },
        explicit: true,
        decomposes_into: vec![],
        requires_assumptions: vec![],
        metadata: None,
        concordance: None,
    };

    // The criterion id is stable across re-synthesis (per Criterion.id
    // doc-comment). Target::Criterion captures the tolerance-definition
    // attack without needing a ReportId snapshot — this is what F-PR14
    // gave us.
    let crit_electrostatic = CriterionId::new("charmm19-electrostatic-vs-ball-rel-err");

    let challenge = ReviewEvent {
        id: EventId::new("rev-synthetic-electrostatic-band-too-wide"),
        target: Target::Criterion(crit_electrostatic.clone()),
        by: challenger,
        protocol: Some("typed-trust-fit-test-synthetic-review-v1".into()),
        rationale: "25% band conflates BALL convention divergence with \
                    proteon-side convention errors of comparable magnitude. \
                    The 2026-05-02 dist-dep dielectric bug was caught by \
                    triangulation, not by this gate."
            .into(),
        at: "2026-06-01T00:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(backing_claim_id.clone()),
        },
    };

    // The F-PR14 split: this challenge attacks the tolerance definition,
    // not a specific synthesized result. Target::Criterion (stable),
    // not Target::CriterionResult (snapshot-bound).
    assert!(matches!(challenge.target, Target::Criterion(_)));

    // The Challenge is substantive: backed_by is Some, so it can move
    // render status to Contested per invariant 6.
    if let ReviewKind::Challenge {
        backed_by,
        category,
    } = &challenge.kind
    {
        assert_eq!(backed_by.as_ref(), Some(&backing_claim_id));
        assert_eq!(*category, ChallengeCategory::WeakStatistics);
    } else {
        panic!("expected Challenge");
    }
}

/// Sanity: the degraded `provenance: human` identity construction
/// matches what the translator should emit. F-PR4 from the fit-test.
#[test]
fn degraded_provenance_human_identity() {
    let id = Identity::unspecified_human_from_manifest();
    assert_eq!(id.kind, IdentityKind::Human);
    assert_eq!(id.name, "unspecified");
    assert_eq!(id.details.len(), 1);
    assert_eq!(id.details[0].key, "manifest_provenance");
    assert_eq!(id.details[0].value, "human");
}
