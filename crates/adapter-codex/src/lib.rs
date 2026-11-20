//! Codex adapter.
//!
//! Reads **only** these metadata columns, declared in [`manifest`]:
//!
//! | table | columns | why |
//! |---|---|---|
//! | `threads` | `id`, `model`, `model_provider` | family attribution (gate A) |
//! | `thread_turns` | `thread_id`, `status` | corroboration only |
//! | `thread_items` | `thread_id`, `item_type` | call occurrence (gate B) |
//!
//! Never read: `first_user_message`, `preview`, `title`, `item_json`,
//! `feedback_log_body`, `cwd`, `rollout_path`, `tokens_used`. The guard makes
//! these unreachable and the manifest cannot be widened (see `adapter-sdk`).
//!
//! # The two gates
//!
//! F0 measured that `threads.model` is written when a session is **created**.
//! Counting it directly would report usage that never happened. So a session
//! only becomes evidence when:
//!
//! - **gate A**: `model` resolves to a family in the registry, and
//! - **gate B**: the session has at least one model-output item.
//!
//! `tokens_used` is not consulted at all — on real data the output-only rule
//! produced an identical family count, so reading consumption would only add
//! privacy cost.

#![forbid(unsafe_code)]

use adapter_sdk::{
    AdapterManifest, AuditEntry, EvidenceCapability, Filter, GroupCountSpec, GuardError, GuardedDb,
    OpenMode, Platform, QueryAudit, RowsSpec, TablePolicy, Value,
};
use light_core::{EvidenceDecision, EvidencePolicy, ResponseProof};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const ADAPTER_ID: &str = "codex-app";
pub const ADAPTER_VERSION: &str = "1.0.0";

pub fn manifest() -> AdapterManifest {
    AdapterManifest {
        id: ADAPTER_ID.into(),
        version: ADAPTER_VERSION.into(),
        platforms: vec![Platform::Macos, Platform::Windows],
        tables: vec![
            TablePolicy::new("threads", &["id", "model", "model_provider"]),
            TablePolicy::new("thread_turns", &["thread_id", "status"]),
            TablePolicy::new("thread_items", &["thread_id", "item_type"]),
        ],
        evidence_capability: EvidenceCapability::LocalCallProof,
        max_rows: 20_000,
        timeout_ms: 15_000,
    }
}

/// Where the client keeps its data. Both files are optional and are never
/// searched for: the caller must supply exact paths.
#[derive(Debug, Clone)]
pub struct CodexPaths {
    /// The session index (contains `threads`).
    pub state_db: PathBuf,
    /// The turn/item store (contains `thread_turns`, `thread_items`).
    pub history_db: Option<PathBuf>,
}

impl CodexPaths {
    pub fn new(state_db: impl Into<PathBuf>, history_db: Option<PathBuf>) -> Self {
        Self {
            state_db: state_db.into(),
            history_db,
        }
    }
}

/// One session's verdict. `session_ref` is a truncated digest, not the raw id,
/// so a report can be shared without exposing session identifiers.
#[derive(Debug, Clone, Serialize)]
pub struct SessionDecision {
    pub session_ref: String,
    pub provider: String,
    pub decision: EvidenceDecision,
    pub proof: ResponseProof,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanCounts {
    pub sessions_seen: usize,
    pub countable_sessions: usize,
    pub observable_sessions: usize,
    pub rejected_task_label: usize,
    pub rejected_unknown_family: usize,
    pub rejected_empty_model: usize,
    /// Sessions whose `model_provider` did not contain the resolved family slug.
    /// Informational only: the registry stays authoritative.
    pub provider_disagreements: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub adapter_id: String,
    pub adapter_version: String,
    pub registry_semver: String,
    /// Which read-only strategy worked for each database. Reported, not hidden.
    pub state_open_mode: OpenMode,
    pub history_open_mode: Option<OpenMode>,
    pub verified_families: Vec<String>,
    pub counts: ScanCounts,
    pub sessions: Vec<SessionDecision>,
    #[serde(skip)]
    pub audit: QueryAudit,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScanError {
    #[error(transparent)]
    Guard(#[from] GuardError),
    #[error("state database is required")]
    MissingStateDb,
}

/// Scan a Codex installation.
///
/// Deterministic and offline: same database in, same report out. If the history
/// database is absent, every session is `Observable` (no call proof) — it never
/// falls back to counting the model string alone.
pub fn scan(paths: &CodexPaths, policy: &EvidencePolicy) -> Result<ScanReport, ScanError> {
    // ---- gate A input: session rows -------------------------------------
    // `compatible` still cannot write; it only tolerates live WAL databases,
    // which strict read-only cannot read at all.
    let mut state = GuardedDb::open_compatible(&paths.state_db, manifest())?;
    let state_open_mode = state.open_mode();
    let max = manifest().max_rows;
    let thread_rows = state.rows(RowsSpec {
        table: "threads".into(),
        columns: vec!["id".into(), "model".into(), "model_provider".into()],
        filter: None,
        limit: max,
    })?;

    let mut sessions: Vec<(String, String, String)> = Vec::with_capacity(thread_rows.len());
    for row in &thread_rows {
        let id = row.get_text("id").unwrap_or_default().to_string();
        let model = row.get_text("model").unwrap_or_default().to_string();
        let provider = row.get_text("model_provider").unwrap_or_default().to_string();
        sessions.push((id, model, provider));
    }

    // ---- gate B input: per-session output item counts --------------------
    let mut output_counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut completed_turns: BTreeMap<String, u32> = BTreeMap::new();
    let mut history_audit = QueryAudit::default();
    let mut history_open_mode: Option<OpenMode> = None;

    if let Some(history_path) = &paths.history_db {
        let mut history = GuardedDb::open_compatible(history_path, manifest())?;
        history_open_mode = Some(history.open_mode());

        let output_placeholders: Vec<Value> = policy
            .model_output_item_types
            .iter()
            .map(|t| Value::from(t.as_str()))
            .collect();

        for (key, count) in history.group_count(GroupCountSpec {
            table: "thread_items".into(),
            group_column: "thread_id".into(),
            filter: Some(Filter::is_in("item_type", output_placeholders)),
            limit: max,
        })? {
            if let Value::Text(thread_id) = key {
                output_counts.insert(thread_id, count.max(0) as u32);
            }
        }

        for (key, count) in history.group_count(GroupCountSpec {
            table: "thread_turns".into(),
            group_column: "thread_id".into(),
            filter: Some(Filter::eq("status", "completed")),
            limit: max,
        })? {
            if let Value::Text(thread_id) = key {
                completed_turns.insert(thread_id, count.max(0) as u32);
            }
        }

        history_audit = history.audit().clone();
    }

    // ---- decide ----------------------------------------------------------
    let mut counts = ScanCounts::default();
    let mut verified: BTreeSet<String> = BTreeSet::new();
    let mut decisions = Vec::with_capacity(sessions.len());

    for (id, model, provider) in &sessions {
        counts.sessions_seen += 1;

        let proof = ResponseProof {
            output_items: output_counts.get(id).copied().unwrap_or(0),
            completed_turns: completed_turns.get(id).copied().unwrap_or(0),
            output_item_types: policy.model_output_item_types.clone(),
        };

        let decision = policy.evaluate(model, &proof);
        match &decision {
            EvidenceDecision::Countable { family, .. } => {
                counts.countable_sessions += 1;
                verified.insert(family.clone());
                let provider_lower = provider.to_ascii_lowercase();
                if !provider_lower.is_empty() && !provider_lower.contains(family.as_str()) {
                    counts.provider_disagreements += 1;
                }
            }
            EvidenceDecision::Observable { .. } => counts.observable_sessions += 1,
            EvidenceDecision::Rejected { reason, .. } => match reason {
                light_core::RejectReason::TaskLabelNotAModel => counts.rejected_task_label += 1,
                light_core::RejectReason::UnknownFamily => counts.rejected_unknown_family += 1,
                light_core::RejectReason::EmptyModel => counts.rejected_empty_model += 1,
            },
        }

        decisions.push(SessionDecision {
            session_ref: session_ref(id),
            provider: provider.clone(),
            decision,
            proof,
        });
    }

    let mut audit = state.audit().clone();
    audit.entries.extend(history_audit.entries);
    audit.rejections.extend(history_audit.rejections);
    audit.rows_read += history_audit.rows_read;

    Ok(ScanReport {
        adapter_id: ADAPTER_ID.into(),
        adapter_version: ADAPTER_VERSION.into(),
        registry_semver: policy.registry.semver().to_string(),
        state_open_mode,
        history_open_mode,
        verified_families: verified.into_iter().collect(),
        counts,
        sessions: decisions,
        audit,
    })
}

/// Short, non-reversible session handle for reports.
fn session_ref(id: &str) -> String {
    let digest = Sha256::digest(id.as_bytes());
    let hex: String = digest[..6].iter().map(|b| format!("{b:02x}")).collect();
    hex
}

/// Every column this adapter is allowed to touch. Exposed so the privacy test
/// suite can assert the manifest stays minimal over time.
pub fn allowed_columns() -> Vec<(String, Vec<String>)> {
    manifest()
        .tables
        .iter()
        .map(|p| (p.table.clone(), p.allow.clone()))
        .collect()
}

/// Audit view helper used by tests.
pub fn audit_entries(report: &ScanReport) -> Vec<&AuditEntry> {
    report.audit.entries.iter().collect()
}

#[cfg(test)]
mod tests;
