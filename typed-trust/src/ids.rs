//! Stable identifier newtypes.
//!
//! Each id wraps a String. The type system catches a `ClaimId` being
//! passed where an `EventId` is expected, without inventing an
//! identity scheme. Equality is string equality.

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

id_type!(ClaimId, "Identifies a Claim across the manifest and the graph.");
id_type!(EventId, "Identifies a ReviewEvent.");
id_type!(EvidenceId, "Identifies an Evidence record.");
id_type!(AttestedId, "Identifies a specific Attested<T> value when targeted.");
id_type!(ReportId, "Identifies a synthesized TrustReport snapshot.");
id_type!(CriterionId, "Identifies a Criterion. Stable across re-synthesis; MetricObservation binds here.");
id_type!(ProvenanceId, "Identifies a ProvenanceRecord.");
id_type!(ProtocolId, "Identifies a review/judgment protocol (rubric, prompt, guideline).");
id_type!(ModelId, "Identifies a specific model (name + version).");

/// ISO-8601 timestamp string (e.g. `"2026-05-11T00:00:00Z"`).
pub type Timestamp = String;

/// Hex hash string (e.g. `"sha256:abc..."` or bare hex digest).
pub type Hash = String;
