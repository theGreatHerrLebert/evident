//! ReviewEvent — see §6 of `concepts/typed-trust.md`.
//!
//! Actions over existing objects in the graph. Not Claims; not
//! pipeline input. Recorded by reviewers (output of adversarial
//! review), consumed by synthesis when computing render status.

use crate::ids::{
    AttestedId, ClaimId, CriterionId, EventId, EvidenceId, ProvenanceId, ReportId, Timestamp,
};
use crate::identity::Identity;

#[derive(Debug, Clone)]
pub struct ReviewEvent {
    pub id: EventId,
    pub target: Target,
    pub by: Identity,
    /// Required at release tier for Endorse/Dissent/Challenge events
    /// (invariant 10). Validator-enforced.
    pub protocol: Option<String>,
    pub rationale: String,
    pub at: Timestamp,
    pub kind: ReviewKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewKind {
    Endorse,
    Dissent,
    /// Re-judgment that overrides the prior attestation. The successor
    /// is constructed independently; this event links them.
    Supersede { successor: AttestedId },
    /// Substantive objection. Unbacked = informational flag; only a
    /// backed Challenge can move render status to Contested
    /// (invariant 6), unless `category` is one of the closed
    /// procedural variants.
    Challenge {
        category: ChallengeCategory,
        backed_by: Option<ClaimId>,
    },
}

/// What a ReviewEvent targets. Note the F-PR14 split:
/// `Criterion(CriterionId)` is for challenging the tolerance /
/// definition (stable across re-synthesis); `CriterionResult` is for
/// challenging a specific synthesized result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Claim(ClaimId),
    ClaimAttestation(AttestedId),
    Evidence(EvidenceId),
    SupportRelation(EvidenceId),
    Provenance(ProvenanceId),
    TrustReport(ReportId),
    /// Challenge against the tolerance / definition. Stable.
    Criterion(CriterionId),
    /// Challenge against a specific result in a specific report.
    CriterionResult {
        report: ReportId,
        criterion: CriterionId,
    },
    ReviewEvent(EventId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeCategory {
    // Substantive — REQUIRE backing Claim to move status.
    MissingControl,
    WeakStatistics,
    Confound,
    UnverifiableAssumption,
    MissingBenchmark,
    ReproducibilityRisk,

    // Procedural — closed list, MAY move status without backing.
    ArtifactUnavailable,
    HashMismatch,
    CommandFailure,
    ConflictOfInterest,
    PeerReviewUnverifiable,

    /// Open project vocab. Always requires a backing Claim
    /// (`Other(_)` is not in the procedural closed list).
    Other(String),
}
