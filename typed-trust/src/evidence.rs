//! Evidence + SupportRelation.
//!
//! An Evidence value carries two derivations:
//! - `extraction`: how the artifact was obtained (almost always Verified)
//! - `supports`: whether the artifact backs the claim (almost always Judged)

use crate::derivation::{Attested, Derivation, Locator};
use crate::ids::{ClaimId, EvidenceId};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Evidence {
    pub id: EvidenceId,
    pub for_claim: ClaimId,
    pub kind: EvidenceKind,
    pub locator: Locator,
    /// HOW the artifact itself was obtained.
    pub extraction: Derivation,
    /// WHETHER it backs the claim.
    pub supports: Attested<SupportRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Figure,
    Table,
    Statistic,
    Benchmark,
    Dataset,
    SourceCode,
    Citation,
    ProvenanceTrail,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SupportRelation {
    Supports { strength: Strength },
    Undermines { strength: Strength },
    Neutral,
    Insufficient,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    Weak,
    Moderate,
    Strong,
}
