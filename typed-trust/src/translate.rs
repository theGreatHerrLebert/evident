//! Manifest → Typed Trust translator.
//!
//! Implements the §11 seam from `concepts/typed-trust.md`: a
//! deterministic projection from the shipping `evident.yaml` schema's
//! measurement-class claims into Typed Trust constructors.
//!
//! Scope (MVP):
//! - Parses the top-level manifest YAML and per-claim measurement
//!   fields into [`ManifestFile`] / [`ManifestClaim`].
//! - Translates one [`ManifestClaim`] into an [`Attested<Claim>`] with
//!   a Verified extraction (per §4 footnote: structured manifest input
//!   yields a Verified, not Judged, extraction).
//! - Translates the per-claim `tolerances` block into [`Tolerance`]
//!   values, populating `against` from a single-oracle heuristic
//!   (multi-oracle disambiguation needs schema work or convention).
//! - Rejects `kind: policy | reference` as [`TranslateError::OutOfScope`]
//!   per §0.
//!
//! Out of scope for this MVP (follow-up work):
//! - Translating `inputs` / `outputs` / `pinned_versions` into
//!   `ProvenanceRecord` + `ToolInvocation.env`.
//! - Translating `evidence.command / artifact` into [`Evidence`].
//! - Translating `last_verified` into a [`Rerun`].
//! - Translating `provenance: peer-reviewed` reviewers into
//!   [`ReviewEvent`]s.
//! - Translating assumptions into `Attested<Assumption>` (currently
//!   dropped on the floor).

use serde::Deserialize;

use crate::claim::{Claim, ClaimKind, SourceSpan};
use crate::derivation::{Attested, Derivation, ToolInvocation};
use crate::identity::{Identity, IdentityKind};
use crate::ids::{ClaimId, Timestamp};
use crate::report::{ComparisonOp, Tolerance};

// ---------- Manifest shape ----------

/// The top-level shape of an `evident.yaml` or included claim file.
/// Only `claims` is consumed by the MVP translator; `version`,
/// `project`, `vocabularies`, `include` are not yet used.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestFile {
    pub claims: Vec<ManifestClaim>,
}

/// Subset of the shipping `claim` schema sufficient for the MVP
/// translator. Many manifest fields (subsystem, trust_strategy,
/// capabilities, inputs, outputs, pinned_versions, last_verified)
/// are NOT consumed yet — see module-level scope.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestClaim {
    pub id: String,
    pub title: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub case: Option<String>,
    pub source: Option<String>,
    pub tier: String,
    pub claim: String,
    pub tolerances: Option<Vec<ManifestTolerance>>,
    pub evidence: Option<ManifestEvidence>,
    pub provenance: Option<String>,
    pub assumptions: Option<Vec<String>>,
    pub failure_modes: Option<Vec<String>>,
}

fn default_kind() -> String {
    "measurement".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestTolerance {
    pub metric: String,
    pub op: String,
    pub value: f64,
    pub output: Option<String>,
    pub prose: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEvidence {
    pub oracle: Vec<String>,
    pub command: String,
    pub artifact: String,
}

// ---------- Translation context, errors ----------

#[derive(Debug, Clone)]
pub struct TranslationContext {
    /// Time at which the translation is performed; goes into
    /// `Attested.at`.
    pub now: Timestamp,
    /// Source manifest path; goes into `Claim.source.path`.
    pub manifest_path: String,
}

#[derive(Debug)]
pub enum TranslateError {
    Yaml(String),
    /// Policy/reference claims are out of typed-trust scope (§0).
    OutOfScope { id: String, kind: String },
    /// An unknown comparison operator in `tolerances[].op`.
    UnknownOp { id: String, op: String },
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranslateError::Yaml(e) => write!(f, "YAML parse error: {e}"),
            TranslateError::OutOfScope { id, kind } => write!(
                f,
                "claim {id} has kind={kind}, out of typed-trust scope (§0)"
            ),
            TranslateError::UnknownOp { id, op } => {
                write!(f, "claim {id}: unknown comparison op {op:?}")
            }
        }
    }
}

impl std::error::Error for TranslateError {}

// ---------- Translation ----------

/// Parse a YAML manifest into its structured form.
pub fn parse_manifest_file(yaml: &str) -> Result<ManifestFile, TranslateError> {
    serde_yaml_ng::from_str(yaml).map_err(|e| TranslateError::Yaml(e.to_string()))
}

/// Translate a single manifest claim into an [`Attested<Claim>`]. The
/// extraction is `Derivation::Verified` because the projection is
/// deterministic (per §4 footnote).
///
/// `span` is the YAML location of this claim within its file (e.g.
/// `"claims[0]"`); goes into [`SourceSpan`].
pub fn translate_claim(
    ctx: &TranslationContext,
    mc: &ManifestClaim,
    span: &str,
) -> Result<Attested<Claim>, TranslateError> {
    // §0 scope: only propositional empirical (measurement) claims.
    if mc.kind != "measurement" {
        return Err(TranslateError::OutOfScope {
            id: mc.id.clone(),
            kind: mc.kind.clone(),
        });
    }

    let claim = Claim {
        id: ClaimId::new(&mc.id),
        text: mc.claim.trim().to_string(),
        kind: infer_kind(mc),
        source: SourceSpan {
            path: ctx.manifest_path.clone(),
            span: span.into(),
        },
        explicit: true,
        decomposes_into: vec![],
        // TODO: translate `assumptions` into Vec<Attested<Assumption>>.
        // Each assumption becomes a Judged attestation by the manifest
        // author. Requires either reviewer identity or the degraded
        // `unspecified_human_from_manifest` form.
        requires_assumptions: vec![],
    };

    let derivation = Derivation::Verified {
        method: ToolInvocation {
            command: format!("typed-trust translate {}", ctx.manifest_path),
            tool_version: env!("CARGO_PKG_VERSION").into(),
            env: vec![],
        },
        ran_by: translator_identity(),
        reruns: vec![],
    };

    Ok(Attested {
        value: claim,
        derivation,
        at: ctx.now.clone(),
    })
}

/// Translate all `tolerances` entries into [`Tolerance`] values. When
/// the claim's `evidence.oracle` is a single entry, populate
/// `Tolerance.against` from it (the F-PR3 single-oracle case);
/// otherwise leave `against = None` and let the manifest author or a
/// later schema extension disambiguate.
pub fn translate_tolerances(mc: &ManifestClaim) -> Result<Vec<Tolerance>, TranslateError> {
    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
        if e.oracle.len() == 1 {
            Some(e.oracle[0].clone())
        } else {
            None
        }
    });

    mc.tolerances
        .as_ref()
        .map(|ts| {
            ts.iter()
                .map(|t| translate_tolerance(t, &single_oracle, &mc.id))
                .collect()
        })
        .unwrap_or_else(|| Ok(vec![]))
}

fn translate_tolerance(
    mt: &ManifestTolerance,
    single_oracle: &Option<String>,
    claim_id: &str,
) -> Result<Tolerance, TranslateError> {
    Ok(Tolerance {
        metric: mt.metric.clone(),
        op: parse_op(&mt.op, claim_id)?,
        value: mt.value,
        output: mt.output.clone(),
        against: single_oracle.clone(),
        prose: mt.prose.trim().to_string(),
    })
}

fn parse_op(op: &str, claim_id: &str) -> Result<ComparisonOp, TranslateError> {
    match op {
        "<" => Ok(ComparisonOp::Lt),
        "<=" => Ok(ComparisonOp::LtEq),
        ">=" => Ok(ComparisonOp::GtEq),
        ">" => Ok(ComparisonOp::Gt),
        "==" => Ok(ComparisonOp::Eq),
        other => Err(TranslateError::UnknownOp {
            id: claim_id.into(),
            op: other.into(),
        }),
    }
}

/// Heuristic for ClaimKind when the manifest doesn't carry an explicit
/// propositional kind. Codex pass 2 flagged this as non-deterministic;
/// for the MVP we use the simplest rule that gets the common case
/// right.
///
/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
///   (most measurement claims compare against an oracle).
/// - Otherwise → `Other("Measurement")` (lossless flag for the
///   manifest author to refine).
fn infer_kind(mc: &ManifestClaim) -> ClaimKind {
    if mc
        .evidence
        .as_ref()
        .is_some_and(|e| !e.oracle.is_empty())
    {
        ClaimKind::Comparison
    } else {
        ClaimKind::Other("Measurement".into())
    }
}

fn translator_identity() -> Identity {
    Identity {
        kind: IdentityKind::Automated,
        name: "typed-trust-translator".into(),
        details: vec![],
    }
}
