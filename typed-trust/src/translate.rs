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

use crate::claim::{Claim, ClaimKind, MetadataDeclaration, SourceSpan};
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
    /// PR5b: required when ``kind == "metadata_compatibility"``.
    /// Carries the declarative configuration claim — what field is
    /// being asserted, what value the source declares, and which
    /// config file the value came from. Absent for empirical
    /// (measurement) claims.
    #[serde(default)]
    pub metadata: Option<ManifestMetadataBlock>,
    /// PR5f: required when ``kind == "behavioral_concordance"``.
    /// Carries the pattern (numeric_band, relative_band, etc.) +
    /// paper_locator + prior_binding. Absent for any other kind;
    /// the translator rejects mixing.
    #[serde(default)]
    pub concordance: Option<ManifestConcordanceBlock>,
    /// PR5i: required when ``kind == "third_party_observation"``.
    /// Carries the pattern (same enum as concordance) +
    /// paper_locator + third_party_tool + metric_definition.
    /// Absent for any other kind.
    #[serde(default)]
    pub observation: Option<ManifestObservationBlock>,
}

/// PR5b: structured block for ``kind: metadata_compatibility``
/// claims. The declaration IS the evidence: the source's
/// pyproject.toml / Cargo.toml / package.json stated this value,
/// no synthesis or measurement required.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestMetadataBlock {
    /// Semantic name of the field being declared (e.g.
    /// ``python_version_requirement``, ``rust_msrv``,
    /// ``node_version_requirement``).
    pub field: String,
    /// The literal value the source declares (e.g. ``">=3.10"``,
    /// ``"1.67"``).
    pub declared_value: String,
    /// The config file the declaration came from
    /// (e.g. ``"pyproject.toml"``).
    pub source_file: String,
    /// The path within the config file where the value lives
    /// (e.g. ``"project.requires-python"`` for TOML, ``"engines.node"``
    /// for package.json).
    pub source_path: String,
}

/// PR5f: structured block for ``kind: behavioral_concordance``
/// claims.
///
/// The shape is a discriminated union on `pattern_kind`. Five
/// variants — `numeric_band`, `relative_band`,
/// `same_order_of_magnitude`, `ordinal_match`, `monotone_with`.
/// Codex v3 review insisted on discriminator dispatch (NOT a
/// Serde untagged union) so each pattern's required fields get
/// specific error messages at parse time, not vague "could not
/// match any variant" failures.
///
/// `deny_unknown_fields` at the variant level catches typos like
/// `prior_valu:` (instead of `prior_value:`) which would otherwise
/// silently drop the prior. Codex's same-PR pattern from PR5b's
/// `metadata` block.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestConcordanceBlock {
    /// The pattern variant + its typed fields. YAML:
    /// ``concordance.pattern.pattern_kind: numeric_band`` is the
    /// discriminator; the rest of the pattern's typed fields
    /// (`metric_path`, `epsilon`, `prior_value`, etc.) live under
    /// the same `pattern` block. Nesting (vs. `serde(flatten)`)
    /// is required because Serde's flatten doesn't compose with
    /// internally-tagged enums.
    pub pattern: ManifestConcordancePattern,
    /// Where in *this* paper the concordance claim is made. v4
    /// design: concordance claims do NOT carry the
    /// measurement-flavored top-level `source` field — they use
    /// `paper_locator` instead so a manifest never has to
    /// disambiguate "is this the paper-side or the prior-side
    /// citation."
    pub paper_locator: String,
    pub prior_binding: ManifestPriorBindingBlock,
}

/// PR5f: discriminator-dispatched pattern variants.
///
/// `pattern_kind` is the discriminator; each variant carries its
/// own typed parameters AND its own typed `prior_value` shape
/// (scalar for the three scalar primitives, per-entity map for
/// `ordinal_match`, absent for `monotone_with` whose prior is the
/// series shape not a value).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "pattern_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManifestConcordancePattern {
    NumericBand {
        metric_path: String,
        epsilon: f64,
        prior_value: f64,
    },
    RelativeBand {
        metric_path: String,
        ratio: f64,
        prior_value: f64,
    },
    SameOrderOfMagnitude {
        metric_path: String,
        #[serde(default = "default_zero_policy")]
        zero_policy: String,
        prior_value: f64,
    },
    OrdinalMatch {
        entity_to_path: std::collections::BTreeMap<String, String>,
        direction: String,
        #[serde(default = "default_tie_policy")]
        tie_policy: String,
        prior_value: std::collections::BTreeMap<String, f64>,
    },
    MonotoneWith {
        metric_path: String,
        parameter_path: String,
        direction: String,
    },
}

fn default_zero_policy() -> String {
    "not_assessed".into()
}

fn default_tie_policy() -> String {
    "strict".into()
}

/// PR5f: the curator-authored prior binding block. v4 design
/// makes the five fields required; they're not optional.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPriorBindingBlock {
    pub prior_unit: String,
    pub prior_metric_definition: String,
    pub locator: String,
    pub prior_extraction_note: String,
    pub source_id: String,
}

/// PR5i: structured block for ``kind: third_party_observation``
/// claims.
///
/// Mirrors `ManifestConcordanceBlock` structurally. Two
/// substantive differences at the YAML/serde boundary:
///
/// 1. The pattern's reference value is named `observed_value`,
///    NOT `prior_value` (v3 codex F-CR1: observation has no
///    "prior" — the paper itself is the source). The translator
///    maps `observed_value` → `prior_value` when lifting onto the
///    shared `ConcordancePattern` enum.
/// 2. The curator block carries `third_party_tool` +
///    `metric_definition` (lighter than concordance's
///    `prior_binding`) because there's no external paper to cite.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestObservationBlock {
    /// REQUIRED non-empty. The translator rejects empty values.
    pub third_party_tool: String,
    /// REQUIRED non-empty prose. Pins down what the metric IS.
    pub metric_definition: String,
    pub pattern: ManifestObservationPattern,
    /// REQUIRED non-empty. Where in this paper the observation
    /// is made.
    pub paper_locator: String,
}

/// PR5i: discriminator-dispatched observation pattern variants.
///
/// Identical to `ManifestConcordancePattern` except the
/// reference-value field is named `observed_value` (codex v2
/// F-CR1). `monotone_with` has no reference value at all (codex
/// v2 F-CR3: the structured fields direction/metric_path/
/// parameter_path carry the shape).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "pattern_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManifestObservationPattern {
    NumericBand {
        metric_path: String,
        epsilon: f64,
        observed_value: f64,
    },
    RelativeBand {
        metric_path: String,
        ratio: f64,
        observed_value: f64,
    },
    SameOrderOfMagnitude {
        metric_path: String,
        #[serde(default = "default_zero_policy")]
        zero_policy: String,
        observed_value: f64,
    },
    OrdinalMatch {
        entity_to_path: std::collections::BTreeMap<String, String>,
        direction: String,
        #[serde(default = "default_tie_policy")]
        tie_policy: String,
        observed_value: std::collections::BTreeMap<String, f64>,
    },
    MonotoneWith {
        metric_path: String,
        parameter_path: String,
        direction: String,
    },
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
    /// PR5i (codex v2 F-CR5): default-empty so
    /// `third_party_observation` and `behavioral_concordance`
    /// manifests can omit the field entirely (their pattern
    /// primitive IS the oracle). For `measurement` claims, the
    /// existing schema rule "evidence requires non-empty oracle"
    /// is RE-IMPOSED by `translate_evidence`, so the default
    /// here doesn't relax measurement.
    #[serde(default)]
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
    /// PR5j (codex review of PR5i): `kind: measurement` requires
    /// a non-empty `evidence.oracle` list. PR5i made the field
    /// `serde(default)` so observation manifests can omit it,
    /// which inadvertently allowed measurement claims to ship
    /// without declared oracles. This error re-imposes the
    /// check for measurement only.
    MeasurementWithoutOracle { id: String },
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
    /// PR5b: `kind: metadata_compatibility` claim missing the
    /// required `metadata` block.
    MetadataClaimMissingBlock { id: String },
    /// PR5b: metadata claims must NOT carry tolerances; they're
    /// declarative not empirical.
    MetadataClaimCarriesTolerances { id: String },
    /// PR5b: metadata claims must NOT carry an `evidence` block;
    /// the declaration IS the evidence (codex F-PR5b-CR1 P2).
    MetadataClaimCarriesEvidence { id: String },
    /// PR5b: measurement claims must NOT carry a metadata block.
    MeasurementClaimCarriesMetadata { id: String },
    /// PR5f: `kind: behavioral_concordance` claim missing the
    /// required `concordance` block.
    ConcordanceClaimMissingBlock { id: String },
    /// PR5f: concordance claims must NOT carry tolerances; the
    /// pattern primitive IS the bound.
    ConcordanceClaimCarriesTolerances { id: String },
    /// PR5f: concordance claims must NOT carry the
    /// measurement-flavored top-level `source` field; they carry
    /// `concordance.paper_locator` instead. v4 design's
    /// `paper_locator`-is-a-schema-exception commitment.
    ConcordanceClaimCarriesSource { id: String },
    /// PR5f: concordance evidence must NOT carry an `oracle`
    /// list. The pattern primitive IS the oracle.
    ConcordanceClaimCarriesOracle { id: String },
    /// PR5f: `ordinal_match` pattern requires that `prior_value`'s
    /// per-entity keyset exactly equals `entity_to_path`'s keyset.
    /// Codex v3 follow-up: keep the two structurally aligned at
    /// translate time so the comparator can dispatch unambiguously.
    ConcordanceOrdinalKeyMismatch { id: String },
    /// PR5f: `same_order_of_magnitude` requires a strictly
    /// positive `prior_value`. Non-positive prior is a curator
    /// authoring error caught at translate time, not at replay.
    ConcordanceSameOrderNonPositivePrior { id: String },
    /// PR5f: `relative_band` requires `ratio > 1.0`. A ratio of
    /// `1.0` would make the band a single point; a ratio of `<1.0`
    /// would invert the interpretation.
    ConcordanceRelativeBandRatioTooSmall { id: String },
    /// PR5f: measurement / metadata_compatibility claims must NOT
    /// carry a concordance block. Keeps the kinds disjoint.
    NonConcordanceClaimCarriesConcordance { id: String },
    /// PR5i: `kind: third_party_observation` claim missing the
    /// required `observation` block.
    ObservationClaimMissingBlock { id: String },
    /// PR5i: observation claims must NOT carry tolerances; the
    /// pattern primitive IS the bound (mirror of the concordance
    /// rule).
    ObservationClaimCarriesTolerances { id: String },
    /// PR5i: observation claims must NOT carry top-level
    /// `source`. Use `observation.paper_locator` instead.
    ObservationClaimCarriesSource { id: String },
    /// PR5i: observation claims must NOT carry the `case` field
    /// (codex v2 F-CR4). Use `observation.paper_locator`.
    ObservationClaimCarriesCase { id: String },
    /// PR5i: observation claims must NOT carry the
    /// `last_verified` block (codex v2 F-CR2). Observation uses
    /// `last_concorded.json`.
    ObservationClaimCarriesLastVerified { id: String },
    /// PR5i: observation claims must NOT carry a `metadata` or
    /// `concordance` block. Keeps the kinds disjoint.
    ObservationClaimCarriesMetadataOrConcordance { id: String },
    /// PR5i: observation evidence must NOT carry a non-empty
    /// `oracle` list. The pattern primitive IS the oracle.
    ObservationClaimCarriesOracle { id: String },
    /// PR5i: `observation.third_party_tool` must be non-empty.
    ObservationMissingThirdPartyTool { id: String },
    /// PR5i: `observation.metric_definition` must be non-empty.
    ObservationMissingMetricDefinition { id: String },
    /// PR5i: `observation.paper_locator` must be non-empty
    /// (codex v2 F-CR4).
    ObservationMissingPaperLocator { id: String },
    /// PR5i: `OrdinalMatch` observation requires
    /// `observed_value`'s per-entity keyset to equal
    /// `entity_to_path`'s keyset.
    ObservationOrdinalKeyMismatch { id: String },
    /// PR5i: `SameOrderOfMagnitude` observation requires
    /// `observed_value > 0`.
    ObservationSameOrderNonPositiveObserved { id: String },
    /// PR5i: `RelativeBand` observation requires `ratio > 1.0`.
    ObservationRelativeBandRatioTooSmall { id: String },
    /// PR5i: numeric sanity (codex v2 F-CR-bug-4) — `epsilon` must
    /// be finite and `> 0` for `NumericBand` observation.
    ObservationNumericBandEpsilonInvalid { id: String },
    /// PR5i: a non-finite f64 (NaN, Inf, -Inf) appeared in an
    /// observation pattern field. Bug-class-4 from codex v2 review.
    ObservationNonFiniteValue { id: String, field: &'static str },
    /// PR5i: a measurement / metadata_compatibility /
    /// behavioral_concordance claim accidentally carries an
    /// `observation` block. Keeps the kinds disjoint.
    NonObservationClaimCarriesObservation { id: String },
    /// PR5f: `concordance.pattern.{enum_field}` carried an unknown
    /// enum value (e.g. `direction: "sideways"`,
    /// `tie_policy: "everything_goes"`, `zero_policy: "ignore"`).
    ConcordanceInvalidEnumValue {
        id: String,
        field: String,
        value: String,
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
            TranslateError::MeasurementWithoutOracle { id } => write!(
                f,
                "claim {id}: kind=measurement requires a non-empty \
                 `evidence.oracle` list (PR5j re-imposed this check \
                 after PR5i's serde(default) relaxation made it possible \
                 to ship measurement claims without declared oracles)"
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
            TranslateError::MetadataClaimMissingBlock { id } => write!(
                f,
                "claim {id}: kind=metadata_compatibility requires a \
                 `metadata` block with field/declared_value/source_file/\
                 source_path"
            ),
            TranslateError::MetadataClaimCarriesTolerances { id } => write!(
                f,
                "claim {id}: kind=metadata_compatibility must NOT carry \
                 tolerances; metadata is declarative, not empirical"
            ),
            TranslateError::MetadataClaimCarriesEvidence { id } => write!(
                f,
                "claim {id}: kind=metadata_compatibility must NOT carry \
                 an `evidence` block; the declaration IS the evidence"
            ),
            TranslateError::MeasurementClaimCarriesMetadata { id } => write!(
                f,
                "claim {id}: kind=measurement must NOT carry a `metadata` \
                 block; metadata belongs only to metadata_compatibility claims"
            ),
            TranslateError::ConcordanceClaimMissingBlock { id } => write!(
                f,
                "claim {id}: kind=behavioral_concordance requires a \
                 `concordance` block (pattern_kind + paper_locator + \
                 prior_binding)"
            ),
            TranslateError::ConcordanceClaimCarriesTolerances { id } => write!(
                f,
                "claim {id}: kind=behavioral_concordance must NOT carry \
                 `tolerances`; the pattern primitive is the bound"
            ),
            TranslateError::ConcordanceClaimCarriesSource { id } => write!(
                f,
                "claim {id}: kind=behavioral_concordance must NOT carry \
                 the top-level `source` field; use \
                 `concordance.paper_locator` instead (v4 design's \
                 schema-exception commitment)"
            ),
            TranslateError::ConcordanceClaimCarriesOracle { id } => write!(
                f,
                "claim {id}: kind=behavioral_concordance evidence must \
                 NOT carry an `oracle` list; the pattern primitive \
                 (numeric_band, ordinal_match, etc.) IS the oracle"
            ),
            TranslateError::ConcordanceOrdinalKeyMismatch { id } => write!(
                f,
                "claim {id}: ordinal_match concordance requires \
                 `prior_value`'s per-entity keyset to exactly equal \
                 `entity_to_path`'s keyset"
            ),
            TranslateError::ConcordanceSameOrderNonPositivePrior { id } => write!(
                f,
                "claim {id}: same_order_of_magnitude requires a strictly \
                 positive `prior_value` (log10 is undefined at zero \
                 and semantically wrong for signed quantities)"
            ),
            TranslateError::ConcordanceRelativeBandRatioTooSmall { id } => write!(
                f,
                "claim {id}: relative_band requires `ratio > 1.0` \
                 (a ratio of 1.0 collapses the band to a point; \
                 a ratio < 1.0 inverts the bound interpretation)"
            ),
            TranslateError::NonConcordanceClaimCarriesConcordance { id } => write!(
                f,
                "claim {id}: only kind=behavioral_concordance may carry \
                 a `concordance` block"
            ),
            TranslateError::ConcordanceInvalidEnumValue {
                id,
                field,
                value,
            } => write!(
                f,
                "claim {id}: concordance.{field} carries unknown value \
                 {value:?}; see EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md \
                 for the legal set"
            ),
            TranslateError::ObservationClaimMissingBlock { id } => write!(
                f,
                "claim {id}: kind=third_party_observation requires an \
                 `observation` block (pattern + third_party_tool + \
                 metric_definition + paper_locator)"
            ),
            TranslateError::ObservationClaimCarriesTolerances { id } => write!(
                f,
                "claim {id}: kind=third_party_observation must NOT carry \
                 `tolerances`; the pattern primitive is the bound"
            ),
            TranslateError::ObservationClaimCarriesSource { id } => write!(
                f,
                "claim {id}: kind=third_party_observation must NOT carry \
                 the top-level `source` field; use \
                 `observation.paper_locator` instead"
            ),
            TranslateError::ObservationClaimCarriesCase { id } => write!(
                f,
                "claim {id}: kind=third_party_observation must NOT carry \
                 a `case` field; use `observation.paper_locator` instead"
            ),
            TranslateError::ObservationClaimCarriesLastVerified { id } => write!(
                f,
                "claim {id}: kind=third_party_observation must NOT carry \
                 a `last_verified` block; observation results are read \
                 from `last_concorded.json`"
            ),
            TranslateError::ObservationClaimCarriesMetadataOrConcordance { id } => write!(
                f,
                "claim {id}: kind=third_party_observation must NOT carry \
                 a `metadata` or `concordance` block; the claim kinds \
                 are disjoint"
            ),
            TranslateError::ObservationClaimCarriesOracle { id } => write!(
                f,
                "claim {id}: kind=third_party_observation evidence must \
                 NOT carry a non-empty `oracle` list; the pattern \
                 primitive (numeric_band, ordinal_match, etc.) IS the \
                 oracle"
            ),
            TranslateError::ObservationMissingThirdPartyTool { id } => write!(
                f,
                "claim {id}: observation.third_party_tool must be non-empty"
            ),
            TranslateError::ObservationMissingMetricDefinition { id } => write!(
                f,
                "claim {id}: observation.metric_definition must be non-empty"
            ),
            TranslateError::ObservationMissingPaperLocator { id } => write!(
                f,
                "claim {id}: observation.paper_locator must be non-empty"
            ),
            TranslateError::ObservationOrdinalKeyMismatch { id } => write!(
                f,
                "claim {id}: ordinal_match observation requires \
                 `observed_value`'s per-entity keyset to exactly equal \
                 `entity_to_path`'s keyset"
            ),
            TranslateError::ObservationSameOrderNonPositiveObserved { id } => write!(
                f,
                "claim {id}: same_order_of_magnitude observation requires \
                 a strictly positive `observed_value`"
            ),
            TranslateError::ObservationRelativeBandRatioTooSmall { id } => write!(
                f,
                "claim {id}: relative_band observation requires \
                 `ratio > 1.0`"
            ),
            TranslateError::ObservationNumericBandEpsilonInvalid { id } => write!(
                f,
                "claim {id}: numeric_band observation requires \
                 `epsilon > 0` and finite"
            ),
            TranslateError::ObservationNonFiniteValue { id, field } => write!(
                f,
                "claim {id}: observation field {field} must be a finite \
                 number (no NaN, Inf, -Inf)"
            ),
            TranslateError::NonObservationClaimCarriesObservation { id } => write!(
                f,
                "claim {id}: only kind=third_party_observation may carry \
                 an `observation` block"
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
    // §0 scope: measurement claims (empirical), metadata_compatibility
    // claims (PR5b — declarative configuration claims), or
    // behavioral_concordance claims (PR5f — paper measured-behavior
    // tracks a prior paper's reported behavior).
    if mc.kind != "measurement"
        && mc.kind != "metadata_compatibility"
        && mc.kind != "behavioral_concordance"
        && mc.kind != "third_party_observation"
    {
        return Err(TranslateError::OutOfScope {
            id: mc.id.clone(),
            kind: mc.kind.clone(),
        });
    }

    // PR5b: metadata_compatibility claims require the `metadata`
    // block (field/declared_value/source_file/source_path) and must
    // NOT carry tolerances/evidence — those belong to the empirical
    // path.
    if mc.kind == "metadata_compatibility" {
        if mc.metadata.is_none() {
            return Err(TranslateError::MetadataClaimMissingBlock {
                id: mc.id.clone(),
            });
        }
        if mc.tolerances.is_some() {
            return Err(TranslateError::MetadataClaimCarriesTolerances {
                id: mc.id.clone(),
            });
        }
        // Codex F-PR5b-CR1 (P2): also reject `evidence:` on a
        // metadata claim. The two paths are disjoint; the
        // declaration IS the evidence.
        if mc.evidence.is_some() {
            return Err(TranslateError::MetadataClaimCarriesEvidence {
                id: mc.id.clone(),
            });
        }
    } else if mc.metadata.is_some() {
        // A measurement OR concordance claim that accidentally
        // carries a metadata block is rejected — keeps the paths
        // disjoint.
        return Err(TranslateError::MeasurementClaimCarriesMetadata {
            id: mc.id.clone(),
        });
    }

    // PR5f: behavioral_concordance claims require the `concordance`
    // block and must NOT carry `tolerances` (the comparator
    // primitive IS the bound) or the `oracle` list inside evidence
    // (the comparator IS the oracle). They DO carry `evidence`
    // for the docker contract (docker_image, command, artifact).
    if mc.kind == "behavioral_concordance" {
        if mc.concordance.is_none() {
            return Err(TranslateError::ConcordanceClaimMissingBlock {
                id: mc.id.clone(),
            });
        }
        if mc.tolerances.is_some() {
            return Err(TranslateError::ConcordanceClaimCarriesTolerances {
                id: mc.id.clone(),
            });
        }
        // Concordance claims don't use the measurement-flavored
        // top-level `source` field — they carry
        // `concordance.paper_locator` instead. v4 design's
        // "paper_locator is a schema exception" commitment.
        if mc.source.is_some() {
            return Err(TranslateError::ConcordanceClaimCarriesSource {
                id: mc.id.clone(),
            });
        }
        // The `oracle` list is a measurement-evidence concept;
        // for concordance the comparator (pattern_kind) IS the
        // oracle. Reject to make the disjointness load-bearing.
        if let Some(ev) = mc.evidence.as_ref() {
            if !ev.oracle.is_empty() {
                return Err(TranslateError::ConcordanceClaimCarriesOracle {
                    id: mc.id.clone(),
                });
            }
        }
        // Validate the OrdinalMatch keyset invariant: the prior's
        // per-entity map keyset MUST equal entity_to_path's
        // keyset. v4 design commitment; codex v3 finding.
        if let Some(cb) = mc.concordance.as_ref() {
            if let ManifestConcordancePattern::OrdinalMatch {
                entity_to_path,
                prior_value,
                ..
            } = &cb.pattern
            {
                let path_keys: std::collections::BTreeSet<&String> =
                    entity_to_path.keys().collect();
                let prior_keys: std::collections::BTreeSet<&String> =
                    prior_value.keys().collect();
                if path_keys != prior_keys {
                    return Err(TranslateError::ConcordanceOrdinalKeyMismatch {
                        id: mc.id.clone(),
                    });
                }
            }
            // SameOrderOfMagnitude: prior_value > 0 is a curator
            // authoring invariant per v4 design.
            if let ManifestConcordancePattern::SameOrderOfMagnitude { prior_value, .. } =
                &cb.pattern
            {
                if *prior_value <= 0.0 {
                    return Err(TranslateError::ConcordanceSameOrderNonPositivePrior {
                        id: mc.id.clone(),
                    });
                }
            }
            // RelativeBand: ratio > 1.0 per v4 design.
            if let ManifestConcordancePattern::RelativeBand { ratio, .. } = &cb.pattern {
                if *ratio <= 1.0 {
                    return Err(TranslateError::ConcordanceRelativeBandRatioTooSmall {
                        id: mc.id.clone(),
                    });
                }
            }
        }
    } else if mc.concordance.is_some() {
        // A measurement / metadata_compatibility / observation
        // claim that accidentally carries a concordance block is
        // rejected.
        return Err(TranslateError::NonConcordanceClaimCarriesConcordance {
            id: mc.id.clone(),
        });
    }

    // PR5i: third_party_observation invariants.
    if mc.kind == "third_party_observation" {
        if mc.observation.is_none() {
            return Err(TranslateError::ObservationClaimMissingBlock {
                id: mc.id.clone(),
            });
        }
        if mc.tolerances.is_some() {
            return Err(TranslateError::ObservationClaimCarriesTolerances {
                id: mc.id.clone(),
            });
        }
        if mc.source.is_some() {
            return Err(TranslateError::ObservationClaimCarriesSource {
                id: mc.id.clone(),
            });
        }
        if mc.case.is_some() {
            return Err(TranslateError::ObservationClaimCarriesCase {
                id: mc.id.clone(),
            });
        }
        if mc.last_verified.is_some() {
            return Err(TranslateError::ObservationClaimCarriesLastVerified {
                id: mc.id.clone(),
            });
        }
        if mc.metadata.is_some() || mc.concordance.is_some() {
            return Err(TranslateError::ObservationClaimCarriesMetadataOrConcordance {
                id: mc.id.clone(),
            });
        }
        if let Some(ev) = mc.evidence.as_ref() {
            if !ev.oracle.is_empty() {
                return Err(TranslateError::ObservationClaimCarriesOracle {
                    id: mc.id.clone(),
                });
            }
        }
    } else if mc.observation.is_some() {
        return Err(TranslateError::NonObservationClaimCarriesObservation {
            id: mc.id.clone(),
        });
    }

    let kind = if mc.kind == "metadata_compatibility" {
        ClaimKind::MetadataCompatibility
    } else if mc.kind == "behavioral_concordance" {
        ClaimKind::BehavioralConcordance
    } else if mc.kind == "third_party_observation" {
        ClaimKind::ThirdPartyObservation
    } else {
        infer_kind(mc)
    };

    let metadata = mc.metadata.as_ref().map(|m| MetadataDeclaration {
        field: m.field.clone(),
        declared_value: m.declared_value.clone(),
        source_file: m.source_file.clone(),
        source_path: m.source_path.clone(),
    });

    let concordance = mc.concordance.as_ref().map(translate_concordance_block).transpose()?;
    let observation = mc.observation.as_ref().map(|ob| translate_observation_block(&mc.id, ob)).transpose()?;

    let claim = Claim {
        id: ClaimId::new(&mc.id),
        text: mc.claim.trim().to_string(),
        kind,
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
        metadata,
        observation,
        concordance,
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

/// PR5f: lift the manifest's `concordance` block onto the typed
/// `ConcordanceDeclaration`. Parses the enum-valued fields
/// (`direction`, `tie_policy`, `zero_policy`) into their typed
/// counterparts and emits a structured `TranslateError` if an
/// unknown value is encountered.
fn translate_concordance_block(
    mb: &ManifestConcordanceBlock,
) -> Result<crate::claim::ConcordanceDeclaration, TranslateError> {
    use crate::claim::{
        ConcordanceDeclaration, ConcordancePattern, MonotoneDirection, PriorBindingContext,
        RankingDirection, TiePolicy, ZeroPolicy,
    };

    fn parse_zero_policy(s: &str) -> Option<ZeroPolicy> {
        match s {
            "reject" => Some(ZeroPolicy::Reject),
            "not_assessed" => Some(ZeroPolicy::NotAssessed),
            _ => None,
        }
    }
    fn parse_ranking_direction(s: &str) -> Option<RankingDirection> {
        match s {
            "lower_is_better" => Some(RankingDirection::LowerIsBetter),
            "higher_is_better" => Some(RankingDirection::HigherIsBetter),
            _ => None,
        }
    }
    fn parse_tie_policy(s: &str) -> Option<TiePolicy> {
        match s {
            "strict" => Some(TiePolicy::Strict),
            "adjacent_swap_ok" => Some(TiePolicy::AdjacentSwapOk),
            _ => None,
        }
    }
    fn parse_monotone_direction(s: &str) -> Option<MonotoneDirection> {
        match s {
            "increasing" => Some(MonotoneDirection::Increasing),
            "decreasing" => Some(MonotoneDirection::Decreasing),
            _ => None,
        }
    }

    let pattern = match &mb.pattern {
        ManifestConcordancePattern::NumericBand {
            metric_path,
            epsilon,
            prior_value,
        } => ConcordancePattern::NumericBand {
            metric_path: metric_path.clone(),
            epsilon: *epsilon,
            prior_value: *prior_value,
        },
        ManifestConcordancePattern::RelativeBand {
            metric_path,
            ratio,
            prior_value,
        } => ConcordancePattern::RelativeBand {
            metric_path: metric_path.clone(),
            ratio: *ratio,
            prior_value: *prior_value,
        },
        ManifestConcordancePattern::SameOrderOfMagnitude {
            metric_path,
            zero_policy,
            prior_value,
        } => ConcordancePattern::SameOrderOfMagnitude {
            metric_path: metric_path.clone(),
            zero_policy: parse_zero_policy(zero_policy).ok_or_else(|| {
                TranslateError::ConcordanceInvalidEnumValue {
                    id: "<unknown>".into(),
                    field: "pattern.zero_policy".into(),
                    value: zero_policy.clone(),
                }
            })?,
            prior_value: *prior_value,
        },
        ManifestConcordancePattern::OrdinalMatch {
            entity_to_path,
            direction,
            tie_policy,
            prior_value,
        } => ConcordancePattern::OrdinalMatch {
            entity_to_path: entity_to_path.clone(),
            direction: parse_ranking_direction(direction).ok_or_else(|| {
                TranslateError::ConcordanceInvalidEnumValue {
                    id: "<unknown>".into(),
                    field: "pattern.direction".into(),
                    value: direction.clone(),
                }
            })?,
            tie_policy: parse_tie_policy(tie_policy).ok_or_else(|| {
                TranslateError::ConcordanceInvalidEnumValue {
                    id: "<unknown>".into(),
                    field: "pattern.tie_policy".into(),
                    value: tie_policy.clone(),
                }
            })?,
            prior_value: prior_value.clone(),
        },
        ManifestConcordancePattern::MonotoneWith {
            metric_path,
            parameter_path,
            direction,
        } => ConcordancePattern::MonotoneWith {
            metric_path: metric_path.clone(),
            parameter_path: parameter_path.clone(),
            direction: parse_monotone_direction(direction).ok_or_else(|| {
                TranslateError::ConcordanceInvalidEnumValue {
                    id: "<unknown>".into(),
                    field: "pattern.direction".into(),
                    value: direction.clone(),
                }
            })?,
        },
    };

    Ok(ConcordanceDeclaration {
        pattern,
        paper_locator: mb.paper_locator.clone(),
        prior_binding: PriorBindingContext {
            prior_unit: mb.prior_binding.prior_unit.clone(),
            prior_metric_definition: mb.prior_binding.prior_metric_definition.clone(),
            locator: mb.prior_binding.locator.clone(),
            prior_extraction_note: mb.prior_binding.prior_extraction_note.clone(),
            source_id: mb.prior_binding.source_id.clone(),
        },
    })
}

/// PR5i: lift the manifest's `observation` block onto the typed
/// `ObservationDeclaration`.
///
/// Reuses `ConcordancePattern` for the pattern: each variant's
/// `observed_value` field on the manifest side maps onto the
/// internal enum's `prior_value` field. The translator does the
/// rename here so the comparator + render layers don't need to
/// know the naming difference.
///
/// All numeric validations run here (codex v2 F-CR-bug-4): finite
/// floats, `epsilon > 0`, `ratio > 1.0`, `prior_value > 0` for
/// `same_order_of_magnitude`, keyset alignment for
/// `ordinal_match`.
fn translate_observation_block(
    claim_id: &str,
    ob: &ManifestObservationBlock,
) -> Result<crate::claim::ObservationDeclaration, TranslateError> {
    use crate::claim::{
        ConcordancePattern, MonotoneDirection, ObservationDeclaration, RankingDirection,
        TiePolicy, ZeroPolicy,
    };

    fn check_finite(v: f64, id: &str, field: &'static str) -> Result<(), TranslateError> {
        if !v.is_finite() {
            Err(TranslateError::ObservationNonFiniteValue {
                id: id.into(),
                field,
            })
        } else {
            Ok(())
        }
    }

    if ob.third_party_tool.trim().is_empty() {
        return Err(TranslateError::ObservationMissingThirdPartyTool {
            id: claim_id.into(),
        });
    }
    if ob.metric_definition.trim().is_empty() {
        return Err(TranslateError::ObservationMissingMetricDefinition {
            id: claim_id.into(),
        });
    }
    if ob.paper_locator.trim().is_empty() {
        return Err(TranslateError::ObservationMissingPaperLocator {
            id: claim_id.into(),
        });
    }

    fn parse_zero_policy(s: &str, id: &str) -> Result<ZeroPolicy, TranslateError> {
        match s {
            "reject" => Ok(ZeroPolicy::Reject),
            "not_assessed" => Ok(ZeroPolicy::NotAssessed),
            _ => Err(TranslateError::ConcordanceInvalidEnumValue {
                id: id.into(),
                field: "pattern.zero_policy".into(),
                value: s.into(),
            }),
        }
    }
    fn parse_ranking_direction(s: &str, id: &str) -> Result<RankingDirection, TranslateError> {
        match s {
            "lower_is_better" => Ok(RankingDirection::LowerIsBetter),
            "higher_is_better" => Ok(RankingDirection::HigherIsBetter),
            _ => Err(TranslateError::ConcordanceInvalidEnumValue {
                id: id.into(),
                field: "pattern.direction".into(),
                value: s.into(),
            }),
        }
    }
    fn parse_tie_policy(s: &str, id: &str) -> Result<TiePolicy, TranslateError> {
        match s {
            "strict" => Ok(TiePolicy::Strict),
            "adjacent_swap_ok" => Ok(TiePolicy::AdjacentSwapOk),
            _ => Err(TranslateError::ConcordanceInvalidEnumValue {
                id: id.into(),
                field: "pattern.tie_policy".into(),
                value: s.into(),
            }),
        }
    }
    fn parse_monotone_direction(s: &str, id: &str) -> Result<MonotoneDirection, TranslateError> {
        match s {
            "increasing" => Ok(MonotoneDirection::Increasing),
            "decreasing" => Ok(MonotoneDirection::Decreasing),
            _ => Err(TranslateError::ConcordanceInvalidEnumValue {
                id: id.into(),
                field: "pattern.direction".into(),
                value: s.into(),
            }),
        }
    }

    let pattern = match &ob.pattern {
        ManifestObservationPattern::NumericBand {
            metric_path,
            epsilon,
            observed_value,
        } => {
            check_finite(*epsilon, claim_id, "pattern.epsilon")?;
            check_finite(*observed_value, claim_id, "pattern.observed_value")?;
            if *epsilon <= 0.0 {
                return Err(TranslateError::ObservationNumericBandEpsilonInvalid {
                    id: claim_id.into(),
                });
            }
            ConcordancePattern::NumericBand {
                metric_path: metric_path.clone(),
                epsilon: *epsilon,
                prior_value: *observed_value,
            }
        }
        ManifestObservationPattern::RelativeBand {
            metric_path,
            ratio,
            observed_value,
        } => {
            check_finite(*ratio, claim_id, "pattern.ratio")?;
            check_finite(*observed_value, claim_id, "pattern.observed_value")?;
            if *ratio <= 1.0 {
                return Err(TranslateError::ObservationRelativeBandRatioTooSmall {
                    id: claim_id.into(),
                });
            }
            ConcordancePattern::RelativeBand {
                metric_path: metric_path.clone(),
                ratio: *ratio,
                prior_value: *observed_value,
            }
        }
        ManifestObservationPattern::SameOrderOfMagnitude {
            metric_path,
            zero_policy,
            observed_value,
        } => {
            check_finite(*observed_value, claim_id, "pattern.observed_value")?;
            if *observed_value <= 0.0 {
                return Err(TranslateError::ObservationSameOrderNonPositiveObserved {
                    id: claim_id.into(),
                });
            }
            ConcordancePattern::SameOrderOfMagnitude {
                metric_path: metric_path.clone(),
                zero_policy: parse_zero_policy(zero_policy, claim_id)?,
                prior_value: *observed_value,
            }
        }
        ManifestObservationPattern::OrdinalMatch {
            entity_to_path,
            direction,
            tie_policy,
            observed_value,
        } => {
            let path_keys: std::collections::BTreeSet<&String> =
                entity_to_path.keys().collect();
            let observed_keys: std::collections::BTreeSet<&String> =
                observed_value.keys().collect();
            if path_keys != observed_keys {
                return Err(TranslateError::ObservationOrdinalKeyMismatch {
                    id: claim_id.into(),
                });
            }
            for (k, v) in observed_value {
                check_finite(*v, claim_id, "pattern.observed_value[*]")
                    .map_err(|_| TranslateError::ObservationNonFiniteValue {
                        id: claim_id.into(),
                        field: "pattern.observed_value[entity]",
                    })?;
                let _ = k;
            }
            ConcordancePattern::OrdinalMatch {
                entity_to_path: entity_to_path.clone(),
                direction: parse_ranking_direction(direction, claim_id)?,
                tie_policy: parse_tie_policy(tie_policy, claim_id)?,
                prior_value: observed_value.clone(),
            }
        }
        ManifestObservationPattern::MonotoneWith {
            metric_path,
            parameter_path,
            direction,
        } => ConcordancePattern::MonotoneWith {
            metric_path: metric_path.clone(),
            parameter_path: parameter_path.clone(),
            direction: parse_monotone_direction(direction, claim_id)?,
        },
    };

    Ok(ObservationDeclaration {
        third_party_tool: ob.third_party_tool.clone(),
        metric_definition: ob.metric_definition.clone(),
        pattern,
        paper_locator: ob.paper_locator.clone(),
    })
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
    // PR5j (codex review of PR5i): measurement claims require a
    // non-empty `evidence.oracle` list. PR5i made the field
    // `serde(default)` so observation manifests can omit it
    // (their pattern primitive IS the oracle), which inadvertently
    // relaxed the measurement schema. Re-impose the check here.
    if mc.kind == "measurement" && me.oracle.is_empty() {
        return Err(TranslateError::MeasurementWithoutOracle {
            id: mc.id.clone(),
        });
    }
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
