//! TrustReport — see §7 and §8 of `concepts/typed-trust.md`.

use crate::derivation::Attested;
use crate::ids::{ClaimId, CriterionId, EventId};

#[derive(Debug, Clone)]
pub struct TrustReport {
    pub claim: ClaimId,
    pub criteria: Vec<Criterion>,
    /// Rendered references to ReviewEvents whose target points into
    /// this report or its claim. Graph is source of truth; this is
    /// convenience for renderers (don't make the reader traverse).
    pub challenges: Vec<EventId>,
    pub gaps: Vec<Gap>,
    /// Default None per invariant 3 (no bare aggregate confidence).
    pub aggregate: Option<Attested<Aggregate>>,
    /// Synthesized at build time. See §8 — render annotation, NOT a
    /// typed field on every `Attested<T>`.
    pub status: RenderStatus,
}

/// Closed enum so consumers (CI gates, doc generators) can branch
/// without parsing strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderStatus {
    Current,
    Superseded,
    Contested,
}

#[derive(Debug, Clone)]
pub struct Criterion {
    /// Stable across re-synthesis. MetricObservation binds here.
    pub id: CriterionId,
    pub name: String,
    pub tolerance: Option<Tolerance>,
    pub result: Attested<CriterionResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tolerance {
    /// Free string + project vocab (the shipping manifest declares
    /// `tolerance_metric` as the source of truth).
    pub metric: String,
    pub op: ComparisonOp,
    pub value: f64,
    /// Names the output when a claim has many.
    pub output: Option<String>,
    /// Names the oracle (from manifest vocab) when a single output is
    /// checked against multiple oracles with different tolerances.
    pub against: Option<String>,
    /// Required human gloss.
    pub prose: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonOp {
    Lt,
    LtEq,
    GtEq,
    Gt,
    /// Safe for integer/discrete metrics and `absolute_error == 0`
    /// exact-equality assertions (DSSP residue_count parity).
    /// Validator MUST warn when paired with `metric: relative_error`
    /// on a float output — that combination is the float-equality
    /// trap.
    Eq,
}

/// An observed value, structurally bound to its Criterion. Name-match
/// would let `relative_error` vs `RelativeError` drift through;
/// CriterionId bind is the falsifiability anchor.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricObservation {
    pub criterion: CriterionId,
    pub value: f64,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriterionResult {
    Pass,
    Fail,
    Partial { detail: String },
    NotApplicable,
    /// Distinct from Fail. Fail means "checked and it did not hold";
    /// NotAssessed means "no check available." See invariant 4.
    NotAssessed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub description: String,
    pub would_satisfy: Vec<String>,
    pub author_actionable: bool,
}

/// Optional aggregate value. Per invariant 3, computed by a named
/// pure function declared in the project's spec, not emitted by a
/// model.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregate {
    pub name: String,
    pub value: f64,
}
