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
use crate::derivation::{
    Attested, Derivation, Locator, Rerun, ReproductionOutcome, ToolInvocation,
};
use crate::evidence::{Evidence, EvidenceKind, Strength, SupportRelation};
use crate::identity::{Identity, IdentityDetail, IdentityKind};
use crate::ids::{ClaimId, CriterionId, EventId, EvidenceId, Timestamp};
use crate::report::{ComparisonOp, MetricObservation, Tolerance};
use crate::derivation::Confidence;

// ---------- Manifest shape ----------

/// The top-level shape of an `evident.yaml` or included claim file.
/// `claims` is optional: a top-level manifest may carry only an
/// `include:` list and no claims of its own (proteon's `evident.yaml`
/// follows this pattern). `version`, `project`, `vocabularies`,
/// `include` are not parsed at this layer — the CLI handles include
/// resolution.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestFile {
    #[serde(default)]
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
    pub last_verified: Option<ManifestLastVerified>,
    pub assumptions: Option<Vec<String>>,
    pub failure_modes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestLastVerified {
    pub commit: Option<String>,
    pub date: Option<String>,
    pub value: Option<f64>,
    pub corpus_sha: Option<String>,
}

fn default_kind() -> String {
    "measurement".into()
}

/// Mirrors the shipping schema's tolerance entry. Per `workflow/SCHEMA.md`:
/// `metric`, `op`, `value` are all-or-nothing — either supply all three
/// for a structured tolerance, or supply none and use only `prose` (the
/// research-tier deferred-spec case). `prose` is always required.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestTolerance {
    pub metric: Option<String>,
    pub op: Option<String>,
    pub value: Option<f64>,
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
    /// A tolerance entry has some but not all of `metric`/`op`/`value`.
    /// The shipping schema requires all three together (structured) or
    /// none (prose-only); mixing them is a manifest error.
    PartialTolerance { id: String },
    /// A prose-only tolerance (metric/op/value all absent) appeared at
    /// a non-research tier. The shipping schema frames prose-only as
    /// the research-tier deferred-spec escape hatch only — CI and
    /// release claims must carry structured tolerances.
    ProseOnlyOutsideResearch { id: String, tier: String },
    /// A `kind: measurement` claim omitted `tolerances` or provided
    /// an empty list. The shipping schema requires non-empty
    /// tolerances on measurement claims; without them the
    /// synthesizer would emit a Current report with nothing
    /// assessed.
    MeasurementWithoutTolerances { id: String },
    /// A `kind: measurement` claim omitted the `evidence` block. The
    /// shipping schema requires evidence on measurement claims;
    /// without it the report would render Current with NotAssessed
    /// criteria — an unevidenced measurement looking accepted.
    MeasurementWithoutEvidence { id: String },
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
            TranslateError::PartialTolerance { id } => write!(
                f,
                "claim {id}: tolerance has some but not all of metric/op/value; \
                 shipping schema requires all-or-nothing"
            ),
            TranslateError::ProseOnlyOutsideResearch { id, tier } => write!(
                f,
                "claim {id}: prose-only tolerance not allowed at tier {tier:?}; \
                 prose-only is the research-tier deferred-spec escape hatch only"
            ),
            TranslateError::MeasurementWithoutTolerances { id } => write!(
                f,
                "claim {id}: kind=measurement requires non-empty tolerances; \
                 add tolerances or change to kind: policy / reference"
            ),
            TranslateError::MeasurementWithoutEvidence { id } => write!(
                f,
                "claim {id}: kind=measurement requires an evidence block; \
                 add evidence or change to kind: policy / reference"
            ),
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

/// A criterion id paired with its tolerance, ready to be lifted into a
/// [`Criterion`] once synthesis decides a result. The id is generated
/// at translate time so [`MetricObservation`] in a [`Rerun`] can bind
/// to it deterministically.
///
/// `tolerance` is `None` when the manifest tolerance is prose-only
/// (research-tier deferred-spec — `metric`/`op`/`value` all absent,
/// only `prose` carried). Synthesis treats such criteria as
/// `NotAssessed { reason: "no structured tolerance ..." }`. The
/// `prose` text is preserved on the Criterion via its name.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedCriterion {
    pub id: CriterionId,
    pub tolerance: Option<Tolerance>,
    /// Always present — `prose` is required by the shipping schema
    /// even when the structured triple is absent.
    pub prose: String,
}

/// Translate all `tolerances` entries into [`TranslatedCriterion`]
/// values. CriterionId is generated as `"{claim_id}-criterion-{idx}"`
/// — stable, locally unique, globally unique because claim ids are.
///
/// When the claim's `evidence.oracle` is a single entry, populate
/// `Tolerance.against` from it (the F-PR3 single-oracle case);
/// otherwise leave `against = None`.
pub fn translate_tolerances(
    mc: &ManifestClaim,
) -> Result<Vec<TranslatedCriterion>, TranslateError> {
    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
        if e.oracle.len() == 1 {
            Some(e.oracle[0].clone())
        } else {
            None
        }
    });

    let Some(ts) = mc.tolerances.as_ref() else {
        // Measurement claims require non-empty tolerances per
        // workflow/SCHEMA.md; without them the report would be Current
        // with nothing to assess.
        if mc.kind == "measurement" {
            return Err(TranslateError::MeasurementWithoutTolerances {
                id: mc.id.clone(),
            });
        }
        return Ok(vec![]);
    };
    if ts.is_empty() && mc.kind == "measurement" {
        return Err(TranslateError::MeasurementWithoutTolerances {
            id: mc.id.clone(),
        });
    }

    ts.iter()
        .enumerate()
        .map(|(idx, t)| {
            let id = CriterionId::new(format!("{}-criterion-{}", mc.id, idx));
            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
            Ok(TranslatedCriterion {
                id,
                tolerance,
                prose: t.prose.trim().to_string(),
            })
        })
        .collect()
}

fn translate_tolerance(
    mt: &ManifestTolerance,
    single_oracle: &Option<String>,
    claim_id: &str,
    tier: &str,
) -> Result<Option<Tolerance>, TranslateError> {
    // Per workflow/SCHEMA.md, metric/op/value are all-or-nothing.
    let triple = (mt.metric.as_ref(), mt.op.as_ref(), mt.value);
    match triple {
        // Prose-only — valid only at research tier as the deferred-spec
        // escape hatch. CI and release claims must carry structured
        // tolerances; allowing them to translate would let
        // under-specified claims pass through as Current.
        (None, None, None) => {
            if tier == "research" {
                Ok(None)
            } else {
                Err(TranslateError::ProseOnlyOutsideResearch {
                    id: claim_id.into(),
                    tier: tier.into(),
                })
            }
        }
        (Some(metric), Some(op), Some(value)) => Ok(Some(Tolerance {
            metric: metric.clone(),
            op: parse_op(op, claim_id)?,
            value,
            output: mt.output.clone(),
            against: single_oracle.clone(),
            prose: mt.prose.trim().to_string(),
        })),
        _ => Err(TranslateError::PartialTolerance {
            id: claim_id.into(),
        }),
    }
}

/// Translate the per-claim `evidence` block into an [`Evidence`].
/// Returns `None` if the manifest carries no `evidence` block.
///
/// Notes / MVP choices:
/// - One Evidence per claim (the YAML's `evidence.oracle` list is
///   collapsed into a single Evidence; oracle identity per tolerance
///   lives in `Tolerance.against`). The fit-test split this into one
///   Evidence per oracle; the translator's 1:1 mapping is simpler and
///   the design accepts both shapes.
/// - `Evidence.kind` defaults to `Benchmark`. The shipping schema has
///   no `evidence.kind` field; refinement is a follow-up.
/// - `Evidence.locator` wraps the manifest's `evidence.artifact`
///   string as-is. The shipping convention "path (archived in release
///   asset)" stays in the locator string; renderers can pretty-print.
/// - `Verified.ran_by` is an `Automated` `Identity` with degraded
///   "unspecified-runner" name, since the manifest doesn't record who
///   ran the command.
/// - `supports.by` uses [`Identity::unspecified_human_from_manifest`]
///   when provenance is "human" (F-PR4 degraded form), and a similar
///   degraded form keyed by provenance flag otherwise. Invariant 9
///   forbids Automated judges, so even `provenance: automatic`
///   produces a Human identity flagged via details.
/// - `last_verified` populates one [`Rerun`] in the Verified extraction
///   when fully populated; primary observed value binds to the FIRST
///   criterion id (shipping convention: `last_verified.value` is the
///   primary scalar metric).
pub fn translate_evidence(
    ctx: &TranslationContext,
    mc: &ManifestClaim,
    criteria: &[TranslatedCriterion],
) -> Result<Option<Evidence>, TranslateError> {
    let Some(me) = mc.evidence.as_ref() else {
        // Per workflow/SCHEMA.md, evidence is required for measurement
        // claims. Without it the report would render Current with
        // NotAssessed criteria — making an unevidenced measurement
        // claim look accepted.
        if mc.kind == "measurement" {
            return Err(TranslateError::MeasurementWithoutEvidence {
                id: mc.id.clone(),
            });
        }
        return Ok(None);
    };
    let runner = unspecified_runner_identity(mc.provenance.as_deref());
    let first_criterion = criteria.first().map(|c| c.id.clone());
    let reruns = translate_last_verified(
        mc.last_verified.as_ref(),
        first_criterion.as_ref(),
        &runner,
    );

    Ok(Some(Evidence {
        id: EvidenceId::new(format!("ev-{}", mc.id)),
        for_claim: ClaimId::new(&mc.id),
        kind: EvidenceKind::Benchmark,
        locator: Locator::Artifact(me.artifact.trim().to_string()),
        extraction: Derivation::Verified {
            method: ToolInvocation {
                command: me.command.trim().to_string(),
                tool_version: "shipping-manifest evidence.command".into(),
                env: vec![],
            },
            ran_by: runner,
            reruns,
        },
        supports: Attested {
            value: SupportRelation::Supports {
                strength: support_strength_for_tier(&mc.tier),
            },
            derivation: Derivation::Judged {
                by: judge_identity_for_provenance(mc.provenance.as_deref()),
                protocol: None,
                rationale: format!(
                    "Asserted by {} tier manifest claim {}.",
                    mc.tier, mc.id
                ),
                confidence: confidence_for_tier(&mc.tier),
            },
            at: ctx.now.clone(),
        },
    }))
}

/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
/// empty vec when:
/// - `last_verified` is absent;
/// - `last_verified.date` is null (replay loop hasn't run);
/// - `last_verified.value` is null (no primary observation).
///
/// When fully populated, returns a single Rerun bound to the FIRST
/// criterion's id, per the shipping convention that `value` is the
/// primary scalar metric.
fn translate_last_verified(
    lv: Option<&ManifestLastVerified>,
    first_criterion: Option<&CriterionId>,
    runner: &Identity,
) -> Vec<Rerun> {
    let Some(lv) = lv else {
        return vec![];
    };
    let (Some(date), Some(value)) = (lv.date.as_ref(), lv.value) else {
        return vec![];
    };

    let observed = match first_criterion {
        Some(crit) => vec![MetricObservation {
            criterion: crit.clone(),
            value,
            unit: None,
        }],
        None => vec![],
    };

    vec![Rerun {
        at: date.clone(),
        by: runner.clone(),
        observed,
        corpus_sha: lv.corpus_sha.clone(),
        // Shipping convention: a populated last_verified records a
        // PASSING re-run; divergence wouldn't update the manifest.
        outcome: ReproductionOutcome::Matched,
    }]
}

fn unspecified_runner_identity(provenance: Option<&str>) -> Identity {
    Identity {
        kind: IdentityKind::Automated,
        name: "unspecified-runner".into(),
        details: vec![IdentityDetail {
            key: "manifest_provenance".into(),
            value: provenance.unwrap_or("automatic").into(),
        }],
    }
}

fn judge_identity_for_provenance(provenance: Option<&str>) -> Identity {
    // Invariant 9: Automated cannot judge. Even `provenance: automatic`
    // becomes a degraded Human identity — the implicit judgment is by
    // the human who set up the CI pipeline, attestation tagged in
    // details for renderers.
    Identity {
        kind: IdentityKind::Human,
        name: "unspecified".into(),
        details: vec![IdentityDetail {
            key: "manifest_provenance".into(),
            value: provenance.unwrap_or("automatic").into(),
        }],
    }
}

fn support_strength_for_tier(tier: &str) -> Strength {
    match tier {
        "release" => Strength::Strong,
        "research" => Strength::Weak,
        _ => Strength::Moderate, // ci or unknown
    }
}

fn confidence_for_tier(tier: &str) -> Confidence {
    match tier {
        "release" => Confidence::High,
        "research" => Confidence::Low,
        _ => Confidence::Moderate,
    }
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

// ---------- Review-event sidecar (Phase 2a) ----------

/// Top-level shape of the `review_events.json` sidecar:
/// `{ "events": [ ... ] }`. Mirrors the agent's append-only log.
#[derive(Debug, Clone, Deserialize)]
pub struct ReviewEventSidecar {
    #[serde(default)]
    pub events: Vec<ManifestReviewEvent>,
}

/// Deserialization shape for one sidecar entry. The agent's
/// `review_events.json` writer produces exactly this shape.
///
/// Endorse / Dissent only in Phase 2a. `Challenge` (Phase 2b) requires
/// `category` + an optional `backed_by`; we accept those fields here
/// without yet routing them through synthesis — `translate_review_event`
/// rejects `challenge` for now with a clear error.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestReviewEvent {
    pub claim_id: String,
    pub kind: String, // "endorse" | "dissent" | "challenge" (rejected in 2a)
    pub author: ManifestReviewAuthor,
    pub rationale: String,
    pub timestamp: String,
    /// Optional explicit event_id. If absent, the translator computes a
    /// canonical-hash event_id over the payload — this is the safer
    /// default since the agent is expected to always write it.
    #[serde(default)]
    pub event_id: Option<String>,
    /// Structured per-check verdict from the model's submit_review tool.
    /// Preserved in the rendered output so reviewers can see which check
    /// the model ran, not just its overall verdict.
    #[serde(default)]
    pub checks: Option<serde_json::Value>,
    #[serde(default)]
    pub observed_value: Option<String>,
    #[serde(default)]
    pub tolerance: Option<String>,
    #[serde(default)]
    pub failure_reason: Option<String>,
    /// Phase 2b-only: for `challenge` events.
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub backed_by: Option<String>,
    /// Optional protocol pointer. Release-tier events must set this
    /// (invariant 10); validator enforcement is downstream.
    #[serde(default)]
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestReviewAuthor {
    pub kind: String, // "human" | "model" | "automated" | "organization" | "anonymous"
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub orcid: Option<String>,
    #[serde(default)]
    pub affiliation: Option<String>,
}

#[derive(Debug)]
pub enum ReviewTranslateError {
    UnknownKind { id: String, kind: String },
    UnknownAuthorKind { id: String, kind: String },
    ModelMissingVersion { id: String },
    ChallengeNotSupported { id: String },
}

impl std::fmt::Display for ReviewTranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewTranslateError::UnknownKind { id, kind } => {
                write!(f, "review event for {id}: unknown kind {kind:?} (expected endorse|dissent|challenge)")
            }
            ReviewTranslateError::UnknownAuthorKind { id, kind } => {
                write!(f, "review event for {id}: unknown author kind {kind:?}")
            }
            ReviewTranslateError::ModelMissingVersion { id } => {
                write!(f, "review event for {id}: author kind=model requires `version`")
            }
            ReviewTranslateError::ChallengeNotSupported { id } => {
                write!(f, "review event for {id}: kind=challenge requires Phase 2b backing-claim support; not supported by typed-trust 2a")
            }
        }
    }
}

impl std::error::Error for ReviewTranslateError {}

/// Translate one sidecar entry into a [`ReviewEvent`].
///
/// Event identity (`EventId`): uses the explicit `event_id` field when
/// present; otherwise computes a sha256 over the canonical JSON of the
/// event payload. This avoids the `(claim_id, author, kind, timestamp)`
/// tuple collision risk under sub-second concurrent runs.
pub fn translate_review_event(
    e: &ManifestReviewEvent,
) -> Result<crate::review::ReviewEvent, ReviewTranslateError> {
    use crate::review::{ReviewEvent, ReviewKind, Target};

    let kind = match e.kind.as_str() {
        "endorse" => ReviewKind::Endorse,
        "dissent" => ReviewKind::Dissent,
        "challenge" => {
            return Err(ReviewTranslateError::ChallengeNotSupported {
                id: e.claim_id.clone(),
            })
        }
        other => {
            return Err(ReviewTranslateError::UnknownKind {
                id: e.claim_id.clone(),
                kind: other.into(),
            })
        }
    };

    let by = translate_author(&e.claim_id, &e.author)?;

    let event_id = match &e.event_id {
        Some(s) => EventId::new(s.clone()),
        None => EventId::new(canonical_event_id(e)),
    };

    Ok(ReviewEvent {
        id: event_id,
        target: Target::Claim(ClaimId::new(&e.claim_id)),
        by,
        protocol: e.protocol.clone(),
        rationale: e.rationale.clone(),
        at: e.timestamp.clone(),
        kind,
    })
}

fn translate_author(
    claim_id: &str,
    a: &ManifestReviewAuthor,
) -> Result<Identity, ReviewTranslateError> {
    use crate::identity::IdentityDetail;

    let kind = match a.kind.as_str() {
        "human" => IdentityKind::Human,
        "model" => IdentityKind::Model,
        "automated" => IdentityKind::Automated,
        "organization" => IdentityKind::Organization,
        "anonymous" => IdentityKind::Anonymous,
        other => {
            return Err(ReviewTranslateError::UnknownAuthorKind {
                id: claim_id.into(),
                kind: other.into(),
            })
        }
    };

    if matches!(kind, IdentityKind::Model) && a.version.is_none() {
        return Err(ReviewTranslateError::ModelMissingVersion {
            id: claim_id.into(),
        });
    }

    let mut details: Vec<IdentityDetail> = Vec::new();
    if let Some(v) = &a.version {
        details.push(IdentityDetail {
            key: "version".into(),
            value: v.clone(),
        });
    }
    if let Some(c) = &a.context {
        details.push(IdentityDetail {
            key: "context".into(),
            value: c.clone(),
        });
    }
    if let Some(o) = &a.orcid {
        details.push(IdentityDetail {
            key: "orcid".into(),
            value: o.clone(),
        });
    }
    if let Some(af) = &a.affiliation {
        details.push(IdentityDetail {
            key: "affiliation".into(),
            value: af.clone(),
        });
    }

    Ok(Identity {
        kind,
        name: a.name.clone(),
        details,
    })
}

/// Canonical-hash event_id: sha256 of a canonically-ordered JSON
/// representation of the event payload. Deterministic and
/// collision-resistant under concurrent agent runs in the same second.
/// Distinct fields → distinct hashes; identical payloads → identical
/// hash (so a deliberate replay of the same recorded fixture produces
/// a stable id).
pub fn canonical_event_id(e: &ManifestReviewEvent) -> String {
    use sha2::{Digest, Sha256};

    // Build a serde_json::Value with keys inserted in a fixed order so
    // serde_json's preserve-insertion-order behavior (default for
    // Map<String, Value>) gives canonical output.
    let canonical = canonical_event_value(e);
    let bytes = serde_json::to_vec(&canonical).expect("serialize canonical event");
    let digest = Sha256::digest(&bytes);
    format!("sha256:{:x}", digest)
}

fn canonical_event_value(e: &ManifestReviewEvent) -> serde_json::Value {
    use serde_json::{Map, Value};

    let mut author = Map::new();
    author.insert("kind".into(), Value::String(e.author.kind.clone()));
    author.insert("name".into(), Value::String(e.author.name.clone()));
    if let Some(v) = &e.author.version {
        author.insert("version".into(), Value::String(v.clone()));
    }
    if let Some(c) = &e.author.context {
        author.insert("context".into(), Value::String(c.clone()));
    }
    if let Some(o) = &e.author.orcid {
        author.insert("orcid".into(), Value::String(o.clone()));
    }
    if let Some(af) = &e.author.affiliation {
        author.insert("affiliation".into(), Value::String(af.clone()));
    }

    let mut m = Map::new();
    m.insert("claim_id".into(), Value::String(e.claim_id.clone()));
    m.insert("kind".into(), Value::String(e.kind.clone()));
    m.insert("author".into(), Value::Object(author));
    m.insert("rationale".into(), Value::String(e.rationale.clone()));
    m.insert("timestamp".into(), Value::String(e.timestamp.clone()));
    if let Some(c) = &e.checks {
        m.insert("checks".into(), c.clone());
    }
    if let Some(o) = &e.observed_value {
        m.insert("observed_value".into(), Value::String(o.clone()));
    }
    if let Some(t) = &e.tolerance {
        m.insert("tolerance".into(), Value::String(t.clone()));
    }
    if let Some(f) = &e.failure_reason {
        m.insert("failure_reason".into(), Value::String(f.clone()));
    }
    if let Some(c) = &e.category {
        m.insert("category".into(), Value::String(c.clone()));
    }
    if let Some(b) = &e.backed_by {
        m.insert("backed_by".into(), Value::String(b.clone()));
    }
    if let Some(p) = &e.protocol {
        m.insert("protocol".into(), Value::String(p.clone()));
    }

    Value::Object(m)
}
