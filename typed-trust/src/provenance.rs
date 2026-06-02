//! ProvenanceRecord — chain of custody for an artifact.
//!
//! Cf. shipping schema's `pinned_versions` + `last_verified` +
//! `signature` (reserved v2). The typed form makes origin and any
//! transformations first-class.

use crate::derivation::{Attested, Locator};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProvenanceRecord {
    pub artifact: Locator,
    pub origin: Attested<Origin>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transformations: Vec<Transformation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<CryptoAttestation>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Origin {
    Original,
    Ported {
        from: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        license: Option<String>,
    },
    PaperInspired {
        citation: String,
    },
    Generated {
        by: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Transformation {
    pub kind: String,
    pub detail: String,
}

/// Reserved for shipping schema v2 signature support.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CryptoAttestation {
    pub kind: String,
    pub by: String,
    pub value: String,
    pub digest: String,
}
