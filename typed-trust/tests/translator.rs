//! Manifest → Typed Trust translator tests.
//!
//! Inputs are abridged but otherwise verbatim from real proteon claim
//! files. The full files are at:
//!   /scratch/TMAlign/proteon/evident/claims/sasa.yaml
//!   /scratch/TMAlign/proteon/evident/claims/release_gate.yaml
//!   /scratch/TMAlign/proteon/evident/claims/dssp.yaml

use typed_trust::translate::{
    parse_manifest_file, translate_claim, translate_tolerances,
    TranslateError, TranslationContext,
};
use typed_trust::*;

/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
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
    assumptions:
      - Biopython's Bio.PDB.SASA.ShrakeRupley uses the same probe radius.
    failure_modes:
      - Single-oracle agreement can mask a shared convention choice.
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
    let tolerances = translate_tolerances(&manifest.claims[0]).unwrap();

    assert_eq!(tolerances.len(), 1);
    let t = &tolerances[0];
    assert_eq!(t.metric, "relative_error");
    assert_eq!(t.op, ComparisonOp::Lt);
    assert_eq!(t.value, 0.02);
    assert_eq!(t.output.as_deref(), Some("total_sasa"));
    // F-PR3 single-oracle case: `against` is populated from the single
    // entry in `evidence.oracle`.
    assert_eq!(t.against.as_deref(), Some("Biopython"));
    assert!(t.prose.contains("biopython_total"));
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
    let tolerances = translate_tolerances(&manifest.claims[0]).unwrap();

    assert_eq!(tolerances.len(), 3);

    // First tolerance: absolute_error == 0 on residue_count.
    assert_eq!(tolerances[0].op, ComparisonOp::Eq);
    assert_eq!(tolerances[0].value, 0.0);
    assert_eq!(tolerances[0].output.as_deref(), Some("residue_count"));
    assert_eq!(tolerances[0].metric, "absolute_error");

    // Second: pass_rate >= 0.95.
    assert_eq!(tolerances[1].op, ComparisonOp::GtEq);
    assert_eq!(tolerances[1].metric, "pass_rate");

    // Third: absolute_error < 0.10.
    assert_eq!(tolerances[2].op, ComparisonOp::Lt);
    assert_eq!(tolerances[2].output.as_deref(), Some("helix_fraction"));

    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
    for t in &tolerances {
        assert_eq!(t.against.as_deref(), Some("pydssp"));
    }
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
fn dssp_claim_translates_with_pydssp_as_inferred_comparison() {
    let manifest = parse_manifest_file(PROTEON_DSSP_YAML).unwrap();
    let ctx = ctx("proteon/evident/claims/dssp.yaml");
    let attested = translate_claim(&ctx, &manifest.claims[0], "claims[0]").unwrap();

    assert_eq!(attested.value.id.as_str(), "proteon-dssp-vs-pydssp-ci");
    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
}
