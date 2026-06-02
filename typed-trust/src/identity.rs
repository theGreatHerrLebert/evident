//! Identity — see §1 of `concepts/typed-trust.md`.
//!
//! One identity type for everyone: humans, models, automation,
//! organizations, anonymized identities. The performer-vs-judge
//! distinction is enforced by where an Identity appears in a
//! Derivation (`ran_by`/`searched_by` = performer; `by` on Judged =
//! judge) plus the validator rule that Automated cannot judge
//! (invariant 9).

/// Who or what acted. Provenance for Verified/Absent, load-bearing
/// for Judged.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Identity {
    pub kind: IdentityKind,
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<IdentityDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    Human,
    Model,
    Automated,
    Organization,
    Anonymous,
}

/// Structured key/value detail. Well-known keys (`orcid`,
/// `affiliation`, `ci_run`, `version`, `anonymity_reason`,
/// `manifest_provenance`) are recognized by renderers; unknown keys
/// pass through.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IdentityDetail {
    pub key: String,
    pub value: String,
}

impl Identity {
    /// Degraded form for a shipping-manifest claim with
    /// `provenance: human` and no `reviewers[]` block. See §1
    /// validator rules — lossy, but preserves the provenance signal
    /// without inventing a reviewer identity the manifest never
    /// carried.
    pub fn unspecified_human_from_manifest() -> Self {
        Self {
            kind: IdentityKind::Human,
            name: "unspecified".into(),
            details: vec![IdentityDetail {
                key: "manifest_provenance".into(),
                value: "human".into(),
            }],
        }
    }
}
