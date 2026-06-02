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
    /// Phase 5: replay-path state. Distinguishes "not run yet" from
    /// "cannot be run from artifacts we can see." Default
    /// `NotAttempted` preserves the meaning of hand-authored manifests
    /// pre-Phase-5.
    #[serde(default)]
    pub replay_status: ReplayStatus,
    /// Phase 5: structured reason a replay is unavailable. Only
    /// meaningful when `replay_status == UnavailableArtifacts`; the
    /// translate-time pair-validator enforces this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_reason: Option<ReplayReason>,
}

/// Phase 5: the state of a claim's replay path. Three values map to
/// three distinct queries the corpus needs to support:
///
/// - `Available` — Phase 1 replay can run this claim's
///   `evidence.command`. Sidecar `last_verified` gets populated by
///   `evident-agent replay`.
/// - `NotAttempted` — nobody has tried. Default for hand-authored
///   manifests; matches today's behaviour. `replay_reason` MUST be
///   `None`.
/// - `UnavailableArtifacts` — the extractor (or a curator) verified
///   that replay cannot succeed from what is available. A
///   structured `replay_reason` is REQUIRED to disambiguate the
///   blocker.
///
/// The pair-validator in `translate::translate_evidence` rejects
/// any other combination of `(replay_status, replay_reason)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReplayStatus {
    Available,
    #[default]
    NotAttempted,
    UnavailableArtifacts,
}

/// Phase 5: structured reason a replay is unavailable. The variants
/// cover the realistic blockers extracted-from-paper / extracted-from-repo
/// claims actually hit; each one is queryable so a curator can find
/// claims gated only on (e.g.) missing data, or only on license waivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayReason {
    /// Source code not released or behind access control.
    CodePrivate,
    /// Dataset / benchmark inputs not released or withdrawn.
    DataUnavailable,
    /// Artifact exists but cannot be legally redistributed or used.
    LicenseRestricted,
    /// Replay needs hardware the framework cannot provision.
    ComputeUnavailable,
    /// Original toolchain / runtime / OS / container cannot be rebuilt.
    EnvironmentUnavailable,
    /// A referenced package, model checkpoint, or container image is gone.
    DependencyUnavailable,
    /// Replay depends on a remote API or service no longer reachable.
    ExternalServiceUnavailable,
    /// Benchmark identity is ambiguous (no version, no checksum).
    BenchmarkUnspecified,
    /// Code/data available but the source does not say how to reproduce.
    InstructionsMissing,
    /// Replay depends on human raters / subjective evaluation.
    RequiresHumanEvaluation,
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
