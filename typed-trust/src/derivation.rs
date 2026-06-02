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
#[derive(Debug, Clone, serde::Serialize)]
pub struct Attested<T: serde::Serialize> {
    pub value: T,
    pub derivation: Derivation,
    pub at: Timestamp,
}

/// How a given assertion was established.
///
/// Serializes with adjacent tagging: `{"type": "verified", "data": {...}}`,
/// `{"type": "judged", "data": {...}}`, `{"type": "absent", "data": {...}}`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Derivation {
    /// Established by a procedure a third party can re-run to the
    /// same result.
    Verified {
        method: ToolInvocation,
        ran_by: Identity,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        reruns: Vec<Rerun>,
    },
    /// Established by interpretation. NEVER rendered as a fact.
    /// Validator: `by.kind == IdentityKind::Automated` is invalid
    /// (invariant 9).
    Judged {
        by: Identity,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol: Option<String>,
        rationale: String,
        confidence: Confidence,
    },
    /// Searched for and not found.
    Absent {
        sought: String,
        searched: Vec<Locator>,
        searched_by: Identity,
    },
}

/// A single rerun of the Verified method.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Rerun {
    pub at: Timestamp,
    pub by: Identity,
    pub observed: Vec<MetricObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corpus_sha: Option<Hash>,
    pub outcome: ReproductionOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ReproductionOutcome {
    Matched,
    Diverged { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Moderate,
    High,
}

/// A specific tool invocation, with enough pinning that a third party
/// can re-run it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolInvocation {
    pub command: String,
    pub tool_version: String,
    /// Key/value env or version pinning. Serializes as `[["k", "v"]]`
    /// — array of pairs — to preserve order and allow duplicate keys.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<(String, String)>,
}

/// Where an artifact lives. Open variants — adopters add their own
/// via `Other`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
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
