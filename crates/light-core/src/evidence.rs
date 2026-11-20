//! Evidence levels and the "did a real call happen" gate.
//!
//! ## Why this module exists
//!
//! During F0 we measured a real AI client database and found that
//! `threads.model` is recorded **when a session is created**, before any model
//! call happens. On that machine, 78 of 347 threads carried a model string with
//! zero tokens and zero model output. Counting those as "usage" would fabricate
//! usage that never occurred.
//!
//! The same investigation found a safe metadata signal that does prove a call:
//! `thread_items.item_type`. The `item_type` column is an enum-like label (it is
//! *not* message content), and a row of type `agentMessage` / `reasoning` can
//! only exist if the model actually produced output.
//!
//! So attribution and occurrence are **two independent gates**:
//!
//! - **Gate A — attribution**: the raw model string resolves to a family in the
//!   registry (task labels and unknown strings fail here).
//! - **Gate B — occurrence**: at least one model-produced output item exists.
//!
//! Both must pass for E3. Neither alone is sufficient. Consumption counters
//! (`tokens_used`) are deliberately **not read at all**: on real data the
//! output-only rule produced an identical result, so reading consumption numbers
//! would add privacy cost for zero accuracy (see ADR-011).

use crate::registry::{Disposition, ModelRegistry, Normalization};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Evidence level, per product spec §5.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub enum EvidenceLevel {
    /// Detected: an app, CLI, config path, or service exists.
    #[serde(rename = "E0")]
    E0,
    /// Connected: a legal connection/login exists.
    #[serde(rename = "E1")]
    E1,
    /// Self-attested: the user ticked "I have used this". Display only.
    #[serde(rename = "E2")]
    E2,
    /// Locally verifiable: an authorized adapter emitted a validated usage event.
    #[serde(rename = "E3")]
    E3,
    /// Supported call receipt: an authorized integration returned a success proof.
    #[serde(rename = "E4")]
    E4,
}

impl EvidenceLevel {
    /// Only E3 and E4 may deepen the core colour or feed trusted statistics.
    pub fn is_countable(self) -> bool {
        matches!(self, Self::E3 | Self::E4)
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::E0 => 0,
            Self::E1 => 1,
            Self::E2 => 2,
            Self::E3 => 3,
            Self::E4 => 4,
        }
    }
}

/// How a countable evidence record was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProofMethod {
    /// Read-only metadata from a local client database through a column allowlist.
    LocalSqliteMetadata,
    /// A signed receipt returned by an authorized integration.
    ClientReceiptApi,
    /// A user-installed hook reported a completed call.
    HookCallback,
    /// A third-party adapter report carrying its own signature.
    SignedAdapterReport,
}

/// Metadata-only observation of whether the model produced output.
///
/// Carries **counts**, never content. `completed_turns` is corroboration and is
/// recorded for auditability, but is not required — see `proves_call`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResponseProof {
    /// Number of items of a model-output type observed for this session.
    pub output_items: u32,
    /// Number of turns marked completed. Corroboration only.
    pub completed_turns: u32,
    /// Distinct `item_type` labels counted toward `output_items`.
    pub output_item_types: Vec<String>,
}

impl ResponseProof {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn from_counts(output_items: u32, completed_turns: u32) -> Self {
        Self {
            output_items,
            completed_turns,
            output_item_types: Vec::new(),
        }
    }

    /// The occurrence gate. A model that never emitted output cannot have been
    /// used, no matter what the session row says.
    pub fn proves_call(&self) -> bool {
        self.output_items > 0
    }
}

/// Why a record could not be counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    /// The client recorded no model at all.
    EmptyModel,
    /// The string is an internal task/subagent label, not a model.
    TaskLabelNotAModel,
    /// No registry rule matched. Never guessed.
    UnknownFamily,
}

/// Why a record is visible but not countable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservableReason {
    /// A session was created with a model preselected, but the model never
    /// produced output. This is the exact failure mode measured in F0.
    SessionCreatedButNoModelOutput,
}

/// The verdict for one observed model string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum EvidenceDecision {
    /// Counts toward the core colour, the knowledge title, and trusted stats.
    Countable {
        family: String,
        level: EvidenceLevel,
        normalization: Normalization,
    },
    /// Shown to the user, never counted. E1 at most.
    Observable {
        level: EvidenceLevel,
        reason: ObservableReason,
        normalization: Normalization,
    },
    /// Not a model family; not even shown as usage.
    Rejected {
        reason: RejectReason,
        normalization: Normalization,
    },
}

impl EvidenceDecision {
    pub fn normalization(&self) -> &Normalization {
        match self {
            Self::Countable { normalization, .. }
            | Self::Observable { normalization, .. }
            | Self::Rejected { normalization, .. } => normalization,
        }
    }

    pub fn family(&self) -> Option<&str> {
        match self {
            Self::Countable { family, .. } => Some(family.as_str()),
            _ => None,
        }
    }
}

/// The rules that turn an observation into an evidence decision.
#[derive(Debug, Clone)]
pub struct EvidencePolicy {
    pub registry: ModelRegistry,
    /// `item_type` labels that can only exist if the model produced output.
    pub model_output_item_types: Vec<String>,
}

impl EvidencePolicy {
    /// Item types that prove the model produced something.
    ///
    /// Deliberately excluded, with reasons:
    /// - `userMessage` — the user's own input, proves nothing about the model.
    /// - `imageView` — the user looking at an image.
    /// - `sleep` — a harness timer.
    /// - `contextCompaction` — harness bookkeeping, not model output.
    pub const DEFAULT_OUTPUT_ITEM_TYPES: &'static [&'static str] = &[
        "agentMessage",
        "reasoning",
        "commandExecution",
        "mcpToolCall",
        "webSearch",
        "fileChange",
        "imageGeneration",
        "collabAgentToolCall",
        "subAgentActivity",
    ];

    /// Item types explicitly rejected as proof.
    pub const NON_PROOF_ITEM_TYPES: &'static [&'static str] = &[
        "userMessage",
        "imageView",
        "sleep",
        "contextCompaction",
    ];

    pub fn v1() -> Self {
        Self {
            registry: ModelRegistry::bundled(),
            model_output_item_types: Self::DEFAULT_OUTPUT_ITEM_TYPES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }

    pub fn with_registry(registry: ModelRegistry) -> Self {
        Self {
            registry,
            model_output_item_types: Self::DEFAULT_OUTPUT_ITEM_TYPES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }

    pub fn classifies_as_output_item(&self, item_type: &str) -> bool {
        self.model_output_item_types.iter().any(|t| t == item_type)
    }

    /// Evaluate one observed model string against the occurrence proof.
    pub fn evaluate(&self, raw_model: &str, proof: &ResponseProof) -> EvidenceDecision {
        let normalization = self.registry.normalize(raw_model);

        if raw_model.trim().is_empty() {
            return EvidenceDecision::Rejected {
                reason: RejectReason::EmptyModel,
                normalization,
            };
        }

        match &normalization.disposition {
            Disposition::Unknown => EvidenceDecision::Rejected {
                reason: RejectReason::UnknownFamily,
                normalization,
            },
            Disposition::TaskLabel { .. } => EvidenceDecision::Rejected {
                reason: RejectReason::TaskLabelNotAModel,
                normalization,
            },
            Disposition::Family { family } => {
                let family = family.clone();
                if proof.proves_call() {
                    EvidenceDecision::Countable {
                        family,
                        level: EvidenceLevel::E3,
                        normalization,
                    }
                } else {
                    EvidenceDecision::Observable {
                        level: EvidenceLevel::E1,
                        reason: ObservableReason::SessionCreatedButNoModelOutput,
                        normalization,
                    }
                }
            }
        }
    }

    /// Count distinct families that pass both gates.
    ///
    /// This is the only function that should ever feed the light level.
    pub fn count_verified_families<'a, I>(&self, observations: I) -> BTreeSet<String>
    where
        I: IntoIterator<Item = (&'a str, &'a ResponseProof)>,
    {
        observations
            .into_iter()
            .filter_map(|(raw, proof)| match self.evaluate(raw, proof) {
                EvidenceDecision::Countable { family, .. } => Some(family),
                _ => None,
            })
            .collect()
    }
}

impl Default for EvidencePolicy {
    fn default() -> Self {
        Self::v1()
    }
}

/// A countable evidence record — the shape that may be sent to a server.
///
/// Note the absence of: raw model strings beyond the family, token counts,
/// prompt text, file paths, and any database content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CountableEvidence {
    pub event_id: String,
    pub family: String,
    pub level: EvidenceLevel,
    pub proof_method: ProofMethod,
    pub adapter_id: String,
    pub adapter_version: String,
    pub registry_semver: String,
    pub observed_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> EvidencePolicy {
        EvidencePolicy::v1()
    }

    #[test]
    fn family_plus_output_is_countable() {
        let p = policy();
        let proof = ResponseProof::from_counts(12, 3);
        let d = p.evaluate("gpt-6-sol", &proof);
        assert_eq!(d.family(), Some("gpt"));
        match d {
            EvidenceDecision::Countable { level, .. } => assert_eq!(level, EvidenceLevel::E3),
            other => panic!("expected countable, got {other:?}"),
        }
    }

    #[test]
    fn family_without_output_is_observable_not_countable() {
        // The exact F0 failure mode: session created, model preselected, no call.
        let p = policy();
        let d = p.evaluate("gpt-6-sol", &ResponseProof::none());
        match d {
            EvidenceDecision::Observable { level, reason, .. } => {
                assert_eq!(level, EvidenceLevel::E1);
                assert_eq!(reason, ObservableReason::SessionCreatedButNoModelOutput);
            }
            other => panic!("expected observable, got {other:?}"),
        }
        assert!(p.evaluate("gpt-6-sol", &ResponseProof::none()).family().is_none());
    }

    #[test]
    fn completed_turns_alone_do_not_prove_a_call() {
        // A turn can be marked completed while no model output item exists.
        // We refuse to count that: output is the proof, turns are corroboration.
        let p = policy();
        let proof = ResponseProof::from_counts(0, 5);
        assert!(!proof.proves_call());
        assert!(matches!(
            p.evaluate("gpt-6-sol", &proof),
            EvidenceDecision::Observable { .. }
        ));
    }

    #[test]
    fn task_label_is_rejected_regardless_of_output() {
        let p = policy();
        let d = p.evaluate("codex-auto-review", &ResponseProof::from_counts(999, 999));
        match d {
            EvidenceDecision::Rejected { reason, .. } => {
                assert_eq!(reason, RejectReason::TaskLabelNotAModel)
            }
            other => panic!("expected rejected, got {other:?}"),
        }
    }

    #[test]
    fn empty_and_unknown_are_rejected() {
        let p = policy();
        let proof = ResponseProof::from_counts(5, 1);
        assert!(matches!(
            p.evaluate("", &proof),
            EvidenceDecision::Rejected { reason: RejectReason::EmptyModel, .. }
        ));
        assert!(matches!(
            p.evaluate("mystery-model-zzz", &proof),
            EvidenceDecision::Rejected { reason: RejectReason::UnknownFamily, .. }
        ));
    }

    #[test]
    fn client_prefixed_strings_contribute_one_family() {
        let p = policy();
        let proof = ResponseProof::from_counts(3, 1);
        // A thread with no output must not contribute, even though its model is
        // a known family.
        let no_output = ResponseProof::none();
        let set = p.count_verified_families(vec![
            ("chatgpt-web/gpt-6-sol", &proof),
            ("gpt-5.6-sol", &proof),
            ("codex-auto-review", &proof),
            ("", &proof),
            ("gpt-6-sol", &no_output),
        ]);
        assert_eq!(set.len(), 1);
        assert!(set.contains("gpt"));
    }

    #[test]
    fn output_item_taxonomy_is_explicit() {
        let p = policy();
        // model-produced
        for t in ["agentMessage", "reasoning", "commandExecution", "webSearch"] {
            assert!(p.classifies_as_output_item(t), "{t} should prove a call");
        }
        // user-originated or harness bookkeeping
        for t in EvidencePolicy::NON_PROOF_ITEM_TYPES {
            assert!(!p.classifies_as_output_item(t), "{t} must not prove a call");
        }
        // no overlap between the two sets
        for t in EvidencePolicy::NON_PROOF_ITEM_TYPES {
            assert!(
                !EvidencePolicy::DEFAULT_OUTPUT_ITEM_TYPES.contains(t),
                "`{t}` appears in both taxonomies"
            );
        }
    }

    #[test]
    fn evidence_level_gating() {
        assert!(!EvidenceLevel::E0.is_countable());
        assert!(!EvidenceLevel::E1.is_countable());
        assert!(!EvidenceLevel::E2.is_countable());
        assert!(EvidenceLevel::E3.is_countable());
        assert!(EvidenceLevel::E4.is_countable());
        assert_eq!(EvidenceLevel::E3.as_u8(), 3);
    }

    #[test]
    fn countable_evidence_serializes_without_sensitive_fields() {
        let e = CountableEvidence {
            event_id: "01J8Z3K9".into(),
            family: "gpt".into(),
            level: EvidenceLevel::E3,
            proof_method: ProofMethod::LocalSqliteMetadata,
            adapter_id: "codex-app".into(),
            adapter_version: "1.0.0".into(),
            registry_semver: "1.0.0".into(),
            observed_at_ms: 1_761_868_800_000,
        };
        let json = serde_json::to_string(&e).unwrap();
        for forbidden in ["token", "prompt", "message", "path", "/Users/", "key"] {
            assert!(
                !json.to_lowercase().contains(forbidden),
                "payload must not contain `{forbidden}`: {json}"
            );
        }
        assert!(json.contains("\"level\":\"E3\""));
    }
}
