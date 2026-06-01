//! Typed Trust core.
//!
//! Rust scaffold for the design specified in `concepts/typed-trust.md`.
//! The verification engine for propositional empirical claims: every
//! `Attested<T>` records *how* the value was established (Verified /
//! Judged / Absent), reviews are graph edges via `ReviewEvent`, and
//! synthesis surfaces contestation as render status without typing
//! it into every value.
//!
//! Module map mirrors the design document's sections:
//! - [`identity`] — §1 Identity
//! - [`derivation`] — §2 Derivation, Attested<T>
//! - [`claim`] — §5 Claim
//! - [`review`] — §6 ReviewEvent, Target (with the F-PR14 split)
//! - [`evidence`] — Evidence + SupportRelation
//! - [`provenance`] — ProvenanceRecord
//! - [`report`] — §7/§8 TrustReport, Tolerance, MetricObservation,
//!   RenderStatus
//! - [`ids`] — newtype identifiers
//!
//! Validator rules (invariants 9, 10, the Eq-with-relative_error
//! warning, etc.) are NOT enforced at construction time in this
//! scaffold. They belong in a separate `validate` module added when
//! the manifest seam translator lands.

pub mod ids;
pub mod identity;
pub mod derivation;
pub mod claim;
pub mod evidence;
pub mod review;
pub mod provenance;
pub mod report;
pub mod translate;

pub use ids::*;
pub use identity::*;
pub use derivation::*;
pub use claim::*;
pub use evidence::*;
pub use review::*;
pub use provenance::*;
pub use report::*;
