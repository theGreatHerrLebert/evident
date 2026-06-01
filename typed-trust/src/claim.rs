//! Claim — see §5 of `concepts/typed-trust.md`.
//!
//! Propositional content only. Review actions (Endorse, Dissent,
//! Challenge, Supersede) live in `ReviewEvent`, not as Claim variants.

use crate::derivation::Attested;
use crate::ids::ClaimId;

#[derive(Debug, Clone)]
pub struct Claim {
    pub id: ClaimId,
    pub text: String,
    pub kind: ClaimKind,
    pub source: SourceSpan,
    /// Stated verbatim in the source vs. inferred.
    pub explicit: bool,
    pub decomposes_into: Vec<ClaimId>,
    pub requires_assumptions: Vec<Attested<Assumption>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimKind {
    Performance,
    Comparison,
    Causal,
    Existence,
    Reproducibility,
    Provenance,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assumption {
    pub text: String,
    pub load_bearing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub path: String,
    pub span: String,
}
