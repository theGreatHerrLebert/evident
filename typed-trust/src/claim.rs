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
