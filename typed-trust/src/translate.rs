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
use crate::evidence::{
    Evidence, EvidenceKind, ReplayReason, ReplayStatus, Strength, SupportRelation,
};
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
    pub provenance: Option<ManifestProvenance>,
    pub last_verified: Option<ManifestLastVerified>,
    pub assumptions: Option<Vec<String>>,
    pub failure_modes: Option<Vec<String>>,
}

/// Phase 5 PR2: the manifest's `provenance` field accepts either the
/// legacy string form (`provenance: automatic`) or a structured object
/// (`provenance: { kind, source_id, ... }`). The structured form is
/// what `evident-extract` writes; the legacy form is what every
/// pre-Phase-5 manifest has and must keep working unchanged.
///
/// Use `effective_kind()` to get the kind string for the existing
/// callers that branch on `automatic` / `human` / `peer-reviewed`
/// without caring about the new sub-fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestProvenance {
    /// Pre-Phase-5 form: `provenance: automatic | human | peer-reviewed`
    /// or any other free-form string. Only `kind` is carried.
    Legacy(String),
    /// Phase 5 form: structured provenance with extractor metadata.
    Structured(ProvenanceBlock),
}

/// Phase 5 PR2: the structured `provenance:` block.
///
/// `kind` is the only required field. Everything else is optional so
/// a manifest can declare `extracted-from-paper` without committing to
/// a particular extractor or source_id at authoring time.
///
/// `deny_unknown_fields` (codex F-PR2-CR2) catches typos like
/// `source_contxt:` at parse time instead of silently dropping them.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceBlock {
    /// The provenance discriminator. Phase 5 introduces
    /// `extracted-from-paper` and `extracted-from-repo`; legacy
    /// values (`automatic`, `human`, `peer-reviewed`) are also
    /// accepted here, so a manifest author who prefers the
    /// structured form can use it for legacy provenance too.
    pub kind: String,
    /// Opaque source identifier — for papers: `arxiv:2501.12345v2`,
    /// `doi:10.1234/xyz`. For repos: `github:org/repo@<sha>`.
    pub source_id: Option<String>,
    /// SHA-256 of the source artifact (e.g. the PDF or the repo
    /// snapshot) so re-extraction is reproducible against the same
    /// source bytes.
    pub source_sha: Option<String>,
    /// Provenance of the text the claim was extracted FROM. Distinct
    /// from `kind`, which is the provenance of the CLAIM. Parses as
    /// a typed enum so an unknown value (`source_context:
    /// completely_made_up`) is rejected at parse time, not at
    /// translate time (codex F-PR2-CR1). This closes the
    /// `list_claims` bypass — every value MCP surfaces is one of
    /// the three legal strings.
    pub source_context: Option<SourceContext>,
    /// Extractor metadata. Optional so manifests can pre-declare
    /// structured provenance before the extractor runs.
    pub extractor: Option<ExtractorBlock>,
    /// Curator identity (set after a human review, null at
    /// extraction time). Free-form here so PR2 doesn't commit to a
    /// curator-identity schema; PR3 will refine.
    pub curator: Option<serde_yaml_ng::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractorBlock {
    pub model: Option<String>,
    pub model_version: Option<String>,
    pub extracted_at: Option<String>,
}

/// Phase 5 PR2: provenance of the source text a claim was extracted
/// FROM. Distinct from `provenance.kind`, which is the provenance of
/// the claim itself.
///
/// Parses as `#[serde(rename_all = "snake_case")]` so the YAML strings
/// are `repo_authored`, `copied_external_text`, `unknown`. Anything
/// else fails at deserialization time with a serde error naming the
/// unknown variant — the validator-at-translate-time pattern was
/// replaced by parse-time enum validation per codex F-PR2-CR1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceContext {
    /// Text was written for the artifact it lives in (e.g. the repo's
    /// own README, the paper's own body).
    RepoAuthored,
    /// Text was copied verbatim from a separate authoritative source
    /// (vendored README, corporate marketing copy, etc.).
    CopiedExternalText,
    /// Extractor could not determine.
    Unknown,
}

impl SourceContext {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceContext::RepoAuthored => "repo_authored",
            SourceContext::CopiedExternalText => "copied_external_text",
            SourceContext::Unknown => "unknown",
        }
    }
}

impl ManifestProvenance {
    /// The provenance kind string — `automatic`, `human`,
    /// `peer-reviewed`, `extracted-from-paper`, `extracted-from-repo`,
    /// etc. Callers that historically branched on `mc.provenance` as a
    /// string use this; the structured form's `kind` is returned
    /// unchanged.
    pub fn effective_kind(&self) -> &str {
        match self {
            ManifestProvenance::Legacy(s) => s.as_str(),
            ManifestProvenance::Structured(b) => b.kind.as_str(),
        }
    }
    pub fn source_id(&self) -> Option<&str> {
        match self {
            ManifestProvenance::Legacy(_) => None,
            ManifestProvenance::Structured(b) => b.source_id.as_deref(),
        }
    }
    pub fn source_sha(&self) -> Option<&str> {
        match self {
            ManifestProvenance::Legacy(_) => None,
            ManifestProvenance::Structured(b) => b.source_sha.as_deref(),
        }
    }
    pub fn source_context(&self) -> Option<&'static str> {
        match self {
            ManifestProvenance::Legacy(_) => None,
            ManifestProvenance::Structured(b) => b.source_context.map(|s| s.as_str()),
        }
    }
    pub fn extractor_model(&self) -> Option<&str> {
        match self {
            ManifestProvenance::Legacy(_) => None,
            ManifestProvenance::Structured(b) => {
                b.extractor.as_ref().and_then(|e| e.model.as_deref())
            }
        }
    }
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
    /// Phase 5: optional replay-path status. Defaults to
    /// `not_attempted` when absent, preserving the meaning of
    /// hand-authored manifests pre-Phase-5.
    #[serde(default)]
    pub replay_status: Option<String>,
    /// Phase 5: optional structured reason a replay is
    /// unavailable. Pair-validator in `translate_evidence` enforces
    /// the legal `(replay_status, replay_reason)` combinations.
    #[serde(default)]
    pub replay_reason: Option<String>,
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
    /// Phase 5: the `evidence.replay_status` field was not one of
    /// `available | not_attempted | unavailable_artifacts`.
    InvalidReplayStatus { id: String, value: String },
    /// Phase 5: the `evidence.replay_reason` field was not one of the
    /// ten known reason strings.
    InvalidReplayReason { id: String, value: String },
    /// Phase 5: the `(replay_status, replay_reason)` pair is not in
    /// the legal set. Legal combinations:
    ///   (available, None), (not_attempted, None),
    ///   (unavailable_artifacts, Some(_))
    IllegalReplayPair {
        id: String,
        status: String,
        reason: Option<String>,
    },
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
            TranslateError::InvalidReplayStatus { id, value } => write!(
                f,
                "claim {id}: evidence.replay_status {value:?} is not one of \
                 available | not_attempted | unavailable_artifacts"
            ),
            TranslateError::InvalidReplayReason { id, value } => write!(
                f,
                "claim {id}: evidence.replay_reason {value:?} is not a known reason"
            ),
            TranslateError::IllegalReplayPair { id, status, reason } => write!(
                f,
                "claim {id}: illegal (replay_status, replay_reason) pair \
                 ({status:?}, {reason:?}); legal pairs are (available, null), \
                 (not_attempted, null), (unavailable_artifacts, <reason>)"
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
    let provenance_kind = mc.provenance.as_ref().map(|p| p.effective_kind());
    let runner = unspecified_runner_identity(provenance_kind);
    let first_criterion = criteria.first().map(|c| c.id.clone());
    let reruns = translate_last_verified(
        mc.last_verified.as_ref(),
        first_criterion.as_ref(),
        &runner,
    );
    let (replay_status, replay_reason) = parse_replay_fields(&mc.id, me)?;

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
                by: judge_identity_for_provenance(provenance_kind),
                protocol: None,
                rationale: format!(
                    "Asserted by {} tier manifest claim {}.",
                    mc.tier, mc.id
                ),
                confidence: confidence_for_tier(&mc.tier),
            },
            at: ctx.now.clone(),
        },
        replay_status,
        replay_reason,
    }))
}

/// Phase 5: parse + validate `evidence.replay_status` and
/// `evidence.replay_reason`. Returns the typed pair, or a translation
/// error explaining which rule was violated. The legal pairs are:
///   (available, None), (not_attempted, None),
///   (unavailable_artifacts, Some(_)).
/// Anything else is rejected here so downstream consumers can rely on
/// the invariant.
fn parse_replay_fields(
    claim_id: &str,
    me: &ManifestEvidence,
) -> Result<(ReplayStatus, Option<ReplayReason>), TranslateError> {
    let status = match me.replay_status.as_deref() {
        None => ReplayStatus::NotAttempted,
        Some(s) => match s {
            "available" => ReplayStatus::Available,
            "not_attempted" => ReplayStatus::NotAttempted,
            "unavailable_artifacts" => ReplayStatus::UnavailableArtifacts,
            _ => {
                return Err(TranslateError::InvalidReplayStatus {
                    id: claim_id.into(),
                    value: s.into(),
                });
            }
        },
    };

    let reason = match me.replay_reason.as_deref() {
        None => None,
        Some(s) => Some(match s {
            "code_private" => ReplayReason::CodePrivate,
            "data_unavailable" => ReplayReason::DataUnavailable,
            "license_restricted" => ReplayReason::LicenseRestricted,
            "compute_unavailable" => ReplayReason::ComputeUnavailable,
            "environment_unavailable" => ReplayReason::EnvironmentUnavailable,
            "dependency_unavailable" => ReplayReason::DependencyUnavailable,
            "external_service_unavailable" => ReplayReason::ExternalServiceUnavailable,
            "benchmark_unspecified" => ReplayReason::BenchmarkUnspecified,
            "instructions_missing" => ReplayReason::InstructionsMissing,
            "requires_human_evaluation" => ReplayReason::RequiresHumanEvaluation,
            _ => {
                return Err(TranslateError::InvalidReplayReason {
                    id: claim_id.into(),
                    value: s.into(),
                });
            }
        }),
    };

    let legal = matches!(
        (status, reason.is_some()),
        (ReplayStatus::Available, false)
            | (ReplayStatus::NotAttempted, false)
            | (ReplayStatus::UnavailableArtifacts, true)
    );
    if !legal {
        return Err(TranslateError::IllegalReplayPair {
            id: claim_id.into(),
            status: me.replay_status.clone().unwrap_or_else(|| match status {
                ReplayStatus::Available => "available",
                ReplayStatus::NotAttempted => "not_attempted",
                ReplayStatus::UnavailableArtifacts => "unavailable_artifacts",
            }.to_string()),
            reason: me.replay_reason.clone(),
        });
    }
    Ok((status, reason))
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
    /// Phase 2b: `challenge` events carry this structured block. Always
    /// required when ``kind == "challenge"``. The `violation` and
    /// `backing_claim` fields are populated for substantive categories
    /// only; procedural categories carry only `category`.
    #[serde(default)]
    pub challenge: Option<ManifestChallengeBlock>,
    /// Phase 2d: optional `target` block. When absent, the event
    /// targets the entry's `claim_id` (Target::Claim). When present
    /// with `type: review_event`, the event targets a prior
    /// ReviewEvent. Phase 2d-i scopes the supported types to
    /// `claim` and `review_event` only; other variants are
    /// translator errors (F-2D-13).
    #[serde(default)]
    pub target: Option<ManifestTargetBlock>,
    /// Phase 2d: required when `kind == "supersede"`. Carries the
    /// successor `AttestedId` that replaces the targeted event's
    /// attestation. Empty for any other `kind`.
    #[serde(default)]
    pub supersede: Option<ManifestSupersedeBlock>,
    /// Phase 5 PR3: required when `kind == "promote_from_extracted"`.
    /// Carries the lifecycle-transition metadata the validator
    /// matches against the manifest's extracted-claim tier.
    #[serde(default)]
    pub promote_from_extracted: Option<ManifestPromoteFromExtractedBlock>,
    /// Optional protocol pointer. Release-tier events must set this
    /// (invariant 10); validator enforcement is downstream.
    #[serde(default)]
    pub protocol: Option<String>,
}

/// Phase 5 PR3: the structured block carried by
/// `kind: promote_from_extracted` sidecar entries.
///
/// `reviewed_extraction_sha` pins the curator's review to a specific
/// version of the extracted `evident.yaml`. The validator uses it to
/// reject promotions of *different* extraction runs against the
/// reviewed one, so a re-extraction can't silently inherit an old
/// curator's blessing.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPromoteFromExtractedBlock {
    pub target_claim: String,
    pub from_tier: String,
    pub to_tier: String,
    pub reviewed_extraction_sha: String,
}

/// Phase 2d sidecar `target` block. Codex F-2D-4: singular shape,
/// no duplication with the per-variant fields (e.g., we do NOT
/// also carry `supersede.target_event_id`). Phase 2d-i restricts
/// `type` to `claim` and `review_event` (F-2D-13).
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestTargetBlock {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

/// Phase 2d sidecar `supersede` block. Carries the successor
/// `AttestedId` that replaces the targeted event's attestation.
/// The targeted event id lives in the outer `target` block — codex
/// F-2D-4 explicitly rejected duplicating it here.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestSupersedeBlock {
    pub successor: String,
}

/// Phase 2b challenge block carried by `kind: challenge` events.
///
/// `target_criterion_id` names which of the target claim's criteria
/// the Challenge attacks — required even for single-criterion targets,
/// so multi-criterion targets are unambiguous.
///
/// `violation` is the model-reported contradiction: which metric, what
/// the observed value was, the comparator and bound from the target
/// tolerance. The agent — NOT the model — constructs `backing_claim`
/// from this tuple; both are persisted to the sidecar so the
/// translator can re-derive when needed and so reviewers can audit
/// the model's reported violation against the constructed backing.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestChallengeBlock {
    pub category: String,
    #[serde(default)]
    pub target_criterion_id: Option<String>,
    /// The model-reported violation. Required for substantive
    /// categories; absent for procedural Challenges.
    #[serde(default)]
    pub violation: Option<ManifestViolation>,
    /// Agent-constructed backing claim. Required for substantive
    /// categories; absent for procedural Challenges.
    #[serde(default)]
    pub backing_claim: Option<ManifestClaim>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestViolation {
    pub metric: String,
    pub observed_value: f64,
    pub bound: f64,
    pub comparator: String,
    pub citation: String,
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
    /// `kind: challenge` events must carry a `challenge` block (which
    /// names the category at minimum, plus violation + backing claim
    /// for substantive categories).
    ChallengeMissingBlock { id: String },
    /// A substantive Challenge category (anything outside the closed
    /// procedural list) requires `challenge.backing_claim`.
    SubstantiveChallengeMissingBacking { id: String, category: String },
    /// A substantive Challenge requires the model-reported violation
    /// tuple (and the criterion it targets) for audit and canonical
    /// identity. A hand-authored sidecar with backing_claim but no
    /// violation would otherwise still flip status without an audit
    /// trail.
    SubstantiveChallengeMissingViolation { id: String, category: String },
    SubstantiveChallengeMissingTargetCriterion { id: String, category: String },
    /// A procedural Challenge category MUST NOT carry a backing claim
    /// — the procedural fact itself moves status, and a backing claim
    /// would be redundant (and possibly contradictory).
    ProceduralChallengeWithBacking { id: String, category: String },
    /// Backing claim's `id` collides with the target claim's id, which
    /// would form a one-step cycle in the challenge-backing graph.
    BackingClaimMatchesTargetId { id: String },
    /// Phase 2d-i: `kind: supersede` events must carry an explicit
    /// `target` block (a Supersede defaulting to Target::Claim
    /// would be a confused entry).
    SupersedeMissingTarget { id: String },
    /// Phase 2d-i: `kind: supersede` events must carry a non-empty
    /// `supersede.successor` field (the AttestedId replacing the
    /// targeted attestation).
    SupersedeMissingSuccessor { id: String },
    /// Phase 2d-i: `target.type` outside the supported set
    /// (`claim`, `review_event`). Other Target variants
    /// (CriterionResult, SupportRelation, ClaimAttestation,
    /// Evidence, Provenance, TrustReport, Criterion) require schema
    /// extensions deferred to Phase 2e+ (codex F-2D-13).
    UnsupportedTargetType { id: String, target_type: String },
    /// Phase 5 PR3: `kind: promote_from_extracted` event must carry a
    /// `promote_from_extracted` block (target_claim, from_tier,
    /// to_tier, reviewed_extraction_sha).
    PromoteFromExtractedMissingBlock { id: String },
    /// Phase 5 PR3 (codex F-PR3-CR4): `reviewed_extraction_sha` must
    /// be non-empty — the whole point is to pin the curator's review
    /// to a specific extraction sha.
    PromoteFromExtractedEmptySha { id: String },
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
            ReviewTranslateError::ChallengeMissingBlock { id } => {
                write!(f, "review event for {id}: kind=challenge requires a `challenge` block with category and (for substantive categories) violation + backing_claim")
            }
            ReviewTranslateError::SubstantiveChallengeMissingBacking { id, category } => {
                write!(f, "review event for {id}: substantive challenge category {category:?} requires `challenge.backing_claim`")
            }
            ReviewTranslateError::SubstantiveChallengeMissingViolation { id, category } => {
                write!(f, "review event for {id}: substantive challenge category {category:?} requires `challenge.violation` (the model-reported violation tuple is load-bearing for audit and canonical identity)")
            }
            ReviewTranslateError::SubstantiveChallengeMissingTargetCriterion { id, category } => {
                write!(f, "review event for {id}: substantive challenge category {category:?} requires `challenge.target_criterion_id` to name which target criterion is contradicted")
            }
            ReviewTranslateError::ProceduralChallengeWithBacking { id, category } => {
                write!(f, "review event for {id}: procedural challenge category {category:?} must not carry a backing_claim; the procedural fact moves status on its own")
            }
            ReviewTranslateError::BackingClaimMatchesTargetId { id } => {
                write!(f, "review event for {id}: backing claim's `id` matches the target claim id — one-step cycle in the challenge-backing graph")
            }
            ReviewTranslateError::SupersedeMissingTarget { id } => {
                write!(f, "review event for {id}: kind=supersede requires an explicit `target` block (a Supersede must point at a prior event)")
            }
            ReviewTranslateError::SupersedeMissingSuccessor { id } => {
                write!(f, "review event for {id}: kind=supersede requires a non-empty `supersede.successor` field (the AttestedId replacing the targeted attestation)")
            }
            ReviewTranslateError::UnsupportedTargetType { id, target_type } => {
                write!(f, "review event for {id}: target.type {target_type:?} not supported in Phase 2d-i (supported: claim, review_event)")
            }
            ReviewTranslateError::PromoteFromExtractedMissingBlock { id } => {
                write!(f, "review event for {id}: kind=promote_from_extracted requires a `promote_from_extracted` block with target_claim, from_tier, to_tier, reviewed_extraction_sha")
            }
            ReviewTranslateError::PromoteFromExtractedEmptySha { id } => {
                write!(f, "review event for {id}: kind=promote_from_extracted requires a non-empty `reviewed_extraction_sha` (the field pins the curator's review to a specific extraction)")
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
    use crate::review::{ReviewEvent, ReviewKind};

    let kind = match e.kind.as_str() {
        "endorse" => ReviewKind::Endorse,
        "dissent" => ReviewKind::Dissent,
        "challenge" => translate_challenge_kind(e)?,
        "supersede" => translate_supersede_kind(e)?,
        "promote_from_extracted" => translate_promote_from_extracted_kind(e)?,
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

    // Phase 2d-i: target resolution. When `target` is present, route
    // to the appropriate Target variant. Without `target`, default
    // to Target::Claim (Phase 2a/b/c behavior, backward compatible).
    let target = translate_target(&e.claim_id, e.target.as_ref(), &e.kind)?;

    Ok(ReviewEvent {
        id: event_id,
        target,
        by,
        protocol: e.protocol.clone(),
        rationale: e.rationale.clone(),
        at: e.timestamp.clone(),
        kind,
    })
}

/// Phase 2d-i target resolution. Scoped to two supported types:
/// `claim` (the implicit default) and `review_event` (the new
/// case Phase 2d-i needs for Supersede). Other variants are
/// translator errors with a clear message (codex F-2D-13).
fn translate_target(
    claim_id: &str,
    target_block: Option<&ManifestTargetBlock>,
    event_kind: &str,
) -> Result<crate::review::Target, ReviewTranslateError> {
    use crate::review::Target;

    // Supersede MUST carry an explicit target. A Supersede defaulting
    // to Target::Claim would be a confused entry: Supersede semantics
    // operate on prior events, not on the claim itself.
    if event_kind == "supersede" && target_block.is_none() {
        return Err(ReviewTranslateError::SupersedeMissingTarget {
            id: claim_id.into(),
        });
    }

    let Some(block) = target_block else {
        return Ok(Target::Claim(ClaimId::new(claim_id)));
    };

    match block.kind.as_str() {
        "claim" => Ok(Target::Claim(ClaimId::new(&block.id))),
        "review_event" => Ok(Target::ReviewEvent(EventId::new(&block.id))),
        other => Err(ReviewTranslateError::UnsupportedTargetType {
            id: claim_id.into(),
            target_type: other.into(),
        }),
    }
}

/// Phase 2d-i `kind: supersede` translation. Requires the
/// `supersede.successor` field; codex F-2D-4 explicitly forbids
/// duplicating the targeted event id here (it lives in `target`).
fn translate_supersede_kind(
    e: &ManifestReviewEvent,
) -> Result<crate::review::ReviewKind, ReviewTranslateError> {
    use crate::ids::AttestedId;
    use crate::review::ReviewKind;

    let block = e
        .supersede
        .as_ref()
        .ok_or_else(|| ReviewTranslateError::SupersedeMissingSuccessor {
            id: e.claim_id.clone(),
        })?;
    if block.successor.trim().is_empty() {
        return Err(ReviewTranslateError::SupersedeMissingSuccessor {
            id: e.claim_id.clone(),
        });
    }
    Ok(ReviewKind::Supersede {
        successor: AttestedId::new(block.successor.clone()),
    })
}

/// Phase 5 PR3: translate a `kind: promote_from_extracted` sidecar
/// entry into the typed `ReviewKind::PromoteFromExtracted` variant.
/// Requires the `promote_from_extracted` block — the entry is
/// rejected otherwise. Also rejects empty `reviewed_extraction_sha`
/// (codex F-PR3-CR4: the field must pin a specific extraction).
fn translate_promote_from_extracted_kind(
    e: &ManifestReviewEvent,
) -> Result<crate::review::ReviewKind, ReviewTranslateError> {
    use crate::review::ReviewKind;

    let block = e.promote_from_extracted.as_ref().ok_or_else(|| {
        ReviewTranslateError::PromoteFromExtractedMissingBlock {
            id: e.claim_id.clone(),
        }
    })?;
    if block.reviewed_extraction_sha.trim().is_empty() {
        return Err(ReviewTranslateError::PromoteFromExtractedEmptySha {
            id: e.claim_id.clone(),
        });
    }
    Ok(ReviewKind::PromoteFromExtracted {
        target_claim: ClaimId::new(&block.target_claim),
        from_tier: block.from_tier.clone(),
        to_tier: block.to_tier.clone(),
        reviewed_extraction_sha: block.reviewed_extraction_sha.clone(),
    })
}

// ----------------------------------------------------------------------
// Phase 5 PR3: validate_promotion_rules — enforce the five
// invariants from `EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md` v3.
// ----------------------------------------------------------------------

/// Phase 5 PR3: structured errors for the promotion-gate validator.
#[derive(Debug)]
pub enum PromotionError {
    /// The manifest sets a non-research tier on an extracted claim
    /// but no matching `PromoteFromExtracted` event was found in the
    /// sidecar.
    MissingPromotionEvent {
        claim_id: String,
        current_tier: String,
        /// Multi-step follow-up: the specific transition that was
        /// missing (`(from_tier, to_tier)`). For single-step
        /// promotions this names `(research, claim.tier)`. For
        /// `tier: release` claims it names whichever leg is
        /// missing.
        missing_transition: (String, String),
    },
    /// The matching event's `event_date` predates the claim's
    /// `provenance.extractor.extracted_at`. The curator cannot have
    /// reviewed an extraction that did not yet exist.
    PromotionPredatesExtraction {
        claim_id: String,
        event_date: String,
        extracted_at: String,
    },
    /// Multi-step follow-up: a later transition's event predates
    /// the prior transition's event. The chain must be
    /// chronologically ordered — a curator cannot have promoted
    /// `ci -> release` before promoting `research -> ci`.
    PromotionChainOutOfOrder {
        claim_id: String,
        prior_transition: (String, String),
        prior_event_date: String,
        next_transition: (String, String),
        next_event_date: String,
    },
}

impl std::fmt::Display for PromotionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromotionError::MissingPromotionEvent {
                claim_id,
                current_tier,
                missing_transition,
            } => write!(
                f,
                "extracted claim {claim_id} at tier {current_tier:?} requires a \
                 matching `promote_from_extracted` review event for the \
                 {from:?} -> {to:?} transition",
                from = missing_transition.0,
                to = missing_transition.1,
            ),
            PromotionError::PromotionPredatesExtraction {
                claim_id,
                event_date,
                extracted_at,
            } => write!(
                f,
                "extracted claim {claim_id}: promote_from_extracted event_date \
                 {event_date} predates extracted_at {extracted_at}"
            ),
            PromotionError::PromotionChainOutOfOrder {
                claim_id,
                prior_transition,
                prior_event_date,
                next_transition,
                next_event_date,
            } => write!(
                f,
                "extracted claim {claim_id}: promotion chain out of order — \
                 {next_from:?} -> {next_to:?} event_date {next_event_date} \
                 predates {prior_from:?} -> {prior_to:?} event_date \
                 {prior_event_date}",
                prior_from = prior_transition.0,
                prior_to = prior_transition.1,
                next_from = next_transition.0,
                next_to = next_transition.1,
            ),
        }
    }
}

impl std::error::Error for PromotionError {}

/// Phase 5 PR3 + multi-step extension: enforce the promotion-gate
/// invariants on a single claim and the relevant sidecar events.
///
/// Rule 1 (gate-on-tier): the gate fires only when the claim is
/// extracted AND `tier` is not `research`. Non-extracted claims and
/// research-tier extracted claims pass without checking events.
///
/// Rule 2 (chain matching): for the gated claim, the required chain
/// of `PromoteFromExtracted` events must exist:
/// - `tier: ci` requires one event (`research -> ci`).
/// - `tier: release` requires two events (`research -> ci` and
///   `ci -> release`).
///   Each transition's latest event by `(timestamp, event_id)`
///   wins.
///
/// Rule 3 (ordering): the first event's `event_date` must not
/// predate `provenance.extractor.extracted_at`. Subsequent
/// transitions must be chronologically after the prior one
/// (you can't promote `ci -> release` before promoting
/// `research -> ci`).
///
/// Rule 4 (uniqueness / latest-event): if multiple events exist
/// for the same `(from_tier, to_tier)`, the latest by
/// `event_date` (with `event_id` as deterministic tiebreaker) is
/// authoritative.
///
/// Rule 5 (Endorse-independence): Endorse / Dissent / Challenge
/// events on the claim are silently ignored here — they're
/// orthogonal to lifecycle transitions. The render layer is what
/// keeps them visually separate.
pub fn validate_promotion_rules(
    claim: &ManifestClaim,
    events: &[ManifestReviewEvent],
) -> Result<(), PromotionError> {
    // Rule 1a: only extracted claims are gated.
    if !is_extracted_claim(claim) {
        return Ok(());
    }
    // Rule 1b: research-tier extracted claims need no promotion.
    if claim.tier == "research" {
        return Ok(());
    }

    // Rule 2: determine the required chain of transitions for this
    // claim's tier and check each step.
    let Some(chain_transitions) = required_chain_for(&claim.tier) else {
        // Codex F-MULTISTEP-CR1 (P2): an unknown gated tier (e.g.
        // `staging`) without a chain definition would otherwise
        // pass silently. Treat it as an unconditional missing-event
        // error so the gate stays closed.
        return Err(PromotionError::MissingPromotionEvent {
            claim_id: claim.id.clone(),
            current_tier: claim.tier.clone(),
            missing_transition: ("research".into(), claim.tier.clone()),
        });
    };

    let extracted_at = claim
        .provenance
        .as_ref()
        .and_then(extracted_at_of_provenance);

    let mut prior_step: Option<(&'static str, &'static str, String)> = None;
    for &(from, to) in chain_transitions.iter() {
        let matching: Vec<&ManifestReviewEvent> = events
            .iter()
            .filter(|e| matches_transition(e, claim, from, to))
            .collect();
        let Some(authoritative) = pick_latest_by_event_date(&matching) else {
            return Err(PromotionError::MissingPromotionEvent {
                claim_id: claim.id.clone(),
                current_tier: claim.tier.clone(),
                missing_transition: (from.into(), to.into()),
            });
        };
        // First-event ordering: must not predate extracted_at.
        if prior_step.is_none() {
            if let Some(ext) = extracted_at {
                if authoritative.timestamp.as_str() < ext {
                    return Err(PromotionError::PromotionPredatesExtraction {
                        claim_id: claim.id.clone(),
                        event_date: authoritative.timestamp.clone(),
                        extracted_at: ext.to_string(),
                    });
                }
            }
        }
        // Chain ordering: subsequent transitions must not predate
        // the prior transition's event.
        if let Some((prior_from, prior_to, ref prior_ts)) = prior_step {
            if authoritative.timestamp.as_str() < prior_ts.as_str() {
                return Err(PromotionError::PromotionChainOutOfOrder {
                    claim_id: claim.id.clone(),
                    prior_transition: (prior_from.into(), prior_to.into()),
                    prior_event_date: prior_ts.clone(),
                    next_transition: (from.into(), to.into()),
                    next_event_date: authoritative.timestamp.clone(),
                });
            }
        }
        prior_step = Some((from, to, authoritative.timestamp.clone()));
    }
    Ok(())
}

/// The required chain of (from_tier, to_tier) transitions for a
/// claim at the given target tier. PR3+multi-step: tiers are
/// linear (research < ci < release) so the chain is uniquely
/// determined.
///
/// Returns ``None`` for tiers outside the known ladder (e.g.
/// ``staging``). The caller treats ``None`` as a gate-still-closed
/// signal — codex F-MULTISTEP-CR1 P2 fix to prevent a malformed
/// tier value from silently bypassing the validator.
fn required_chain_for(
    tier: &str,
) -> Option<&'static [(&'static str, &'static str)]> {
    match tier {
        "ci" => Some(&[("research", "ci")]),
        "release" => Some(&[("research", "ci"), ("ci", "release")]),
        _ => None,
    }
}

fn is_extracted_claim(claim: &ManifestClaim) -> bool {
    let Some(prov) = claim.provenance.as_ref() else {
        return false;
    };
    matches!(
        prov.effective_kind(),
        "extracted-from-paper" | "extracted-from-repo"
    )
}

/// Does this sidecar entry match a specific `(from_tier, to_tier)`
/// transition for the given claim?
///
/// Codex F-PR3-CR3 was the single-step version of this check. The
/// multi-step extension parametrises the transition so the
/// validator can require each leg of a `research -> ci -> release`
/// chain independently.
fn matches_transition(
    e: &ManifestReviewEvent,
    claim: &ManifestClaim,
    from_tier: &str,
    to_tier: &str,
) -> bool {
    let Some(block) = e.promote_from_extracted.as_ref() else {
        return false;
    };
    block.target_claim == claim.id
        && block.from_tier == from_tier
        && block.to_tier == to_tier
}

/// Pick the latest matching event by `(timestamp, event_id)` —
/// timestamp is primary, canonical event_id is the deterministic
/// tiebreaker for same-timestamp duplicates (codex F-PR3-CR2).
///
/// Tiebreaker rationale: when two curators emit
/// PromoteFromExtracted events at the same wall-clock second with
/// different `reviewed_extraction_sha`, we need a deterministic
/// winner so that re-running the validator on a re-ordered sidecar
/// produces the same result. The canonical event_id (sha256 of the
/// payload) is a stable, content-addressed tiebreaker.
///
/// Timestamp comparison itself is still string-lexicographic.
/// That's correct for normalized UTC strings ending in `Z` but
/// would mis-order mixed-timezone offsets. The framework's
/// timestamp fields are strings throughout; a chrono migration is
/// a separate codebase-wide concern (codex F-PR3-CR1 flagged this
/// for follow-up).
fn pick_latest_by_event_date<'a>(
    matching: &[&'a ManifestReviewEvent],
) -> Option<&'a ManifestReviewEvent> {
    matching
        .iter()
        .max_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| canonical_event_id(a).cmp(&canonical_event_id(b)))
        })
        .copied()
}

fn extracted_at_of_provenance(prov: &ManifestProvenance) -> Option<&str> {
    match prov {
        ManifestProvenance::Legacy(_) => None,
        ManifestProvenance::Structured(b) => b
            .extractor
            .as_ref()
            .and_then(|e| e.extracted_at.as_deref()),
    }
}

/// Procedural Challenge categories — typed-trust's closed list that
/// MAY move render status without a backing claim. Mirrors
/// `synthesize::is_procedural_category` but operates on the
/// snake_case sidecar string so we can enforce the rule at
/// translation time.
fn is_procedural_category_str(category: &str) -> bool {
    matches!(
        category,
        "artifact_unavailable"
            | "hash_mismatch"
            | "command_failure"
            | "conflict_of_interest"
            | "peer_review_unverifiable"
    )
}

/// Map a snake_case category string from the sidecar onto the
/// `ChallengeCategory` enum. Unknown strings become `Other(_)`, which
/// is treated as substantive.
fn translate_challenge_category(s: &str) -> crate::review::ChallengeCategory {
    use crate::review::ChallengeCategory as C;
    match s {
        "missing_control" => C::MissingControl,
        "weak_statistics" => C::WeakStatistics,
        "confound" => C::Confound,
        "unverifiable_assumption" => C::UnverifiableAssumption,
        "missing_benchmark" => C::MissingBenchmark,
        "reproducibility_risk" => C::ReproducibilityRisk,
        "artifact_unavailable" => C::ArtifactUnavailable,
        "hash_mismatch" => C::HashMismatch,
        "command_failure" => C::CommandFailure,
        "conflict_of_interest" => C::ConflictOfInterest,
        "peer_review_unverifiable" => C::PeerReviewUnverifiable,
        other => C::Other(other.into()),
    }
}

/// Translate a `kind: challenge` sidecar entry's challenge block into
/// a `ReviewKind::Challenge`. Enforces:
/// - `challenge` block is present;
/// - substantive categories carry `backing_claim`;
/// - procedural categories do NOT carry `backing_claim`;
/// - backing claim's id ≠ target claim's id (cycle guard).
fn translate_challenge_kind(
    e: &ManifestReviewEvent,
) -> Result<crate::review::ReviewKind, ReviewTranslateError> {
    use crate::review::ReviewKind;

    let block = e
        .challenge
        .as_ref()
        .ok_or_else(|| ReviewTranslateError::ChallengeMissingBlock {
            id: e.claim_id.clone(),
        })?;

    let category = translate_challenge_category(&block.category);
    let procedural = is_procedural_category_str(&block.category);

    if procedural && block.backing_claim.is_some() {
        return Err(ReviewTranslateError::ProceduralChallengeWithBacking {
            id: e.claim_id.clone(),
            category: block.category.clone(),
        });
    }
    if !procedural {
        // Substantive Challenges require the full audit triple:
        // backing claim, violation tuple, target_criterion_id. The
        // backing claim alone is insufficient for audit because the
        // model-reported violation is what the agent's
        // build_backing_claim was derived from — its absence in a
        // sidecar means the backing's evidentiary basis is unknown.
        if block.backing_claim.is_none() {
            return Err(ReviewTranslateError::SubstantiveChallengeMissingBacking {
                id: e.claim_id.clone(),
                category: block.category.clone(),
            });
        }
        if block.violation.is_none() {
            return Err(ReviewTranslateError::SubstantiveChallengeMissingViolation {
                id: e.claim_id.clone(),
                category: block.category.clone(),
            });
        }
        if block.target_criterion_id.is_none() {
            return Err(ReviewTranslateError::SubstantiveChallengeMissingTargetCriterion {
                id: e.claim_id.clone(),
                category: block.category.clone(),
            });
        }
    }

    let backed_by = match &block.backing_claim {
        Some(bc) => {
            if bc.id == e.claim_id {
                return Err(ReviewTranslateError::BackingClaimMatchesTargetId {
                    id: e.claim_id.clone(),
                });
            }
            Some(ClaimId::new(bc.id.clone()))
        }
        None => None,
    };

    Ok(ReviewKind::Challenge {
        category,
        backed_by,
    })
}

/// Helper for the CLI: pull the backing `ManifestClaim` out of a
/// challenge event so it can be translated and synthesized into a
/// backing TrustReport. Returns `None` for procedural Challenges (or
/// for non-Challenge events).
pub fn backing_claim_for_event(e: &ManifestReviewEvent) -> Option<&ManifestClaim> {
    e.challenge.as_ref().and_then(|b| b.backing_claim.as_ref())
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
    if let Some(ch) = &e.challenge {
        m.insert("challenge".into(), challenge_canonical_value(ch));
    }
    // Phase 2d-i: include target + supersede in the canonical hash
    // ONLY when present. Pre-2d sidecars without these fields
    // canonicalize to the exact same bytes as before (codex F-2D-5
    // parity discipline).
    if let Some(t) = &e.target {
        let mut tm = serde_json::Map::new();
        tm.insert("type".into(), Value::String(t.kind.clone()));
        tm.insert("id".into(), Value::String(t.id.clone()));
        m.insert("target".into(), Value::Object(tm));
    }
    if let Some(s) = &e.supersede {
        let mut sm = serde_json::Map::new();
        sm.insert("successor".into(), Value::String(s.successor.clone()));
        m.insert("supersede".into(), Value::Object(sm));
    }
    if let Some(p) = &e.protocol {
        m.insert("protocol".into(), Value::String(p.clone()));
    }

    Value::Object(m)
}

/// Canonical-JSON projection of the challenge block. Only the fields
/// that semantically distinguish two challenges go in — the backing
/// claim's `id` (because the agent derives it from the violation
/// tuple, so it's redundant for hashing purposes) is skipped to keep
/// the hash stable across backing-id-generation changes.
fn challenge_canonical_value(block: &ManifestChallengeBlock) -> serde_json::Value {
    use serde_json::{Map, Value};

    let mut m = Map::new();
    m.insert("category".into(), Value::String(block.category.clone()));
    if let Some(t) = &block.target_criterion_id {
        m.insert("target_criterion_id".into(), Value::String(t.clone()));
    }
    if let Some(v) = &block.violation {
        let mut vm = Map::new();
        vm.insert("metric".into(), Value::String(v.metric.clone()));
        vm.insert(
            "observed_value".into(),
            serde_json::Number::from_f64(v.observed_value)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
        vm.insert(
            "bound".into(),
            serde_json::Number::from_f64(v.bound)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
        vm.insert("comparator".into(), Value::String(v.comparator.clone()));
        vm.insert("citation".into(), Value::String(v.citation.clone()));
        m.insert("violation".into(), Value::Object(vm));
    }
    // backing_claim deliberately excluded from the canonical hash:
    // the agent generates its id from the violation tuple, so the
    // backing claim contributes no additional discriminating info.
    Value::Object(m)
}
