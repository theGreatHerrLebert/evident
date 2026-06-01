//! Derivation — see §2 of `concepts/typed-trust.md`. The spine.
//!
//! Every `Attested<T>` records HOW the value was established. The
//! reproducibility distinction (Verified vs Judged) does NOT depend
//! on who established it; same machinery for model and human authors.

use crate::ids::{Hash, Timestamp};
use crate::identity::Identity;
use crate::report::MetricObservation;

/// A value paired with its derivation and the time it was established.
/// The audit surface — same shape regardless of T.
#[derive(Debug, Clone)]
pub struct Attested<T> {
    pub value: T,
    pub derivation: Derivation,
    pub at: Timestamp,
}

/// How a given assertion was established.
#[derive(Debug, Clone)]
pub enum Derivation {
    /// Established by a procedure a third party can re-run to the
    /// same result. Anyone with the method can re-verify;
    /// `ran_by` is forensic provenance, not load-bearing for trust.
    Verified {
        method: ToolInvocation,
        ran_by: Identity,
        reruns: Vec<Rerun>,
    },
    /// Established by interpretation. NEVER rendered as a fact.
    /// `by` is load-bearing — re-asking a different judge produces a
    /// different judgment, not the same one.
    /// Validator: `by.kind == IdentityKind::Automated` is invalid
    /// (invariant 9).
    Judged {
        by: Identity,
        protocol: Option<String>,
        rationale: String,
        confidence: Confidence,
    },
    /// Searched for and not found. Absence is a result, not a hole.
    Absent {
        sought: String,
        searched: Vec<Locator>,
        searched_by: Identity,
    },
}

/// A single rerun of the Verified method. Chronological in the
/// containing `Vec<Rerun>`; the latest is the most recent
/// verification. Empty Vec = no reruns attempted.
#[derive(Debug, Clone)]
pub struct Rerun {
    pub at: Timestamp,
    pub by: Identity,
    pub observed: Vec<MetricObservation>,
    pub corpus_sha: Option<Hash>,
    pub outcome: ReproductionOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReproductionOutcome {
    Matched,
    Diverged { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confidence {
    Low,
    Moderate,
    High,
}

/// A specific tool invocation, with enough pinning that a third party
/// can re-run it.
#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub command: String,
    pub tool_version: String,
    /// Key/value env or version pinning (e.g. `("Biopython", "1.87")`,
    /// `("python", "3.12")`). Free-form keys — projects extend.
    pub env: Vec<(String, String)>,
}

/// Where an artifact lives. Open variants — adopters add their own
/// via `Other`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    Artifact(String),
    ArtifactInRelease {
        archive: String,
        path: String,
        sha256: Hash,
    },
    Repo {
        repo: String,
        commit: String,
    },
    Url(String),
    Doi(String),
    Other(String),
}
