//! TrustReport — see §7 and §8 of `concepts/typed-trust.md`.

use crate::derivation::Attested;
use crate::ids::{ClaimId, CriterionId, EventId};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TrustReport {
    pub claim: ClaimId,
    /// Synthesized at build time. See §8 — render annotation, NOT a
    /// typed field on every `Attested<T>`.
    pub status: RenderStatus,
    pub criteria: Vec<Criterion>,
    /// Rendered references to ReviewEvents whose target points into
    /// this report or its claim. Graph is source of truth; this is
    /// convenience for renderers (don't make the reader traverse).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub challenges: Vec<EventId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<Gap>,
    /// Default None per invariant 3 (no bare aggregate confidence).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<Attested<Aggregate>>,
}

/// Closed enum so consumers (CI gates, doc generators) can branch
/// without parsing strings.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderStatus {
    Current,
    Superseded,
    Contested,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Criterion {
    /// Stable across re-synthesis. MetricObservation binds here.
    pub id: CriterionId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<Tolerance>,
    pub result: Attested<CriterionResult>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Tolerance {
    /// Free string + project vocab.
    pub metric: String,
    pub op: ComparisonOp,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub against: Option<String>,
    /// Required human gloss.
    pub prose: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ComparisonOp {
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    LtEq,
    #[serde(rename = ">=")]
    GtEq,
    #[serde(rename = ">")]
    Gt,
    /// Safe for integer/discrete metrics and `absolute_error == 0`
    /// exact-equality assertions (DSSP residue_count parity).
    /// Validator MUST warn when paired with `metric: relative_error`
    /// on a float output.
    #[serde(rename = "==")]
    Eq,
}

/// An observed value, structurally bound to its Criterion.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MetricObservation {
    pub criterion: CriterionId,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum CriterionResult {
    Pass,
    Fail,
    Partial { detail: String },
    NotApplicable,
    /// Distinct from Fail. Fail means "checked and it did not hold";
    /// NotAssessed means "no check available." See invariant 4.
    NotAssessed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Gap {
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub would_satisfy: Vec<String>,
    pub author_actionable: bool,
}

/// Optional aggregate value. Per invariant 3, computed by a named
/// pure function declared in the project's spec, not emitted by a
/// model.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Aggregate {
    pub name: String,
    pub value: f64,
}
