//! Claim — see §5 of `concepts/typed-trust.md`.
//!
//! Propositional content only. Review actions (Endorse, Dissent,
//! Challenge, Supersede) live in `ReviewEvent`, not as Claim variants.

use crate::derivation::Attested;
use crate::ids::ClaimId;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Claim {
    pub id: ClaimId,
    pub text: String,
    pub kind: ClaimKind,
    pub source: SourceSpan,
    /// Stated verbatim in the source vs. inferred.
    pub explicit: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub decomposes_into: Vec<ClaimId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires_assumptions: Vec<Attested<Assumption>>,
    /// PR5c: declarative configuration claim from a config file.
    /// Only `Some` when `kind == MetadataCompatibility`; the typed
    /// declaration carries field name, declared value, and which
    /// config file/path it came from. Renderers surface this in
    /// place of the (missing) criteria section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataDeclaration>,
    /// PR5f: behavioral concordance — paper claims its measured
    /// behavior tracks a prior paper's reported behavior. Only
    /// `Some` when `kind == BehavioralConcordance`. Replaces the
    /// `source: SourceSpan` plumbing for this kind: concordance
    /// claims do NOT carry the measurement-flavored `source`
    /// field at the schema layer (see
    /// `EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md` v4
    /// "paper_locator is a schema exception").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concordance: Option<ConcordanceDeclaration>,
}

/// PR5f: typed lift of the manifest's `concordance:` block.
///
/// A concordance claim asserts: "my measured behavior at
/// `metric_path` (in the docker artifact) tracks the prior value
/// curated into `prior_binding`, under the relationship declared
/// by `pattern`."
///
/// The framework owns the relationship vocabulary (the
/// `ConcordancePattern` enum + its per-variant parameters and
/// prior shapes). The curator owns the prior binding (transcribing
/// the prior paper's value, unit, metric definition, and
/// extraction provenance). The docker artifact owns the measured
/// value. See the draft for the full layer split.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ConcordanceDeclaration {
    /// The framework-owned pattern: which relationship is being
    /// asserted between the measured value and the prior. Carries
    /// the per-variant parameters and the typed `prior_value` so
    /// every variant has a structurally complete shape.
    pub pattern: ConcordancePattern,
    /// Where in *this* paper the concordance claim is made.
    /// Replaces the measurement-flavored top-level `source` field
    /// for this kind (avoids the "is `source` the rustims-side
    /// or Meier-side citation?" ambiguity).
    pub paper_locator: String,
    /// Curator-authored prior binding: the prose + locator + audit
    /// fields that pin down what the prior actually says. Required
    /// for every concordance claim. `prior_value` lives inside
    /// `pattern` (single source of truth, pattern-typed); this
    /// block carries the human-facing context.
    pub prior_binding: PriorBindingContext,
}

/// PR5f: per-pattern typed shape carrying both the pattern's
/// parameters and its typed prior value. Discriminator-dispatched
/// deserialization (parse `pattern_kind` first, then validate the
/// variant's required fields) — NOT a Serde untagged union, to
/// keep error messages specific (per codex v3 review).
///
/// Variant naming follows the schema's snake_case
/// (`numeric_band`, etc.); see `serde` tag below.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "pattern_kind", rename_all = "snake_case")]
pub enum ConcordancePattern {
    /// Measured value must lie within
    /// `[prior_value - epsilon, prior_value + epsilon]`.
    /// Use when the paper cites a single numeric figure with an
    /// absolute tolerance band (e.g. "FDR within ±0.5 pp of
    /// Meier 2024 Table 3").
    NumericBand {
        /// Top-level metric_path (where in the docker artifact
        /// to read the measured value).
        metric_path: String,
        epsilon: f64,
        /// Pattern-typed prior: scalar.
        prior_value: f64,
    },
    /// Measured value must lie within
    /// `[prior_value / ratio, prior_value * ratio]`.
    /// Use for multiplicative bands ("runtime within 2× of
    /// baseline"). `ratio` must be > 1.0.
    RelativeBand {
        metric_path: String,
        ratio: f64,
        prior_value: f64,
    },
    /// `floor(log10(measured)) == floor(log10(prior_value))`.
    /// Use when the magnitude band is what's load-bearing, not the
    /// exact value. Restricted to strictly positive metrics;
    /// `zero_policy` governs what happens when the measured value
    /// is non-positive.
    SameOrderOfMagnitude {
        metric_path: String,
        /// Behavior when the measured value is `<= 0`.
        /// `prior_value <= 0` is a curator authoring error caught
        /// at translate time.
        zero_policy: ZeroPolicy,
        /// Pattern-typed prior: strictly positive scalar
        /// (validated at translate time).
        prior_value: f64,
    },
    /// The ranking of entities (by their measured values under
    /// `direction`) must match the ranking implied by
    /// `prior_value` (a per-entity prior map). Unlike the other
    /// primitives, `ordinal_match` does NOT carry a top-level
    /// `metric_path` — each entity's measured value resolves via
    /// `entity_to_path` to its own artifact location.
    ///
    /// Translate-time validator: `prior_value`'s keyset MUST
    /// equal `entity_to_path`'s keyset.
    OrdinalMatch {
        /// Explicit per-entity artifact paths. Replaces v2's
        /// implicit `{entity}` substitution (codex v3 fix).
        entity_to_path: std::collections::BTreeMap<String, String>,
        direction: RankingDirection,
        tie_policy: TiePolicy,
        /// Pattern-typed prior: per-entity map. Keyset MUST match
        /// `entity_to_path`.
        prior_value: std::collections::BTreeMap<String, f64>,
    },
    /// The measured series at `metric_path`, when sorted by the
    /// paired parameter series at `parameter_path`, is monotone
    /// in `direction`. No prior numeric value — the prior is the
    /// *shape* of the series, captured in
    /// `PriorBindingContext.prior_metric_definition`.
    MonotoneWith {
        metric_path: String,
        parameter_path: String,
        direction: MonotoneDirection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZeroPolicy {
    /// Treat a non-positive measured value as a translate-time
    /// rejection (the artifact is malformed for the declared
    /// claim).
    Reject,
    /// Treat a non-positive measured value as `NotAssessed`
    /// (replay ran but the comparison can't be made; the criterion
    /// stays unassessed rather than failing).
    NotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingDirection {
    LowerIsBetter,
    HigherIsBetter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TiePolicy {
    /// Any ties between adjacent entities fail the comparison.
    Strict,
    /// A single adjacent-pair swap relative to `prior_value`'s
    /// ranking is tolerated (the common "within noise" case).
    AdjacentSwapOk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MonotoneDirection {
    Increasing,
    Decreasing,
}

/// PR5h: typed lift of a `last_concorded.json` entry.
///
/// The agent's comparator (`evident_agent.concordance`) produces
/// a `ConcordanceResult` per concordance claim and writes it to
/// `last_concorded.json`. typed-trust reads it back here. The
/// shape mirrors the Python `LastConcordedEntry` exactly so the
/// two layers round-trip without translation.
///
/// `comparison_status` is the load-bearing discriminator the
/// framework reads for status synthesis. Pattern-specific fields
/// (`observed_ordering` / `observed_series` etc.) round-trip
/// through but the synthesizer doesn't interpret them; they're
/// audit material for render.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConcordanceResult {
    /// `"pass" | "fail" | "not_assessed"` — the comparator's
    /// verdict. Kept as a typed enum so the synthesizer can
    /// dispatch without string-matching.
    pub comparison_status: ComparisonStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_ordering: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_ordering: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_series: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_series: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produced_at: Option<String>,
    /// Free-form diagnostics block from the comparator (e.g.
    /// `{"delta_from_prior": 0.1, "within_band": true}`).
    /// Preserved verbatim through serde so the rendered output
    /// can surface whatever the comparator chose to record.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub diagnostics: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonStatus {
    Pass,
    Fail,
    NotAssessed,
}

/// PR5f: curator-authored prior binding context.
///
/// The five required fields pin down what the prior paper
/// actually says — without this, a curator who reads "1.5%" from
/// Meier's Table 3 when the actual cell says "1.4%" is an error
/// the framework cannot catch. The audit trail is structural,
/// not prose-only.
///
/// `prior_value` is NOT here (single source of truth: lives on
/// the typed `ConcordancePattern`, pattern-typed for the variant).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PriorBindingContext {
    /// Unit of the prior value (e.g. `percentage_points`,
    /// `seconds`). Unit-mismatch is the #1 silent concordance
    /// failure; required even for `MonotoneWith` (describes the
    /// series' unit).
    pub prior_unit: String,
    /// Multi-sentence prose describing precisely what the prior
    /// metric IS — denominator, preprocessing, what's excluded.
    /// "FDR" means different things in different papers; the
    /// curator pins down which.
    pub prior_metric_definition: String,
    /// Where in the prior paper the value lives
    /// (e.g. "Meier 2024 Table 3 row 'FragPipe v22 / HLA-I 10k
    /// measured', column 'true_fdr_pct'").
    pub locator: String,
    /// Curator's audit trail: who extracted, when, what version
    /// of the prior they read, what edge-case checks they ran
    /// (caption confirms units, supplementary figure cross-check,
    /// etc.).
    pub prior_extraction_note: String,
    /// The cited artifact. Either DOI, arXiv id, or another
    /// `source_id` token (analogous to manifest provenance).
    pub source_id: String,
}

/// PR5c: typed lift of the manifest's `metadata:` block. The
/// declaration IS the evidence — the source's
/// `pyproject.toml`/`Cargo.toml`/`package.json` stated this value,
/// no synthesis required.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MetadataDeclaration {
    /// Semantic name of the field (e.g. `rust_msrv`,
    /// `python_version_requirement`).
    pub field: String,
    /// Literal value the source declares (e.g. `">=3.10"`,
    /// `"1.67"`).
    pub declared_value: String,
    /// Config file the declaration came from
    /// (e.g. `"Cargo.toml"`).
    pub source_file: String,
    /// Path within the config file
    /// (e.g. `"package.rust-version"`).
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ClaimKind {
    Performance,
    Comparison,
    Causal,
    Existence,
    Reproducibility,
    Provenance,
    /// PR5b: declarative claim about a configuration field
    /// (e.g. ``requires-python = ">=3.10"`` in pyproject.toml).
    /// Not an empirical measurement — the declaration IS the
    /// evidence. Synthesizer emits a metadata-flavored
    /// TrustReport without empirical Criteria.
    MetadataCompatibility,
    /// PR5f: behavioral concordance — paper claims its measured
    /// behavior tracks a prior paper's reported behavior, under
    /// a framework-owned relationship pattern (numeric_band,
    /// relative_band, same_order_of_magnitude, ordinal_match,
    /// monotone_with). The curator authors `prior_binding`; the
    /// docker artifact carries the measured value; the framework
    /// comparator decides. Synthesizer emits a concordance-
    /// flavored TrustReport whose status reflects the comparator's
    /// pass/fail/not-assessed verdict.
    BehavioralConcordance,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Assumption {
    pub text: String,
    pub load_bearing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceSpan {
    pub path: String,
    pub span: String,
}
