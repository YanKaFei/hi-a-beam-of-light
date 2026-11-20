//! Wire types shared by every platform, plus JSON Schema export.
//!
//! This module is the cross-platform contract (F1 requirement D-06: the protocol
//! must not be tied to the desktop implementation). It contains **only data and
//! constants** — no transport, no storage.
//!
//! All request structs use `deny_unknown_fields`. That is what makes "never send
//! token counts or prompt text" a machine check rather than a promise: an extra
//! field is a deserialisation error, not a silent passthrough (F0 gap G-15).

use crate::evidence::{EvidenceLevel, ProofMethod};
use crate::light::{KnowledgeTitle, LightLevel};
use crate::transmission::SessionState;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROTOCOL_VERSION: &str = "v1";

/// SSE heartbeat interval. Keeps proxies from closing idle streams and lets the
/// client distinguish "quiet" from "dead".
pub const SSE_HEARTBEAT_MS: u64 = 15_000;

/// Server-side cap on field pulses per stream (spec §14.3: ≤2/min).
pub const SSE_MAX_EVENTS_PER_MINUTE: u32 = 2;

/// How far back `Last-Event-ID` may resume. Beyond this the server answers
/// `410 Gone` with `resync: true` instead of replaying history as "live".
pub const SSE_REPLAY_WINDOW_MS: i64 = 300_000;

/// Allowed request clock skew.
pub const CLOCK_SKEW_MS: i64 = 120_000;

/// Default one-time transmission session lifetime.
pub const TRANSMISSION_DEFAULT_TTL_SECONDS: u32 = 600;

// ---------------------------------------------------------------------------
// Usage attestation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UsageAttestRequest {
    pub event_id: String,
    pub family: String,
    pub level: EvidenceLevel,
    pub proof_method: ProofMethod,
    pub adapter_id: String,
    pub adapter_version: String,
    pub registry_semver: String,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UsageAttestResponse {
    /// False when this family was already counted for the identity.
    pub counted: bool,
    pub verified_families: u32,
    pub level: LightLevel,
    pub knowledge_title: KnowledgeTitle,
}

// ---------------------------------------------------------------------------
// Transmission
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Qr,
    Nfc,
    Link,
    Nearby,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransmissionCreateRequest {
    pub channel: Channel,
    pub ttl_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransmissionCreateResponse {
    pub id: String,
    /// Returned exactly once. The server stores only a hash.
    pub token: String,
    pub receive_url: String,
    /// String to write to an NDEF tag: a public HTTPS URL, nothing else.
    pub nfc_payload: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransmissionActivateRequest {
    pub usage_event_id: String,
    pub session_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransmissionStateResponse {
    pub id: String,
    pub state: SessionState,
    pub version: u32,
    pub expires_at_ms: i64,
}

// ---------------------------------------------------------------------------
// Public field
// ---------------------------------------------------------------------------

/// One public metric. Every metric carries its own window and note, so the three
/// quantities can never be silently merged into a single "total" (spec §8.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldMetric {
    pub value: u64,
    pub window: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldSummary {
    pub generated_at_ms: i64,
    pub cache_seconds: u32,
    /// Nodes that consented and have at least one verified activation.
    pub total_activated_nodes: FieldMetric,
    /// Short-window online instances. Not a number of people.
    pub online_instances: FieldMetric,
    /// New receiver activations in the window.
    pub new_activations: FieldMetric,
    /// Nodes that helped at least one other node activate.
    pub helper_nodes: FieldMetric,
    pub methodology_url: String,
}

/// Coarse density band. Deliberately discrete so an abused endpoint cannot
/// reconstruct a real distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DensityBand {
    None,
    Low,
    Mid,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegionEntry {
    pub code: String,
    pub name: String,
    pub active_nodes: u64,
    pub new_activations: u64,
    pub density_band: DensityBand,
    pub suppressed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldRegionsResponse {
    pub level: String,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    pub k_threshold: u32,
    pub regions: Vec<RegionEntry>,
}

// ---------------------------------------------------------------------------
// SSE contract (D-07)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldEventKind {
    /// Someone far away lit up.
    FarLight,
    /// Several events merged because of the rate cap.
    Aggregate,
    /// A receiver completed a verified activation.
    TransmissionActivated,
}

/// One SSE frame. `id` doubles as the SSE `id:` field, which the browser sends
/// back as `Last-Event-ID` on reconnect — that is the dedup key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldEvent {
    pub id: u64,
    pub kind: FieldEventKind,
    pub region: Option<String>,
    pub band: Option<DensityBand>,
    pub at_ms: i64,
    /// >1 only for `Aggregate`.
    pub merged_count: u32,
}

/// Sent once at the head of a stream so the client knows the rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StreamHello {
    pub protocol_version: String,
    /// Echo of the `Last-Event-ID` the client supplied, if any.
    pub resumed_from: Option<u64>,
    pub heartbeat_ms: u64,
    pub rate_limit_per_minute: u32,
    pub replay_window_ms: i64,
    /// Anonymous viewers get band-level data only.
    pub identity_required_for_detail: bool,
}

// ---------------------------------------------------------------------------
// Schema export
// ---------------------------------------------------------------------------

/// Every wire type, keyed by a stable name. Committed under
/// `packages/protocol/schemas/` and checked in CI.
pub fn exported_schemas() -> BTreeMap<String, serde_json::Value> {
    let mut map = BTreeMap::new();
    macro_rules! add {
        ($name:literal, $ty:ty) => {
            map.insert(
                $name.to_string(),
                serde_json::to_value(schemars::schema_for!($ty)).expect("schema serializes"),
            );
        };
    }
    add!("usage_attest_request", UsageAttestRequest);
    add!("usage_attest_response", UsageAttestResponse);
    add!("transmission_create_request", TransmissionCreateRequest);
    add!("transmission_create_response", TransmissionCreateResponse);
    add!("transmission_activate_request", TransmissionActivateRequest);
    add!("transmission_state_response", TransmissionStateResponse);
    add!("field_metric", FieldMetric);
    add!("field_summary", FieldSummary);
    add!("field_regions_response", FieldRegionsResponse);
    add!("field_event", FieldEvent);
    add!("stream_hello", StreamHello);
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_attest() -> serde_json::Value {
        serde_json::json!({
            "event_id": "01J8Z3K9ABCDEF",
            "family": "gpt",
            "level": "E3",
            "proof_method": "local_sqlite_metadata",
            "adapter_id": "codex-app",
            "adapter_version": "1.0.0",
            "registry_semver": "1.0.0",
            "observed_at_ms": 1_761_868_800_000i64
        })
    }

    #[test]
    fn attest_request_roundtrips() {
        let req: UsageAttestRequest = serde_json::from_value(minimal_attest()).unwrap();
        assert_eq!(req.family, "gpt");
        assert_eq!(req.level, EvidenceLevel::E3);
        let back = serde_json::to_value(&req).unwrap();
        assert_eq!(back, minimal_attest());
    }

    #[test]
    fn attest_request_rejects_smuggled_fields() {
        // G-15: consumption counters and prompt text must be impossible to send.
        for forbidden in [
            ("tokens_used", serde_json::json!(12345)),
            ("prompt", serde_json::json!("hello")),
            ("chat_text", serde_json::json!("...")),
            ("api_key", serde_json::json!("sk-...")),
            ("db_path", serde_json::json!("/Users/x/.codex/state.sqlite")),
        ] {
            let mut v = minimal_attest();
            v[forbidden.0] = forbidden.1;
            let err = serde_json::from_value::<UsageAttestRequest>(v).unwrap_err();
            assert!(
                err.to_string().contains("unknown field"),
                "field `{}` must be rejected, got: {err}",
                forbidden.0
            );
        }
    }

    #[test]
    fn attest_request_rejects_e0_e1_e2_levels_semantically() {
        // The type allows the enum, but the level must be countable for an attest.
        for (label, value) in [("E0", "E0"), ("E1", "E1"), ("E2", "E2")] {
            let mut v = minimal_attest();
            v["level"] = serde_json::json!(value);
            let req: UsageAttestRequest = serde_json::from_value(v).unwrap();
            assert!(
                !req.level.is_countable(),
                "level {label} must not be countable on the wire"
            );
        }
    }

    #[test]
    fn field_summary_keeps_three_quantities_separate() {
        let s = FieldSummary {
            generated_at_ms: 0,
            cache_seconds: 60,
            total_activated_nodes: FieldMetric {
                value: 10,
                window: "lifetime".into(),
                note: "nodes".into(),
            },
            online_instances: FieldMetric {
                value: 3,
                window: "90s".into(),
                note: "instances, not people".into(),
            },
            new_activations: FieldMetric {
                value: 1,
                window: "24h".into(),
                note: "activations".into(),
            },
            helper_nodes: FieldMetric {
                value: 4,
                window: "lifetime".into(),
                note: "helpers".into(),
            },
            methodology_url: "https://example.invalid/methodology".into(),
        };
        let v = serde_json::to_value(&s).unwrap();
        // Each metric must carry its own window and note; there is no aggregate field.
        for key in ["total_activated_nodes", "online_instances", "new_activations", "helper_nodes"] {
            assert!(v[key]["window"].is_string(), "{key} must declare a window");
            assert!(v[key]["note"].is_string(), "{key} must declare a note");
        }
        assert!(v.get("total").is_none());
    }

    #[test]
    fn schemas_export_and_describe_the_contract() {
        let schemas = exported_schemas();
        assert!(schemas.len() >= 11);
        for (name, schema) in &schemas {
            assert!(schema.is_object(), "{name} must be an object schema");
        }
        // deny_unknown_fields must surface as additionalProperties: false.
        let attest = &schemas["usage_attest_request"];
        assert_eq!(
            attest.get("additionalProperties"),
            Some(&serde_json::json!(false)),
            "attest schema must forbid unknown properties"
        );
    }

    #[test]
    fn sse_constants_match_spec() {
        assert_eq!(SSE_MAX_EVENTS_PER_MINUTE, 2);
        assert!(SSE_HEARTBEAT_MS > 0);
        assert!(SSE_REPLAY_WINDOW_MS > 0);
        assert_eq!(PROTOCOL_VERSION, "v1");
    }

    #[test]
    fn stream_hello_carries_reconnect_semantics() {
        let h = StreamHello {
            protocol_version: PROTOCOL_VERSION.into(),
            resumed_from: Some(42),
            heartbeat_ms: SSE_HEARTBEAT_MS,
            rate_limit_per_minute: SSE_MAX_EVENTS_PER_MINUTE,
            replay_window_ms: SSE_REPLAY_WINDOW_MS,
            identity_required_for_detail: true,
        };
        let v = serde_json::to_value(&h).unwrap();
        assert_eq!(v["resumed_from"], serde_json::json!(42));
        assert_eq!(v["protocol_version"], serde_json::json!("v1"));
    }
}
