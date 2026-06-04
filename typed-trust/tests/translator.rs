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
        translate_evidence(&ctx, &manifest.claims[0], &criteria).unwrap()
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
fn rejects_measurement_claim_without_evidence_block() {
    // Codex round 7: kind: measurement requires an evidence block per
    // workflow/SCHEMA.md. Without it, synthesize would emit a Current
    // report with NotAssessed criteria — an unevidenced measurement
    // looking accepted.
    let yaml = r#"
claims:
  - id: measurement-no-evidence
    title: missing evidence
    kind: measurement
    case: x.md
    source: ..
    tier: ci
    claim: text
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.01
        prose: rel err < 1%
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
    let ctx = ctx("manifest.yaml");
    let result = translate_evidence(&ctx, &manifest.claims[0], &criteria);
    match result {
        Err(TranslateError::MeasurementWithoutEvidence { id }) => {
            assert_eq!(id, "measurement-no-evidence");
        }
        other => panic!("expected MeasurementWithoutEvidence, got {other:?}"),
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
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap().unwrap();

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
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap().unwrap();

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
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap().unwrap();

    match &evidence.locator {
        Locator::Artifact(s) => assert_eq!(s, "validation/results.json"),
        other => panic!("expected Locator::Artifact, got {other:?}"),
    }
}

// ----------------------------------------------------------------------
// Phase 5 PR1: replay_status + replay_reason
//
// Load-bearing tests for the new evidence-block fields. The schema
// distinguishes three states a replay path can be in:
//   - available           — framework can run the command (Phase 1 default)
//   - not_attempted       — no replay run; no reason allowed
//   - unavailable_artifacts — extractor verified replay impossible; reason required
//
// Pair-validator rules (translate-time):
//   (available, null)              → OK
//   (not_attempted, null)          → OK (default for hand-authored manifests)
//   (unavailable_artifacts, <any>) → OK
//   anything else                  → translation error
// ----------------------------------------------------------------------

/// Extracted-from-paper claim that cannot be replayed because the paper's
/// code is private. The extractor (or the curator) records this with
/// `replay_status: unavailable_artifacts` and `replay_reason:
/// code_private` so downstream queries can distinguish "not run yet"
/// from "cannot be run from what we have."
const EXTRACTED_PAPER_NO_REPLAY_YAML: &str = r#"
claims:
  - id: cool-paper-rmsd-vs-baseline
    title: Cool Paper claims median RMSD below 0.5 angstrom on BPTI suite
    kind: measurement
    tier: research
    case: source/cited.md#claim-1
    source: ..
    claim: >
      Median RMSD < 0.5 Å against Baseline X on the BPTI test suite
      (n=1000), per Section 4.2 Table 3.
    tolerances:
      - metric: median_rmsd
        op: "<"
        value: 0.5
        prose: |
          paper Table 3 row "ours": median RMSD = 0.42; bound 0.5 stated
          in cited sentence.
    evidence:
      oracle: [Paper-Authority]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: code_private
    provenance: extracted-from-paper
    last_verified:
      commit: null
      date: null
      value: null
      corpus_sha: null
"#;

#[test]
fn evidence_carries_replay_status_unavailable_artifacts_with_reason() {
    let manifest = parse_manifest_file(EXTRACTED_PAPER_NO_REPLAY_YAML).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("extracted/cool-paper/evident.yaml");
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap().unwrap();

    assert_eq!(
        evidence.replay_status,
        typed_trust::evidence::ReplayStatus::UnavailableArtifacts
    );
    assert_eq!(
        evidence.replay_reason,
        Some(typed_trust::evidence::ReplayReason::CodePrivate)
    );
}

#[test]
fn evidence_default_replay_status_is_not_attempted_with_null_reason() {
    // The CI fixture YAML has no replay_status / replay_reason — the
    // current behaviour. Defaulting to NotAttempted + None matches what
    // hand-authored manifests have always meant: nobody has run this yet.
    let manifest = parse_manifest_file(PROTEON_SASA_CI_YAML).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("proteon/evident/claims/sasa.yaml");
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap().unwrap();

    assert_eq!(
        evidence.replay_status,
        typed_trust::evidence::ReplayStatus::NotAttempted
    );
    assert!(evidence.replay_reason.is_none());
}

#[test]
fn evidence_rejects_not_attempted_paired_with_a_reason() {
    // Illegal pair: not_attempted means "nobody tried"; a reason claims
    // a specific blocker. Combining them is incoherent and the
    // pair-validator rejects at translate time.
    let yaml = r#"
claims:
  - id: bad-pair-claim
    title: bad pair example
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: bad pair
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: |
          example
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
      replay_status: not_attempted
      replay_reason: code_private
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("any.yaml");
    let err = translate_evidence(&ctx, mc, &criteria).unwrap_err();

    match err {
        TranslateError::IllegalReplayPair { id, status, reason } => {
            assert_eq!(id, "bad-pair-claim");
            assert_eq!(status, "not_attempted");
            assert_eq!(reason.as_deref(), Some("code_private"));
        }
        other => panic!("expected IllegalReplayPair, got {other:?}"),
    }
}

#[test]
fn evidence_rejects_unavailable_artifacts_without_a_reason() {
    // Illegal pair the other way: unavailable_artifacts asserts a
    // blocker exists; without a reason there's nothing to query on.
    let yaml = r#"
claims:
  - id: bad-pair-no-reason
    title: bad pair no reason
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: bad pair no reason
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: |
          example
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
      replay_status: unavailable_artifacts
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("any.yaml");
    let err = translate_evidence(&ctx, mc, &criteria).unwrap_err();

    match err {
        TranslateError::IllegalReplayPair { id, status, reason } => {
            assert_eq!(id, "bad-pair-no-reason");
            assert_eq!(status, "unavailable_artifacts");
            assert!(reason.is_none());
        }
        other => panic!("expected IllegalReplayPair, got {other:?}"),
    }
}

#[test]
fn evidence_rejects_unknown_replay_status_string() {
    let yaml = r#"
claims:
  - id: unknown-status
    title: unknown status example
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: unknown status example
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: |
          example
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
      replay_status: maybe_someday
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("any.yaml");
    let err = translate_evidence(&ctx, mc, &criteria).unwrap_err();

    match err {
        TranslateError::InvalidReplayStatus { id, value } => {
            assert_eq!(id, "unknown-status");
            assert_eq!(value, "maybe_someday");
        }
        other => panic!("expected InvalidReplayStatus, got {other:?}"),
    }
}

#[test]
fn evidence_rejects_absent_status_with_present_reason() {
    // Codex code review F-PR1-CR1 coverage: when status is absent it
    // defaults to NotAttempted; pairing the default with a present
    // reason is the only illegal-pair case that exercises the fallback
    // string in IllegalReplayPair.
    let yaml = r#"
claims:
  - id: absent-status-with-reason
    title: absent status with reason
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: absent status with reason
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: |
          example
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
      replay_reason: data_unavailable
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("any.yaml");
    let err = translate_evidence(&ctx, mc, &criteria).unwrap_err();

    match err {
        TranslateError::IllegalReplayPair { id, status, reason } => {
            assert_eq!(id, "absent-status-with-reason");
            assert_eq!(status, "not_attempted");
            assert_eq!(reason.as_deref(), Some("data_unavailable"));
        }
        other => panic!("expected IllegalReplayPair, got {other:?}"),
    }
}

#[test]
fn evidence_rejects_available_paired_with_reason() {
    // Codex code review F-PR1-CR1 coverage: `available + reason` is
    // illegal too (a replay path that succeeded shouldn't carry a
    // blocker). The existing tests covered `not_attempted + reason`
    // but not this side of the same rule.
    let yaml = r#"
claims:
  - id: available-with-reason
    title: available with reason
    kind: measurement
    tier: ci
    case: src.md
    source: ..
    claim: available with reason
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: |
          example
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
      replay_status: available
      replay_reason: data_unavailable
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("any.yaml");
    let err = translate_evidence(&ctx, mc, &criteria).unwrap_err();

    match err {
        TranslateError::IllegalReplayPair { id, status, reason } => {
            assert_eq!(id, "available-with-reason");
            assert_eq!(status, "available");
            assert_eq!(reason.as_deref(), Some("data_unavailable"));
        }
        other => panic!("expected IllegalReplayPair, got {other:?}"),
    }
}

#[test]
fn evidence_parses_all_ten_replay_reason_values() {
    use typed_trust::evidence::ReplayReason;
    let cases = [
        ("code_private", ReplayReason::CodePrivate),
        ("data_unavailable", ReplayReason::DataUnavailable),
        ("license_restricted", ReplayReason::LicenseRestricted),
        ("compute_unavailable", ReplayReason::ComputeUnavailable),
        ("environment_unavailable", ReplayReason::EnvironmentUnavailable),
        ("dependency_unavailable", ReplayReason::DependencyUnavailable),
        ("external_service_unavailable", ReplayReason::ExternalServiceUnavailable),
        ("benchmark_unspecified", ReplayReason::BenchmarkUnspecified),
        ("instructions_missing", ReplayReason::InstructionsMissing),
        ("requires_human_evaluation", ReplayReason::RequiresHumanEvaluation),
    ];
    for (reason_str, expected) in cases {
        let yaml = format!(
            r#"
claims:
  - id: reason-{reason_str}
    title: reason coverage test
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: reason coverage
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: |
          example
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
      replay_status: unavailable_artifacts
      replay_reason: {reason_str}
"#
        );
        let manifest = parse_manifest_file(&yaml).unwrap_or_else(|e| {
            panic!("parse failed for {reason_str}: {e:?}")
        });
        let mc = &manifest.claims[0];
        let criteria = translate_tolerances(mc).unwrap();
        let ctx = ctx("any.yaml");
        let evidence = translate_evidence(&ctx, mc, &criteria)
            .unwrap_or_else(|e| panic!("translate failed for {reason_str}: {e:?}"))
            .unwrap();
        assert_eq!(
            evidence.replay_reason,
            Some(expected),
            "wrong enum for {reason_str}",
        );
    }
}

// ----------------------------------------------------------------------
// Phase 5 PR2: structured provenance + source_context
//
// The legacy schema treated `provenance` as a single string:
//   provenance: automatic | human | peer-reviewed
//
// Phase 5 needs to express extracted-from-paper / extracted-from-repo
// claims, which carry additional fields (source_id, source_sha,
// source_context, extractor, curator). The PR2 contract:
//
//   provenance: <legacy_string>          # still accepted, unchanged behaviour
//   # OR
//   provenance:
//     kind: extracted-from-paper | extracted-from-repo | <legacy>
//     source_id: <opaque string>
//     source_sha: <hex>
//     source_context: repo_authored | copied_external_text | unknown
//     extractor:
//       model: <name>
//       model_version: <date or version>
//       extracted_at: <iso timestamp>
//     curator: <free-form, null at extraction time>
//
// The translate layer normalises both forms into ManifestProvenance.
// Legacy callers (judge_identity_for_provenance) consume the
// effective kind string via a helper so the existing flow is
// unchanged.
// ----------------------------------------------------------------------

#[test]
fn legacy_string_provenance_still_parses() {
    // The CI fixture uses `provenance: automatic` (legacy string form).
    // PR2 must not break it.
    let manifest = parse_manifest_file(PROTEON_SASA_CI_YAML).unwrap();
    let mc = &manifest.claims[0];
    let provenance = mc.provenance.as_ref().expect("legacy provenance set");
    assert_eq!(provenance.effective_kind(), "automatic");
    assert!(provenance.source_context().is_none());
    assert!(provenance.source_id().is_none());
}

#[test]
fn structured_provenance_with_source_context_parses() {
    let yaml = r#"
claims:
  - id: extracted-repo-claim
    title: extracted repo claim with copied marketing text
    kind: measurement
    tier: research
    case: source/cited.md#claim-1
    source: ..
    claim: copied marketing
    tolerances:
      - metric: throughput
        op: ">"
        value: 1000.0
        prose: |
          README says "handles >1000 requests/sec" but text appears
          verbatim on vendor's marketing site
    evidence:
      oracle: [Repo-README]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: instructions_missing
    provenance:
      kind: extracted-from-repo
      source_id: github:org/repo@deadbeef
      source_sha: 0123456789abcdef
      source_context: copied_external_text
      extractor:
        model: claude-opus-4-7
        model_version: "20260601"
        extracted_at: "2026-09-14T10:00:00Z"
      curator: null
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let provenance = mc.provenance.as_ref().expect("structured provenance set");

    assert_eq!(provenance.effective_kind(), "extracted-from-repo");
    assert_eq!(provenance.source_context(), Some("copied_external_text"));
    assert_eq!(provenance.source_id(), Some("github:org/repo@deadbeef"));
    assert_eq!(provenance.source_sha(), Some("0123456789abcdef"));
    assert_eq!(
        provenance.extractor_model(),
        Some("claude-opus-4-7")
    );
}

#[test]
fn structured_provenance_without_optional_fields_parses() {
    // The minimum structured form: just `kind`. All other fields
    // optional. Lets a manifest declare extracted-from-paper without
    // committing to a particular extractor model or sha.
    let yaml = r#"
claims:
  - id: minimal-structured-provenance
    title: minimal structured provenance
    kind: measurement
    tier: research
    case: source/cited.md#claim-1
    source: ..
    claim: minimal structured provenance
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: minimal
    evidence:
      oracle: [Manual]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: data_unavailable
    provenance:
      kind: extracted-from-paper
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let provenance = mc.provenance.as_ref().unwrap();

    assert_eq!(provenance.effective_kind(), "extracted-from-paper");
    assert!(provenance.source_context().is_none());
    assert!(provenance.source_id().is_none());
}

#[test]
fn structured_provenance_rejects_unknown_source_context_value_at_parse_time() {
    // Codex F-PR2-CR1 fix: source_context is a typed enum, so an
    // unknown value fails at parse time. This closes the MCP bypass
    // (list_claims would have surfaced the raw string).
    let yaml = r#"
claims:
  - id: bad-source-context
    title: bad source context
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: bad source context
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: bad
    evidence:
      oracle: [Manual]
      command: "no-replay-path"
      artifact: src.md
      replay_status: unavailable_artifacts
      replay_reason: data_unavailable
    provenance:
      kind: extracted-from-paper
      source_context: completely_made_up
"#;
    let err = parse_manifest_file(yaml).unwrap_err();
    match err {
        TranslateError::Yaml(msg) => {
            assert!(
                msg.contains("source_context") || msg.contains("variant"),
                "expected yaml error naming source_context/variant, got: {msg}"
            );
        }
        other => panic!("expected Yaml parse error, got {other:?}"),
    }
}

#[test]
fn structured_provenance_rejects_unknown_field_at_parse_time() {
    // Codex F-PR2-CR2 fix: deny_unknown_fields on ProvenanceBlock
    // catches typos like `source_contxt:` at parse time instead of
    // silently dropping them.
    let yaml = r#"
claims:
  - id: typo-claim
    title: typo claim
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: typo claim
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: typo
    evidence:
      oracle: [Manual]
      command: "no-replay-path"
      artifact: src.md
      replay_status: unavailable_artifacts
      replay_reason: data_unavailable
    provenance:
      kind: extracted-from-paper
      source_contxt: repo_authored
"#;
    let err = parse_manifest_file(yaml).unwrap_err();
    match err {
        TranslateError::Yaml(msg) => {
            // Untagged-enum dispatch produces a less precise error
            // than naming the typo directly ("did not match any
            // variant of untagged enum ManifestProvenance"). A custom
            // Deserialize impl would name the typo; that's a
            // follow-up. The important guarantee is that the typo
            // does NOT silently parse — without deny_unknown_fields
            // the manifest would have parsed and the field would
            // have been dropped.
            assert!(
                msg.contains("ManifestProvenance")
                    || msg.contains("source_contxt")
                    || msg.contains("unknown field"),
                "expected yaml error rejecting the typo, got: {msg}"
            );
        }
        other => panic!("expected Yaml parse error, got {other:?}"),
    }
}

#[test]
fn structured_provenance_all_three_source_context_values_parse() {
    // Codex F-PR2-CR3 coverage: each legal source_context value
    // round-trips.
    for (yaml_value, expected) in [
        ("repo_authored", "repo_authored"),
        ("copied_external_text", "copied_external_text"),
        ("unknown", "unknown"),
    ] {
        let yaml = format!(
            r#"
claims:
  - id: sc-{yaml_value}
    title: sc table test
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: sc table test
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: sc table test
    evidence:
      oracle: [Manual]
      command: "no-replay-path"
      artifact: src.md
      replay_status: unavailable_artifacts
      replay_reason: data_unavailable
    provenance:
      kind: extracted-from-paper
      source_context: {yaml_value}
"#
        );
        let manifest = parse_manifest_file(&yaml)
            .unwrap_or_else(|e| panic!("parse failed for {yaml_value}: {e:?}"));
        let mc = &manifest.claims[0];
        let provenance = mc.provenance.as_ref().unwrap();
        assert_eq!(
            provenance.source_context(),
            Some(expected),
            "wrong projection for {yaml_value}",
        );
    }
}

#[test]
fn absent_provenance_field_yields_none() {
    // A claim with no provenance field at all is valid and produces
    // ManifestClaim.provenance == None. Used downstream as "legacy
    // hand-authored, no extra context."
    let yaml = r#"
claims:
  - id: no-provenance
    title: no provenance field
    kind: measurement
    tier: ci
    source: .
    claim: no provenance field
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: stay under 2 percent
    evidence:
      oracle: [Biopython]
      command: pytest
      artifact: out.json
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    assert!(
        mc.provenance.is_none(),
        "expected provenance=None, got {:?}",
        mc.provenance
    );
}

#[test]
fn structured_provenance_kind_routes_through_judge_identity() {
    // Existing translate_evidence path uses provenance.kind to pick
    // the judge identity. Structured form must route the same way.
    let yaml = r#"
claims:
  - id: structured-routes-judge
    title: structured routes judge
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: structured routes judge
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: example
    evidence:
      oracle: [Manual]
      command: "no-replay-path"
      artifact: src.md
      replay_status: unavailable_artifacts
      replay_reason: data_unavailable
    provenance:
      kind: extracted-from-paper
      source_context: unknown
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    let ctx = ctx("any.yaml");
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap().unwrap();

    // The supports derivation's judge identity should carry the
    // provenance kind, just like the legacy "automatic" path did.
    if let Derivation::Judged { by, .. } = &evidence.supports.derivation {
        assert_eq!(by.kind, IdentityKind::Human);
        // Identity carries the effective provenance kind in its detail
        // pairs.
        let has_kind = by
            .details
            .iter()
            .any(|d| d.key == "manifest_provenance" && d.value == "extracted-from-paper");
        assert!(has_kind, "judge identity missing provenance kind: {by:?}");
    } else {
        panic!("expected Judged derivation, got {:?}", evidence.supports.derivation);
    }
}

// ----------------------------------------------------------------------
// PR5b: metadata_compatibility claim kind
// ----------------------------------------------------------------------

#[test]
fn metadata_claim_translates_with_metadata_block() {
    let yaml = r#"
claims:
  - id: pdbtbx-rust-msrv
    title: pdbtbx requires Rust MSRV 1.67+
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: |
      pdbtbx's Cargo.toml declares rust-version = "1.67"
    metadata:
      field: rust_msrv
      declared_value: "1.67"
      source_file: Cargo.toml
      source_path: package.rust-version
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    assert_eq!(attested.value.kind, ClaimKind::MetadataCompatibility);
    assert_eq!(attested.value.id.as_str(), "pdbtbx-rust-msrv");
}

// PR5c: the manifest's `metadata:` block must be lifted onto the typed
// Claim so the render layer (which reaches the declaration via
// RenderInput) can surface it. Without this the metadata block parses
// but is dropped on the floor between translate and render.
#[test]
fn metadata_claim_lifts_block_onto_typed_claim_pr5c() {
    let yaml = r#"
claims:
  - id: pdbtbx-rust-msrv
    title: pdbtbx requires Rust MSRV 1.67+
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: pdbtbx declares rust-version = "1.67" in Cargo.toml
    metadata:
      field: rust_msrv
      declared_value: "1.67"
      source_file: Cargo.toml
      source_path: package.rust-version
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    let md = attested
        .value
        .metadata
        .as_ref()
        .expect("metadata declaration present on typed Claim");
    assert_eq!(md.field, "rust_msrv");
    assert_eq!(md.declared_value, "1.67");
    assert_eq!(md.source_file, "Cargo.toml");
    assert_eq!(md.source_path, "package.rust-version");
}

// PR5c: measurement claims do NOT carry a metadata declaration even
// though the struct field exists — keeps the typed Claim's two paths
// disjoint at the type level.
#[test]
fn measurement_claim_has_no_metadata_field_on_typed_claim_pr5c() {
    let yaml = r#"
claims:
  - id: m
    title: measurement
    kind: measurement
    tier: research
    source: .
    claim: c
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: ok
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    assert!(attested.value.metadata.is_none());
}

#[test]
fn metadata_claim_without_metadata_block_is_rejected() {
    let yaml = r#"
claims:
  - id: missing-meta
    title: missing metadata block
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: missing metadata block
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    match err {
        TranslateError::MetadataClaimMissingBlock { id } => {
            assert_eq!(id, "missing-meta");
        }
        other => panic!("expected MetadataClaimMissingBlock, got {other:?}"),
    }
}

#[test]
fn metadata_claim_with_tolerances_is_rejected() {
    let yaml = r#"
claims:
  - id: bad-meta
    title: metadata claim with tolerances
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: bad meta
    metadata:
      field: x
      declared_value: "1"
      source_file: pyproject.toml
      source_path: x
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: should not be allowed
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::MetadataClaimCarriesTolerances { .. }),
        "expected MetadataClaimCarriesTolerances, got {err:?}",
    );
}

#[test]
fn measurement_claim_with_metadata_block_is_rejected() {
    let yaml = r#"
claims:
  - id: bad-measurement
    title: measurement claim with metadata block
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: should not have metadata
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: ok
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
    metadata:
      field: x
      declared_value: "1"
      source_file: pyproject.toml
      source_path: x
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::MeasurementClaimCarriesMetadata { .. }),
        "expected MeasurementClaimCarriesMetadata, got {err:?}",
    );
}

#[test]
fn metadata_claim_emits_no_criteria() {
    let yaml = r#"
claims:
  - id: pkg-python
    title: package requires Python >= 3.10
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: pkg requires Python >= 3.10
    metadata:
      field: python_version_requirement
      declared_value: ">=3.10"
      source_file: pyproject.toml
      source_path: project.requires-python
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    assert!(
        criteria.is_empty(),
        "metadata claim should have empty criteria, got {criteria:?}",
    );
}

#[test]
fn metadata_claim_emits_no_evidence() {
    let yaml = r#"
claims:
  - id: pkg-rust
    title: package requires Rust MSRV 1.67
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: pkg requires Rust MSRV 1.67
    metadata:
      field: rust_msrv
      declared_value: "1.67"
      source_file: Cargo.toml
      source_path: package.rust-version
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let criteria = translate_tolerances(mc).unwrap();
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap();
    assert!(
        evidence.is_none(),
        "metadata claim should produce no Evidence, got Some(_)",
    );
}

#[test]
fn metadata_claim_rejects_unknown_field_in_block() {
    let yaml = r#"
claims:
  - id: bad-extra-field
    title: extra field in metadata
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: bad
    metadata:
      field: x
      declared_value: "1"
      source_file: pyproject.toml
      source_path: x
      unknown_field: this should be rejected
"#;
    let err = parse_manifest_file(yaml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown_field") || msg.contains("unknown field"),
        "expected error mentioning the unknown field, got: {msg}",
    );
}

#[test]
fn metadata_claim_with_evidence_block_is_rejected() {
    // Codex F-PR5b-CR1 (P2): the disjointness rule must reject
    // `evidence:` on a metadata claim. The declaration IS the
    // evidence; carrying a command would be misleading.
    let yaml = r#"
claims:
  - id: bad-meta-evidence
    title: metadata claim with evidence
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: bad
    metadata:
      field: x
      declared_value: "1"
      source_file: pyproject.toml
      source_path: x
    evidence:
      oracle: [Manual]
      command: echo no
      artifact: out.txt
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::MetadataClaimCarriesEvidence { .. }),
        "expected MetadataClaimCarriesEvidence, got {err:?}",
    );
}

// ----------------------------------------------------------------------
// PR5f: behavioral_concordance translator + Manifest deserialization
// ----------------------------------------------------------------------

#[test]
fn behavioral_concordance_numeric_band_translates_with_full_block() {
    let yaml = r#"
claims:
  - id: rustims-fragpipe-fdr-10k-concords-meier
    title: FragPipe FDR on rustims-simulated HLA-I 10k tracks Meier 2024
    kind: behavioral_concordance
    tier: research
    claim: |
      FragPipe v22's empirical true FDR on rustims-simulated HLA-I 10k
      lies within 0.5 pp of Meier 2024's measured value.
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: fragpipe.hla_10k.fdr_pct
        epsilon: 0.5
        prior_value: 1.5
      paper_locator: source/cited.md#rustims-fragpipe-fdr-10k
      prior_binding:
        prior_unit: percentage_points
        prior_metric_definition: |
          Empirical true FDR after target-decoy q<=0.01 filter.
        locator: "Meier 2024 Table 3 row 'FragPipe v22 HLA-I 10k measured'"
        prior_extraction_note: "Curator verified Table 3 print version 2026-XX"
        source_id: "doi:10.1038/PLACEHOLDER"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    assert_eq!(attested.value.kind, ClaimKind::BehavioralConcordance);
    let cd = attested.value.concordance.as_ref().expect("concordance present");
    match &cd.pattern {
        typed_trust::ConcordancePattern::NumericBand {
            metric_path,
            epsilon,
            prior_value,
        } => {
            assert_eq!(metric_path, "fragpipe.hla_10k.fdr_pct");
            assert_eq!(*epsilon, 0.5);
            assert_eq!(*prior_value, 1.5);
        }
        other => panic!("expected NumericBand, got {other:?}"),
    }
    assert_eq!(cd.paper_locator, "source/cited.md#rustims-fragpipe-fdr-10k");
    assert_eq!(cd.prior_binding.prior_unit, "percentage_points");
    assert_eq!(cd.prior_binding.source_id, "doi:10.1038/PLACEHOLDER");
}

#[test]
fn behavioral_concordance_ordinal_match_keyset_alignment_enforced() {
    // entity_to_path has key "FragPipe_v22" but prior_value has
    // "FragPipe_v23" — keyset mismatch must be rejected.
    let yaml = r#"
claims:
  - id: rustims-tools-fdr-ordering-concords-meier
    title: Tool FDR ordering on rustims-simulated HLA-I 10k
    kind: behavioral_concordance
    tier: research
    claim: |
      Tool ordering by FDR on rustims-simulated data matches Meier 2024.
    concordance:
      pattern:
        pattern_kind: ordinal_match
        entity_to_path:
          FragPipe_v22: fragpipe_v22.hla_10k.fdr_pct
          PEAKS_XPro: peaks_xpro.hla_10k.fdr_pct
        direction: lower_is_better
        tie_policy: adjacent_swap_ok
        prior_value:
          FragPipe_v23: 1.5
          PEAKS_XPro: 1.8
      paper_locator: source/cited.md#rustims-fdr-ordering
      prior_binding:
        prior_unit: percentage_points
        prior_metric_definition: "Empirical true FDR per Meier 2024 §Methods."
        locator: "Meier 2024 Table 3 across two tool rows"
        prior_extraction_note: "Curator verified ordering"
        source_id: "doi:10.1038/PLACEHOLDER"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceOrdinalKeyMismatch { .. }),
        "expected ConcordanceOrdinalKeyMismatch, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_same_order_rejects_non_positive_prior() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    claim: c
    concordance:
      pattern:
        pattern_kind: same_order_of_magnitude
        metric_path: foo.bar
        prior_value: 0.0
        zero_policy: not_assessed
      paper_locator: src.md
      prior_binding:
        prior_unit: count
        prior_metric_definition: "x"
        locator: "x"
        prior_extraction_note: "x"
        source_id: "x"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceSameOrderNonPositivePrior { .. }),
        "expected ConcordanceSameOrderNonPositivePrior, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_relative_band_rejects_ratio_at_or_below_one() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    claim: c
    concordance:
      pattern:
        pattern_kind: relative_band
        metric_path: foo.bar
        ratio: 1.0
        prior_value: 10.0
      paper_locator: src.md
      prior_binding:
        prior_unit: ms
        prior_metric_definition: "runtime"
        locator: "x"
        prior_extraction_note: "x"
        source_id: "x"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceRelativeBandRatioTooSmall { .. }),
        "expected ConcordanceRelativeBandRatioTooSmall, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_rejects_top_level_source() {
    // Concordance claims must NOT carry the measurement-flavored
    // `source` field (v4 design's schema-exception commitment).
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    source: src.md
    claim: c
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: foo.bar
        epsilon: 0.1
        prior_value: 1.0
      paper_locator: src.md
      prior_binding:
        prior_unit: x
        prior_metric_definition: "x"
        locator: "x"
        prior_extraction_note: "x"
        source_id: "x"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceClaimCarriesSource { .. }),
        "expected ConcordanceClaimCarriesSource, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_rejects_oracle_in_evidence() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    claim: c
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: foo.bar
        epsilon: 0.1
        prior_value: 1.0
      paper_locator: src.md
      prior_binding:
        prior_unit: x
        prior_metric_definition: x
        locator: x
        prior_extraction_note: x
        source_id: x
    evidence:
      oracle: [BALL]
      command: "pytest"
      artifact: results.json
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceClaimCarriesOracle { .. }),
        "expected ConcordanceClaimCarriesOracle, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_rejects_tolerances() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    claim: c
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: x
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: foo.bar
        epsilon: 0.1
        prior_value: 1.0
      paper_locator: src.md
      prior_binding:
        prior_unit: x
        prior_metric_definition: x
        locator: x
        prior_extraction_note: x
        source_id: x
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceClaimCarriesTolerances { .. }),
        "expected ConcordanceClaimCarriesTolerances, got {err:?}",
    );
}

#[test]
fn measurement_claim_rejects_concordance_block() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: measurement
    tier: research
    source: .
    claim: c
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: x
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: foo.bar
        epsilon: 0.1
        prior_value: 1.0
      paper_locator: src.md
      prior_binding:
        prior_unit: x
        prior_metric_definition: x
        locator: x
        prior_extraction_note: x
        source_id: x
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::NonConcordanceClaimCarriesConcordance { .. }),
        "expected NonConcordanceClaimCarriesConcordance, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_missing_block_rejected() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    claim: c
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceClaimMissingBlock { .. }),
        "expected ConcordanceClaimMissingBlock, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_monotone_with_translates_with_null_prior_value() {
    let yaml = r#"
claims:
  - id: rustims-fdr-monotone
    title: FDR decreases monotonically with dataset complexity
    kind: behavioral_concordance
    tier: research
    claim: |
      FragPipe FDR on rustims-simulated data decreases monotonically
      as dataset complexity (peptide count) increases, matching the
      monotone trend Meier 2024 documents.
    concordance:
      pattern:
        pattern_kind: monotone_with
        metric_path: fragpipe.fdr_series
        parameter_path: fragpipe.dataset_complexity
        direction: decreasing
      paper_locator: source/cited.md#rustims-fdr-monotone
      prior_binding:
        prior_unit: percentage_points
        prior_metric_definition: |
          FragPipe empirical true FDR series across dataset
          complexity levels.
        locator: "Meier 2024 Fig 2"
        prior_extraction_note: "Curator confirmed direction"
        source_id: "doi:10.1038/PLACEHOLDER"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    let cd = attested.value.concordance.as_ref().expect("concordance present");
    match &cd.pattern {
        typed_trust::ConcordancePattern::MonotoneWith {
            metric_path,
            parameter_path,
            direction,
        } => {
            assert_eq!(metric_path, "fragpipe.fdr_series");
            assert_eq!(parameter_path, "fragpipe.dataset_complexity");
            assert!(matches!(direction, typed_trust::MonotoneDirection::Decreasing));
        }
        other => panic!("expected MonotoneWith, got {other:?}"),
    }
}

// ----------------------------------------------------------------------
// PR5i: third_party_observation translator
// ----------------------------------------------------------------------

#[test]
fn third_party_observation_numeric_band_translates_pr5i() {
    let yaml = r#"
claims:
  - id: rustims-maxquant-peak-matching-error-7p5min
    title: MaxQuant peak matching error reaches up to 30% on 7.5min 150k-peptide
    kind: third_party_observation
    tier: research
    claim: |
      On rustims-simulated 7.5min, 150,000-peptide dda-PASEF data,
      MaxQuant's peak matching error rate reached up to 30%.
    observation:
      third_party_tool: MaxQuant
      metric_definition: |
        Peak matching error rate per Cox 2008 §Methods.
      pattern:
        pattern_kind: numeric_band
        metric_path: maxquant.peak_matching_error.fraction_pct
        epsilon: 5.0
        observed_value: 30.0
      paper_locator: source/cited.md#rustims-maxquant-peak-matching
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    assert_eq!(attested.value.kind, ClaimKind::ThirdPartyObservation);
    let od = attested.value.observation.as_ref().expect("observation block");
    assert_eq!(od.third_party_tool, "MaxQuant");
    assert_eq!(od.paper_locator, "source/cited.md#rustims-maxquant-peak-matching");
    // The internal Rust ConcordancePattern::NumericBand has prior_value;
    // the translator mapped observed_value (YAML) → prior_value (internal).
    match &od.pattern {
        typed_trust::ConcordancePattern::NumericBand {
            metric_path,
            epsilon,
            prior_value,
        } => {
            assert_eq!(metric_path, "maxquant.peak_matching_error.fraction_pct");
            assert_eq!(*epsilon, 5.0);
            assert_eq!(*prior_value, 30.0);
        }
        other => panic!("expected NumericBand, got {other:?}"),
    }
}

#[test]
fn third_party_observation_missing_block_rejected_pr5i() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: third_party_observation
    tier: research
    claim: c
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ObservationClaimMissingBlock { .. }),
        "expected ObservationClaimMissingBlock, got {err:?}",
    );
}

#[test]
fn third_party_observation_rejects_source_field_pr5i() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: third_party_observation
    tier: research
    source: src.md
    claim: c
    observation:
      third_party_tool: X
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 1.0
        observed_value: 10.0
      paper_locator: src.md
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ObservationClaimCarriesSource { .. }),
        "expected ObservationClaimCarriesSource, got {err:?}",
    );
}

#[test]
fn third_party_observation_rejects_case_field_pr5i() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: third_party_observation
    tier: research
    case: case.md
    claim: c
    observation:
      third_party_tool: X
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 1.0
        observed_value: 10.0
      paper_locator: src.md
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ObservationClaimCarriesCase { .. }),
        "expected ObservationClaimCarriesCase, got {err:?}",
    );
}

#[test]
fn third_party_observation_rejects_last_verified_pr5i() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: third_party_observation
    tier: research
    claim: c
    last_verified:
      commit: abc
      date: "2026-01-01"
    observation:
      third_party_tool: X
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 1.0
        observed_value: 10.0
      paper_locator: src.md
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ObservationClaimCarriesLastVerified { .. }),
        "expected ObservationClaimCarriesLastVerified, got {err:?}",
    );
}

#[test]
fn third_party_observation_rejects_oracle_in_evidence_pr5i() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: third_party_observation
    tier: research
    claim: c
    observation:
      third_party_tool: X
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 1.0
        observed_value: 10.0
      paper_locator: src.md
    evidence:
      oracle: [BALL]
      command: pytest
      artifact: out.json
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ObservationClaimCarriesOracle { .. }),
        "expected ObservationClaimCarriesOracle, got {err:?}",
    );
}

#[test]
fn third_party_observation_evidence_without_oracle_accepted_pr5i() {
    // Codex v2 F-CR5: serde(default) on oracle should allow observation
    // manifests to omit the field entirely.
    let yaml = r#"
claims:
  - id: ok
    title: ok
    kind: third_party_observation
    tier: research
    claim: c
    observation:
      third_party_tool: X
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 1.0
        observed_value: 10.0
      paper_locator: src.md
    evidence:
      command: pytest
      artifact: out.json
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    // Should parse + translate without error.
    let _ = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap();
}

#[test]
fn measurement_claim_rejects_observation_block_pr5i() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: measurement
    tier: research
    source: .
    claim: c
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: x
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
    observation:
      third_party_tool: X
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 1.0
        observed_value: 10.0
      paper_locator: src.md
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::NonObservationClaimCarriesObservation { .. }),
        "expected NonObservationClaimCarriesObservation, got {err:?}",
    );
}

#[test]
fn third_party_observation_rejects_non_finite_observed_value_pr5i() {
    // Codex v2 F-CR-bug-4: NaN/Inf must be rejected.
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: third_party_observation
    tier: research
    claim: c
    observation:
      third_party_tool: X
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 1.0
        observed_value: .nan
      paper_locator: src.md
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ObservationNonFiniteValue { .. }),
        "expected ObservationNonFiniteValue, got {err:?}",
    );
}

#[test]
fn third_party_observation_rejects_empty_third_party_tool_pr5i() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: third_party_observation
    tier: research
    claim: c
    observation:
      third_party_tool: " "
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 1.0
        observed_value: 10.0
      paper_locator: src.md
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ObservationMissingThirdPartyTool { .. }),
        "expected ObservationMissingThirdPartyTool, got {err:?}",
    );
}

#[test]
fn third_party_observation_rejects_epsilon_zero_pr5i() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: third_party_observation
    tier: research
    claim: c
    observation:
      third_party_tool: X
      metric_definition: y
      pattern:
        pattern_kind: numeric_band
        metric_path: x.y
        epsilon: 0.0
        observed_value: 10.0
      paper_locator: src.md
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let err = translate_claim(&ctx("any.yaml"), &manifest.claims[0], "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ObservationNumericBandEpsilonInvalid { .. }),
        "expected ObservationNumericBandEpsilonInvalid, got {err:?}",
    );
}

// ----------------------------------------------------------------------
// PR5j: codex review fixes for PR5i
// ----------------------------------------------------------------------

#[test]
fn measurement_evidence_without_oracle_rejected_pr5j_fix1() {
    // PR5i made ManifestEvidence.oracle serde(default). For
    // measurement claims, the schema requires a non-empty oracle
    // list. PR5j re-imposes the check at translate-evidence time.
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: measurement
    tier: ci
    source: .
    claim: c
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: under 2 percent
    evidence:
      command: pytest
      artifact: out.json
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    // translate_claim succeeds (kind validation), but
    // translate_tolerances + translate_evidence run after.
    translate_claim(&ctx, mc, "claims[0]").unwrap();
    let criteria = translate_tolerances(mc).unwrap();
    let err = translate_evidence(&ctx, mc, &criteria).unwrap_err();
    assert!(
        matches!(err, TranslateError::MeasurementWithoutOracle { .. }),
        "expected MeasurementWithoutOracle, got {err:?}",
    );
}

#[test]
fn measurement_evidence_with_explicit_empty_oracle_rejected_pr5j_fix1() {
    // Same as above but with explicit `oracle: []` — must also
    // be rejected so existing manifests that ship the field but
    // leave it empty don't slip through.
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: measurement
    tier: ci
    source: .
    claim: c
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: under 2 percent
    evidence:
      oracle: []
      command: pytest
      artifact: out.json
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    translate_claim(&ctx, mc, "claims[0]").unwrap();
    let criteria = translate_tolerances(mc).unwrap();
    let err = translate_evidence(&ctx, mc, &criteria).unwrap_err();
    assert!(
        matches!(err, TranslateError::MeasurementWithoutOracle { .. }),
        "expected MeasurementWithoutOracle, got {err:?}",
    );
}
