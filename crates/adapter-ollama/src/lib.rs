//! Ollama adapter.
//!
//! Ollama is a **local service**, not a database. Its evidence is therefore
//! different in kind from the Codex adapter's:
//!
//! | signal | what it proves | level |
//! |---|---|---|
//! | the service answers `/api/version` | Ollama is installed and running | E0 |
//! | models are listed by `/api/tags` | a model is available to call | E1 |
//! | **we performed a call and hold the response** | a real inference happened | E4 |
//!
//! The middle row is the trap this module exists to avoid: an installed model is
//! not usage. Per F2 authorisation §4.5, anything that happened outside our own
//! call flow must never be inferred — so `/api/tags` can *never* light the core,
//! and there is no "probably used it" path anywhere in this file.

#![forbid(unsafe_code)]

mod http;

use light_core::{CountableEvidence, EvidenceLevel, ModelRegistry, ProofMethod};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const ADAPTER_ID: &str = "ollama-local";
pub const ADAPTER_VERSION: &str = "1.0.0";
pub const DEFAULT_BASE: &str = "http://127.0.0.1:11434";

/// One locally available model, normalized to a registry family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalModel {
    /// Name as Ollama reports it, e.g. `qwen2.5:0.5b`.
    pub name: String,
    /// Registry family, if the name resolves. `None` must not be counted.
    pub family: Option<String>,
    pub size_bytes: u64,
    pub modified_at: String,
}

/// What the service looks like right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceInfo {
    pub base_url: String,
    pub reachable: bool,
    /// Present when the service answered `/api/version`.
    pub version: Option<String>,
    pub models: Vec<LocalModel>,
    /// Set when the probe failed, for an honest UI message.
    pub error: Option<String>,
}

/// The proof that a real call happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InferenceReceipt {
    pub request_id: String,
    pub model: String,
    pub family: Option<String>,
    pub prompt_chars: usize,
    pub response_chars: usize,
    /// Tokens reported by the service itself.
    pub eval_count: u64,
    pub created_at_ms: i64,
    /// The answer itself. Offered to the caller, **never** uploaded.
    pub response: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OllamaError {
    #[error("local Ollama service is not reachable at {base}")]
    Unreachable { base: String },
    #[error("model `{model}` is not installed locally")]
    ModelMissing { model: String },
    #[error("model name does not resolve to a known family: {model}")]
    UnknownFamily { model: String },
    #[error("service returned an error: {0}")]
    Service(String),
    #[error("malformed response: {0}")]
    Protocol(String),
}

/// Probe the service. Never errors: an unreachable service is a normal state.
pub fn probe(base: &str) -> ServiceInfo {
    match http::get_json(&format!("{}/api/version", base.trim_end_matches('/')), 1500) {
        Ok(value) => {
            let version = value
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let models = list_models(base).unwrap_or_default();
            ServiceInfo {
                base_url: base.to_string(),
                reachable: true,
                version,
                models,
                error: None,
            }
        }
        Err(e) => ServiceInfo {
            base_url: base.to_string(),
            reachable: false,
            version: None,
            models: vec![],
            error: Some(e),
        },
    }
}

fn list_models(base: &str) -> Result<Vec<LocalModel>, String> {
    let value = http::get_json(&format!("{}/api/tags", base.trim_end_matches('/')), 2000)?;
    let registry = ModelRegistry::bundled();
    let mut out = Vec::new();
    if let Some(models) = value.get("models").and_then(|m| m.as_array()) {
        for model in models {
            let Some(name) = model.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let family = registry.normalize(name).family;
            out.push(LocalModel {
                name: name.to_string(),
                family,
                size_bytes: model.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                modified_at: model
                    .get("modified_at")
                    .and_then(|m| m.as_str())
                    .unwrap_or_default()
                    .to_string(),
            });
        }
    }
    Ok(out)
}

/// Perform one inference on the user's behalf.
///
/// This is the only path that can produce evidence, and it exists only because
/// the person asked for it and supplied the prompt. The caller must present the
/// answer back to the user: the receipt is for a call that really happened.
pub fn generate(
    base: &str,
    model: &str,
    prompt: &str,
    registry: &ModelRegistry,
) -> Result<InferenceReceipt, OllamaError> {
    let normalized = registry.normalize(model);
    let Some(family) = normalized.family.clone() else {
        // An unknown family can't be attributed, so it must not be called for
        // evidence purposes. (The user could still use Ollama directly.)
        return Err(OllamaError::UnknownFamily {
            model: model.to_string(),
        });
    };

    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        // Keep the verification cheap: this is a proof of capability, not a chat.
        "options": { "num_predict": 64 },
    });

    let value = http::post_json(
        &format!("{}/api/generate", base.trim_end_matches('/')),
        &body.to_string(),
        60_000,
    )
    .map_err(|e| {
        if e.contains("404") || e.contains("not found") {
            OllamaError::ModelMissing {
                model: model.to_string(),
            }
        } else {
            OllamaError::Service(e)
        }
    })?;

    let response = value
        .get("response")
        .and_then(|r| r.as_str())
        .ok_or_else(|| OllamaError::Protocol("missing `response`".into()))?
        .to_string();

    let reported_model = value
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or(model)
        .to_string();

    let created_at_ms = value
        .get("created_at")
        .and_then(|c| c.as_str())
        .and_then(parse_rfc3339_ms)
        .unwrap_or_else(now_ms);

    let request_id = request_id(&reported_model, prompt, created_at_ms);

    Ok(InferenceReceipt {
        request_id,
        model: reported_model,
        family: Some(family),
        prompt_chars: prompt.chars().count(),
        response_chars: response.chars().count(),
        eval_count: value.get("eval_count").and_then(|c| c.as_u64()).unwrap_or(0),
        created_at_ms,
        response,
    })
}

/// Convert a receipt into countable evidence.
///
/// Level is E4: a supported call receipt. It is **only** produced by an
/// inference this adapter performed with a user-supplied prompt.
pub fn receipt_to_evidence(receipt: &InferenceReceipt, registry_semver: &str) -> Option<CountableEvidence> {
    Some(CountableEvidence {
        event_id: receipt.request_id.clone(),
        family: receipt.family.clone()?,
        level: EvidenceLevel::E4,
        proof_method: ProofMethod::ClientReceiptApi,
        adapter_id: ADAPTER_ID.to_string(),
        adapter_version: ADAPTER_VERSION.to_string(),
        registry_semver: registry_semver.to_string(),
        observed_at_ms: receipt.created_at_ms,
    })
}

fn request_id(model: &str, prompt: &str, at: i64) -> String {
    let mut h = Sha256::new();
    h.update(model.as_bytes());
    h.update(b"\x00");
    h.update(prompt.as_bytes());
    h.update(b"\x00");
    h.update(at.to_le_bytes());
    let d = h.finalize();
    d[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Minimal RFC 3339 parser for the timestamps Ollama returns.
fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    // 2026-10-31T09:12:00.123456789Z
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<i64> {
        s.get(range)?.parse::<i64>().ok()
    };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);

    // Days since Unix epoch (civil-from-days, Howard Hinnant's algorithm).
    let y_adj = if mo <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    Some(((days * 86400) + h * 3600 + mi * 60 + se) * 1000)
}

#[cfg(test)]
mod tests;
