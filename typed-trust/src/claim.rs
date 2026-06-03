//! Claim — see §5 of `concepts/typed-trust.md`.
//!
//! Propositional content only. Review actions (Endorse, Dissent,
//! Challenge, Supersede) live in `ReviewEvent`, not as Claim variants.

use crate::derivation::Attested;
use crate::ids::ClaimId;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Claim {
    pub id: ClaimId,
    pub text: String,
    pub kind: ClaimKind,
    pub source: SourceSpan,
    /// Stated verbatim in the source vs. inferred.
    pub explicit: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub decomposes_into: Vec<ClaimId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires_assumptions: Vec<Attested<Assumption>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ClaimKind {
    Performance,
    Comparison,
    Causal,
    Existence,
    Reproducibility,
    Provenance,
    /// PR5b: declarative claim about a configuration field
    /// (e.g. ``requires-python = ">=3.10"`` in pyproject.toml).
    /// Not an empirical measurement — the declaration IS the
    /// evidence. Synthesizer emits a metadata-flavored
    /// TrustReport without empirical Criteria.
    MetadataCompatibility,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Assumption {
    pub text: String,
    pub load_bearing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceSpan {
    pub path: String,
    pub span: String,
}
