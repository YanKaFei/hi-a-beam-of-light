//! FirstLight Core — platform-neutral reference implementation.
//!
//! This crate contains **no I/O, no network, and no platform code**. It is a
//! pure library so that:
//!
//! 1. every rule can be tested deterministically and offline;
//! 2. the same rules can be re-implemented on any platform and checked against
//!    the shared fixtures in `packages/protocol/fixtures/`.
//!
//! Module map:
//!
//! | module | responsibility |
//! |---|---|
//! | [`codec`] | hex / base32 without external dependencies |
//! | [`ids`] | anonymous identifier derivation |
//! | [`identity`] | root seed, epoch keys, recovery code, lifecycle |
//! | [`registry`] | model family dictionary and normalization |
//! | [`evidence`] | E0–E4 levels and the "proven call" gate |
//! | [`light`] | light level, palette, and the two title ladders |
//! | [`transmission`] | transmission session state machine |
//! | [`protocol`] | wire types and JSON schema export |
//! | [`node`] | Node / Device / Identity accounting |

#![forbid(unsafe_code)]

pub mod codec;
pub mod evidence;
pub mod identity;
pub mod ids;
pub mod light;
pub mod node;
pub mod protocol;
pub mod registry;
pub mod transmission;

pub use evidence::{
    CountableEvidence, EvidenceDecision, EvidenceLevel, EvidencePolicy, ObservableReason,
    ProofMethod, RejectReason, ResponseProof,
};
pub use identity::{
    verify_epoch_certificate, EpochCertificate, IdentityState, RecoveryCode, RootIdentity,
};
pub use light::{
    compute_light_state, knowledge_title, light_level, palette, transmission_title, KnowledgeTitle,
    LightLevel, LightState, Palette, TransmissionTitle,
};
pub use node::{NodeBook, NodeBookError};
pub use protocol::PROTOCOL_VERSION;
pub use registry::{Disposition, MatchKind, ModelRegistry, Normalization};
pub use transmission::{Context, Effect, FsmError, Session, SessionEvent, SessionState, Transition};

/// Errors shared across the core.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("registry is invalid: {0}")]
    Registry(String),
    #[error("recovery code is malformed or checksum failed")]
    RecoveryCodeInvalid,
    #[error("signature verification failed")]
    SignatureInvalid,
    #[error("epoch certificate does not match the given anon id")]
    EpochMismatch,
}
