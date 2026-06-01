//! Manifest → Typed Trust translator tests.
//!
//! Inputs are abridged but otherwise verbatim from real proteon claim
//! files. The full files are at:
//!   /scratch/TMAlign/proteon/evident/claims/sasa.yaml
//!   /scratch/TMAlign/proteon/evident/claims/release_gate.yaml
//!   /scratch/TMAlign/proteon/evident/claims/dssp.yaml

use typed_trust::translate::{
    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
    TranslateError, TranslationContext,
};
use typed_trust::*;

/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
/// `last_verified` block has all null values (the CI replay loop is
/// not populated for this tier).
const PROTEON_SASA_CI_YAML: &str = r#"
claims:
  - id: proteon-sasa-vs-biopython-ci
    title: Proteon SASA agrees with Biopython on a fixed CI fixture
    kind: measurement
    subsystem: sasa
    case: claims/sasa.md
    source: ..
    tier: ci
    trust_strategy:
      - validation
    claim: >
      Proteon's Shrake-Rupley solvent-accessible surface area agrees with
      Biopython's reference Shrake-Rupley implementation on the crambin
      (1crn) fixture to within 2% relative difference on total SASA, on
      every CI run.
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        output: total_sasa
        prose: |
          |proteon_total - biopython_total| / biopython_total < 0.02 on 1crn
    evidence:
      oracle:
        - Biopython
      command: pytest tests/test_sasa.py::TestBiopythonOracle -v
      artifact: pytest console output (CI-tier; no persisted artifact)
    provenance: automatic
    last_verified:
      commit: null
      date: null
      value: null
      corpus_sha: null
    assumptions:
      - Biopython's Bio.PDB.SASA.ShrakeRupley uses the same probe radius.
    failure_modes:
      - Single-oracle agreement can mask a shared convention choice.
"#;

/// proteon-sasa-vs-biopython-release-1k-pdbs — the rich release-tier
/// claim with a populated last_verified block. Verbatim values for the
/// fields that matter; oracle list trimmed to one for the
/// single-oracle path (the real claim has two — Biopython AND
/// FreeSASA — covered by a separate test).
const PROTEON_SASA_RELEASE_YAML: &str = r#"
claims:
  - id: proteon-sasa-vs-biopython-release-1k-pdbs
    title: Proteon SASA tracks Biopython on 1000 random PDBs
    kind: measurement
    subsystem: sasa
    case: claims/sasa.md
    source: ..
    tier: release
    trust_strategy:
      - validation
    claim: >
      Across a 1000-PDB validation corpus proteon total SASA agrees
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
      commit: "4d6ddbec100b810b85c0d2104ecd63d78ac848ec"
      date: "2026-05-11"
      value: 0.0017
      corpus_sha: "b319c47c59871ed3990f81fb025c6ae90abba6adcff0b91ff7f118e41c730a53"
    assumptions:
      - validation/pdbs/ is a representative sample.
    failure_modes:
      - Biopython and FreeSASA disagree by ~0.5-1% on non-standard residues.
"#;

/// release_gate — kind: policy, out of scope per §0.
const PROTEON_POLICY_YAML: &str = r#"
claims:
  - id: proteon-oracle-backed-release-gate
    title: Proteon core numerical claims are release-gated by external oracles
    kind: policy
    case: ../devdocs/ORACLE.md
    source: ..
    tier: release
    trust_strategy:
      - validation
      - understanding
    claim: >
      Core Proteon numerical and structural-biology claims should be
      accepted for release only when they point to an independent oracle.
    evidence:
      oracle:
        - OpenMM
      command: pytest tests/oracle -v
      artifact: validation/report.html
    provenance: human
    assumptions: []
    failure_modes: []
"#;

/// proteon-dssp-vs-pydssp-ci — exercises ComparisonOp::Eq (residue_count
/// absolute_error == 0), the F-PR1 restoration.
const PROTEON_DSSP_YAML: &str = r#"
claims:
  - id: proteon-dssp-vs-pydssp-ci
    title: Proteon DSSP matches pydssp on a 5-structure CI corpus
    kind: measurement
    subsystem: dssp
    case: claims/dssp.md
    source: ..
    tier: ci
    trust_strategy:
      - validation
    claim: >
      For each of five reference structures, proteon's DSSP assigns
      per-residue secondary structure that agrees with pydssp on at
      least 95% of residues.
    tolerances:
      - metric: absolute_error
        op: "=="
        value: 0
        output: residue_count
        prose: |
          Residue counts exactly equal between proteon and pydssp.
      - metric: pass_rate
        op: ">="
        value: 0.95
        output: ss_3class_agreement
        prose: |
          (proteon_3class == pydssp_3class).mean() >= 0.95 per structure.
      - metric: absolute_error
        op: "<"
        value: 0.10
        output: helix_fraction
        prose: |
          |fraction_helix_proteon - fraction_helix_pydssp| < 0.10.
    evidence:
      oracle:
        - pydssp
      command: pytest tests/oracle/test_dssp_oracle.py -v
      artifact: pytest console output (CI-tier; no persisted artifact)
    provenance: automatic
    assumptions:
      - pydssp's installed version is the same one that produced the cited numbers.
    failure_modes:
      - Helix-flavor distinctions are lost in 3-class collapse.
"#;

fn ctx(path: &str) -> TranslationContext {
    TranslationContext {
        now: "2026-06-01T00:00:00Z".into(),
        manifest_path: path.into(),
    }
}

#[test]
fn translates_sasa_ci_claim_into_attested_claim() {
    let manifest = parse_manifest_file(PROTEON_SASA_CI_YAML).unwrap();
    assert_eq!(manifest.claims.len(), 1);

    let ctx = ctx("proteon/evident/claims/sasa.yaml");
    let attested = translate_claim(&ctx, &manifest.claims[0], "claims[0]").unwrap();

    assert_eq!(attested.value.id.as_str(), "proteon-sasa-vs-biopython-ci");
    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
    assert!(attested.value.text.contains("Shrake-Rupley"));
    assert!(attested.value.text.contains("2% relative difference"));
    assert_eq!(attested.value.source.path, "proteon/evident/claims/sasa.yaml");
    assert_eq!(attested.value.source.span, "claims[0]");

    // Extraction from structured manifest is Verified, not Judged,
    // per the §4 footnote in concepts/typed-trust.md.
    match &attested.derivation {
        Derivation::Verified {
            method, ran_by, ..
        } => {
            assert_eq!(ran_by.kind, IdentityKind::Automated);
            assert_eq!(ran_by.name, "typed-trust-translator");
            assert!(method.command.contains("typed-trust translate"));
        }
        other => panic!("expected Verified derivation, got {other:?}"),
    }
}

#[test]
fn translates_single_oracle_tolerance_populates_against() {
    let manifest = parse_manifest_file(PROTEON_SASA_CI_YAML).unwrap();
    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();

    assert_eq!(criteria.len(), 1);
    let t = criteria[0].tolerance.as_ref().unwrap();
    assert_eq!(t.metric, "relative_error");
    assert_eq!(t.op, ComparisonOp::Lt);
    assert_eq!(t.value, 0.02);
    assert_eq!(t.output.as_deref(), Some("total_sasa"));
    // F-PR3 single-oracle case: `against` is populated from the single
    // entry in `evidence.oracle`.
    assert_eq!(t.against.as_deref(), Some("Biopython"));
    assert!(t.prose.contains("biopython_total"));

    // CriterionId is generated deterministically — observations in
    // last_verified Reruns bind to this stable id.
    assert_eq!(
        criteria[0].id.as_str(),
        "proteon-sasa-vs-biopython-ci-criterion-0"
    );
}

#[test]
fn rejects_policy_claim_as_out_of_scope() {
    let manifest = parse_manifest_file(PROTEON_POLICY_YAML).unwrap();
    let ctx = ctx("proteon/evident/claims/release_gate.yaml");
    let result = translate_claim(&ctx, &manifest.claims[0], "claims[0]");

    match result {
        Err(TranslateError::OutOfScope { id, kind }) => {
            assert_eq!(id, "proteon-oracle-backed-release-gate");
            assert_eq!(kind, "policy");
        }
        other => panic!("expected OutOfScope, got {other:?}"),
    }
}

#[test]
fn translates_dssp_tolerances_including_eq_operator() {
    // F-PR1 win: ComparisonOp::Eq is restored for integer/discrete
    // equality assertions (DSSP residue_count parity).
    let manifest = parse_manifest_file(PROTEON_DSSP_YAML).unwrap();
    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();

    assert_eq!(criteria.len(), 3);

    let t0 = criteria[0].tolerance.as_ref().unwrap();
    let t1 = criteria[1].tolerance.as_ref().unwrap();
    let t2 = criteria[2].tolerance.as_ref().unwrap();

    // First tolerance: absolute_error == 0 on residue_count.
    assert_eq!(t0.op, ComparisonOp::Eq);
    assert_eq!(t0.value, 0.0);
    assert_eq!(t0.output.as_deref(), Some("residue_count"));
    assert_eq!(t0.metric, "absolute_error");

    // Second: pass_rate >= 0.95.
    assert_eq!(t1.op, ComparisonOp::GtEq);
    assert_eq!(t1.metric, "pass_rate");

    // Third: absolute_error < 0.10.
    assert_eq!(t2.op, ComparisonOp::Lt);
    assert_eq!(t2.output.as_deref(), Some("helix_fraction"));

    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
    for c in &criteria {
        assert_eq!(
            c.tolerance.as_ref().unwrap().against.as_deref(),
            Some("pydssp")
        );
    }

    // CriterionIds are stable, ordered by tolerance index.
    assert_eq!(
        criteria[0].id.as_str(),
        "proteon-dssp-vs-pydssp-ci-criterion-0"
    );
    assert_eq!(
        criteria[2].id.as_str(),
        "proteon-dssp-vs-pydssp-ci-criterion-2"
    );
}

#[test]
fn rejects_unknown_comparison_op() {
    let bad_yaml = r#"
claims:
  - id: bad-op-claim
    title: ...
    kind: measurement
    case: x.md
    source: ..
    tier: ci
    claim: text
    tolerances:
      - metric: relative_error
        op: "≈"
        value: 0.01
        prose: nonsense
    evidence:
      oracle: [SomeOracle]
      command: cmd
      artifact: a
"#;
    let manifest = parse_manifest_file(bad_yaml).unwrap();
    let result = translate_tolerances(&manifest.claims[0]);

    match result {
        Err(TranslateError::UnknownOp { id, op }) => {
            assert_eq!(id, "bad-op-claim");
            assert_eq!(op, "≈");
        }
        other => panic!("expected UnknownOp, got {other:?}"),
    }
}

#[test]
fn parses_prose_only_tolerance_and_produces_not_assessed() {
    // Codex review #1 + workflow/SCHEMA.md: at research tier a
    // tolerance may carry only `prose` (metric/op/value all absent).
    // The translator must accept this and produce a TranslatedCriterion
    // with tolerance: None; the synthesizer must render NotAssessed.
    let yaml = r#"
claims:
  - id: research-tier-prose-only
    title: Research-tier prose-only tolerance
    kind: measurement
    case: x.md
    source: ..
    tier: research
    claim: deferred-spec
    tolerances:
      - prose: |
          We will quantify alignment quality once we have a
          ground-truth corpus. Until then, the tolerance is the
          maintainer's discretion.
    evidence:
      oracle: [internal]
      command: pytest tests/oracle/test_dssp_oracle.py -v
      artifact: console
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();

    assert_eq!(criteria.len(), 1);
    assert!(criteria[0].tolerance.is_none());
    assert!(criteria[0].prose.contains("quantify alignment quality"));

    // Verify the same claim flows through synthesize() to a
    // NotAssessed criterion result.
    let ctx = ctx("research/manifest.yaml");
    let _claim = translate_claim(&ctx, &manifest.claims[0], "claims[0]").unwrap();
    let evidence: Vec<Evidence> =
        translate_evidence(&ctx, &manifest.claims[0], &criteria)
            .into_iter()
            .collect();
    let report = synthesize(
        ClaimId::new("research-tier-prose-only"),
        criteria,
        &evidence,
        &[],
        &[],
        &std::collections::HashSet::new(),
        "2026-06-01T00:00:00Z".into(),
    );
    let r = &report.criteria[0].result.value;
    assert!(matches!(r, CriterionResult::NotAssessed { .. }), "got {r:?}");
}

#[test]
fn rejects_prose_only_tolerance_at_ci_tier() {
    // Codex round 4: prose-only is the research-tier deferred-spec
    // escape hatch only. CI/release manifests with prose-only must
    // fail translation, not silently translate to NotAssessed +
    // Current.
    let yaml = r#"
claims:
  - id: ci-prose-only-bad
    title: CI claim with prose-only tolerance
    kind: measurement
    case: x.md
    source: ..
    tier: ci
    claim: text
    tolerances:
      - prose: |
          We have not yet decided what to measure here.
    evidence:
      oracle: [Foo]
      command: pytest tests/oracle
      artifact: console
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let result = translate_tolerances(&manifest.claims[0]);
    match result {
        Err(TranslateError::ProseOnlyOutsideResearch { id, tier }) => {
            assert_eq!(id, "ci-prose-only-bad");
            assert_eq!(tier, "ci");
        }
        other => panic!("expected ProseOnlyOutsideResearch, got {other:?}"),
    }
}

#[test]
fn rejects_measurement_claim_without_tolerances() {
    // Codex round 5: kind: measurement requires non-empty tolerances per
    // workflow/SCHEMA.md. Without them, synthesize would emit a Current
    // report with nothing to assess.
    let yaml = r#"
claims:
  - id: measurement-no-tolerances
    title: missing tolerances
    kind: measurement
    case: x.md
    source: ..
    tier: ci
    claim: hand-waved
    evidence:
      oracle: [Foo]
      command: pytest
      artifact: console
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let result = translate_tolerances(&manifest.claims[0]);
    match result {
        Err(TranslateError::MeasurementWithoutTolerances { id }) => {
            assert_eq!(id, "measurement-no-tolerances");
        }
        other => panic!("expected MeasurementWithoutTolerances, got {other:?}"),
    }
}

#[test]
fn rejects_measurement_claim_with_empty_tolerances_list() {
    let yaml = r#"
claims:
  - id: measurement-empty-tolerances
    title: empty tolerances
    kind: measurement
    case: x.md
    source: ..
    tier: ci
    claim: text
    tolerances: []
    evidence:
      oracle: [Foo]
      command: pytest
      artifact: console
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let result = translate_tolerances(&manifest.claims[0]);
    match result {
        Err(TranslateError::MeasurementWithoutTolerances { .. }) => {}
        other => panic!("expected MeasurementWithoutTolerances, got {other:?}"),
    }
}

#[test]
fn rejects_prose_only_tolerance_at_release_tier() {
    let yaml = r#"
claims:
  - id: release-prose-only-bad
    title: Release claim with prose-only tolerance
    kind: measurement
    case: x.md
    source: ..
    tier: release
    claim: text
    tolerances:
      - prose: |
          We have not yet decided what to measure here.
    evidence:
      oracle: [Foo]
      command: pytest tests/oracle
      artifact: console
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let result = translate_tolerances(&manifest.claims[0]);
    match result {
        Err(TranslateError::ProseOnlyOutsideResearch { tier, .. }) => {
            assert_eq!(tier, "release");
        }
        other => panic!("expected ProseOnlyOutsideResearch, got {other:?}"),
    }
}

#[test]
fn rejects_partial_tolerance_with_some_but_not_all_of_metric_op_value() {
    // metric and op present but value absent — schema violation.
    let yaml = r#"
claims:
  - id: partial-tolerance-claim
    title: bad
    kind: measurement
    case: x.md
    source: ..
    tier: ci
    claim: text
    tolerances:
      - metric: relative_error
        op: "<"
        prose: missing value field
    evidence:
      oracle: [Foo]
      command: x
      artifact: y
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let result = translate_tolerances(&manifest.claims[0]);
    match result {
        Err(TranslateError::PartialTolerance { id }) => {
            assert_eq!(id, "partial-tolerance-claim");
        }
        other => panic!("expected PartialTolerance, got {other:?}"),
    }
}

#[test]
fn dssp_claim_translates_with_pydssp_as_inferred_comparison() {
    let manifest = parse_manifest_file(PROTEON_DSSP_YAML).unwrap();
    let ctx = ctx("proteon/evident/claims/dssp.yaml");
    let attested = translate_claim(&ctx, &manifest.claims[0], "claims[0]").unwrap();

    assert_eq!(attested.value.id.as_str(), "proteon-dssp-vs-pydssp-ci");
    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
}

// --- Evidence + last_verified translation ---

#[test]
fn ci_claim_with_null_last_verified_has_empty_reruns() {
    let manifest = parse_manifest_file(PROTEON_SASA_CI_YAML).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("proteon/evident/claims/sasa.yaml");
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap();

    assert_eq!(evidence.id.as_str(), "ev-proteon-sasa-vs-biopython-ci");
    assert_eq!(evidence.for_claim.as_str(), "proteon-sasa-vs-biopython-ci");

    // CI tier without populated last_verified → empty reruns.
    match &evidence.extraction {
        Derivation::Verified { reruns, method, ran_by } => {
            assert!(reruns.is_empty(), "expected empty reruns, got {reruns:?}");
            assert!(method.command.contains("pytest"));
            assert_eq!(ran_by.kind, IdentityKind::Automated); // performer, not judge
            assert_eq!(ran_by.name, "unspecified-runner");
        }
        other => panic!("expected Verified, got {other:?}"),
    }

    // CI tier → Moderate support strength, Moderate confidence.
    match (&evidence.supports.value, &evidence.supports.derivation) {
        (
            SupportRelation::Supports { strength: Strength::Moderate },
            Derivation::Judged { by, confidence, .. },
        ) => {
            assert_eq!(*confidence, Confidence::Moderate);
            // Invariant 9: judge is Human even when provenance is "automatic".
            assert_eq!(by.kind, IdentityKind::Human);
            assert_eq!(by.name, "unspecified");
            let prov = by.details.iter().find(|d| d.key == "manifest_provenance");
            assert_eq!(prov.map(|d| d.value.as_str()), Some("automatic"));
        }
        other => panic!("unexpected supports shape: {other:?}"),
    }
}

#[test]
fn release_claim_with_populated_last_verified_emits_rerun() {
    let manifest = parse_manifest_file(PROTEON_SASA_RELEASE_YAML).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("proteon/evident/claims/sasa.yaml");
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap();

    // Release tier → Strong support, High confidence, provenance: human → Human judge.
    match (&evidence.supports.value, &evidence.supports.derivation) {
        (
            SupportRelation::Supports { strength: Strength::Strong },
            Derivation::Judged { by, confidence, .. },
        ) => {
            assert_eq!(*confidence, Confidence::High);
            let prov = by.details.iter().find(|d| d.key == "manifest_provenance");
            assert_eq!(prov.map(|d| d.value.as_str()), Some("human"));
        }
        other => panic!("unexpected supports shape: {other:?}"),
    }

    // last_verified is fully populated → one Rerun.
    let Derivation::Verified { reruns, .. } = &evidence.extraction else {
        panic!("expected Verified");
    };
    assert_eq!(reruns.len(), 1);
    let rerun = &reruns[0];
    assert_eq!(rerun.at, "2026-05-11");
    assert_eq!(rerun.outcome, ReproductionOutcome::Matched);
    assert_eq!(
        rerun.corpus_sha.as_deref(),
        Some("b319c47c59871ed3990f81fb025c6ae90abba6adcff0b91ff7f118e41c730a53")
    );

    // Observation binds to the first criterion id (shipping convention:
    // last_verified.value is the primary scalar metric).
    assert_eq!(rerun.observed.len(), 1);
    let obs = &rerun.observed[0];
    assert_eq!(
        obs.criterion.as_str(),
        "proteon-sasa-vs-biopython-release-1k-pdbs-criterion-0"
    );
    assert_eq!(obs.value, 0.0017);
}

#[test]
fn evidence_locator_wraps_manifest_artifact_string() {
    let manifest = parse_manifest_file(PROTEON_SASA_RELEASE_YAML).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("proteon/evident/claims/sasa.yaml");
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap();

    match &evidence.locator {
        Locator::Artifact(s) => assert_eq!(s, "validation/results.json"),
        other => panic!("expected Locator::Artifact, got {other:?}"),
    }
}
