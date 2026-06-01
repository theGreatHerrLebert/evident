//! ProvenanceRecord — chain of custody for an artifact.
//!
//! Cf. shipping schema's `pinned_versions` + `last_verified` +
//! `signature` (reserved v2). The typed form makes origin and any
//! transformations first-class.

use crate::derivation::{Attested, Locator};

#[derive(Debug, Clone)]
pub struct ProvenanceRecord {
    pub artifact: Locator,
    pub origin: Attested<Origin>,
    pub transformations: Vec<Transformation>,
    pub integrity: Option<CryptoAttestation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Original,
    Ported {
        from: String,
        license: Option<String>,
    },
    PaperInspired {
        citation: String,
    },
    Generated {
        by: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transformation {
    pub kind: String,
    pub detail: String,
}

/// Reserved for shipping schema v2 signature support. Carries
/// signature kind (e.g. `ed25519`, `sigstore`), signer id, signature
/// bytes, and the digest that was signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoAttestation {
    pub kind: String,
    pub by: String,
    pub value: String,
    pub digest: String,
}
