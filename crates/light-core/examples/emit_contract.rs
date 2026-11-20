//! Emit the cross-platform contract: JSON Schemas and shared fixtures.
//!
//! Run: `cargo run -p light-core --example emit_contract`
//!
//! Output goes to `packages/protocol/`, which every other platform consumes.
//! The same files are read back by:
//! - `crates/light-core/tests/contract.rs`  (Rust must agree)
//! - `packages/protocol/test/conformance.test.mjs` (TypeScript must agree)
//!
//! If the two implementations ever drift, one of those two suites fails.

use light_core::evidence::{EvidencePolicy, ResponseProof};
use light_core::registry::ModelRegistry;
use light_core::{compute_light_state, knowledge_title, light_level, protocol, transmission_title};
use serde_json::{json, Value};
use std::path::PathBuf;

fn protocol_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop(); // crates/
    dir.pop(); // repo root
    dir.push("packages");
    dir.push("protocol");
    dir
}

fn main() {
    let root = protocol_dir();
    std::fs::create_dir_all(root.join("schemas")).expect("schemas dir");
    std::fs::create_dir_all(root.join("fixtures")).expect("fixtures dir");

    write_schemas(&root);
    write_title_fixtures(&root);
    write_registry_fixtures(&root);
    write_light_state_fixtures(&root);
    write_evidence_fixtures(&root);
    write_fsm_fixtures(&root);

    println!("contract written to {}", root.display());
}

fn write_json(path: PathBuf, value: &Value) {
    let text = serde_json::to_string_pretty(value).expect("serialize");
    std::fs::write(&path, format!("{text}\n")).expect("write");
    println!("  wrote {}", path.display());
}

fn write_schemas(root: &PathBuf) {
    for (name, schema) in protocol::exported_schemas() {
        write_json(root.join("schemas").join(format!("{name}.schema.json")), &schema);
    }
}

fn write_title_fixtures(root: &PathBuf) {
    let mut knowledge = Vec::new();
    let mut levels = Vec::new();
    for families in 0u32..=12 {
        knowledge.push(json!({
            "families": families,
            "slug": knowledge_title(families).slug(),
            "zh": knowledge_title(families).display_zh(),
        }));
        levels.push(json!({
            "families": families,
            "level": match light_level(families) {
                light_core::LightLevel::Dormant => "DORMANT",
                light_core::LightLevel::Faint => "FAINT",
                light_core::LightLevel::Growing => "GROWING",
                light_core::LightLevel::Blazing => "BLAZING",
                light_core::LightLevel::Deep => "DEEP",
            },
        }));
    }

    let mut transmission = Vec::new();
    for nodes in [0u32, 1, 2, 3, 4, 9, 10, 11, 29, 30, 31, 99, 100, 101, 500] {
        transmission.push(json!({
            "nodes": nodes,
            "slug": transmission_title(nodes).slug(),
            "zh": transmission_title(nodes).display_zh(),
        }));
    }

    write_json(
        root.join("fixtures").join("titles.json"),
        &json!({
            "note": "Single source of truth for both title ladders. A tier applies when value >= threshold.",
            "knowledge": knowledge,
            "transmission": transmission,
            "levels": levels,
        }),
    );
}

fn write_registry_fixtures(root: &PathBuf) {
    let registry = ModelRegistry::bundled();
    let raws = [
        "gpt-6-sol",
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-6.1-sol",
        "gpt-6-astra",
        "chatgpt-web/gpt-6-sol",
        "chatgpt-web/gpt-5.6-sol",
        "codex-auto-review",
        "",
        "deepseek-r1",
        "claude-sonnet-4-5",
        "qwen3-coder",
        "gemini-2.5-pro",
        "mystery-model-zzz",
        "my-gpt-wrapper",
    ];

    let cases: Vec<Value> = raws
        .iter()
        .map(|raw| {
            let n = registry.normalize(raw);
            let disposition = match &n.disposition {
                light_core::registry::Disposition::Family { family } => {
                    json!({"kind":"family","family":family})
                }
                light_core::registry::Disposition::TaskLabel { reason } => {
                    json!({"kind":"task_label","reason":reason})
                }
                light_core::registry::Disposition::Unknown => json!({"kind":"unknown"}),
            };
            json!({
                "raw": raw,
                "disposition": disposition,
                "family": n.family,
                "client": n.client,
            })
        })
        .collect();

    write_json(
        root.join("fixtures").join("registry_normalization.json"),
        &json!({
            "registry_semver": registry.semver(),
            "note": "Normalization must never guess. Anything absent from the registry is 'unknown'.",
            "cases": cases,
        }),
    );
}

fn write_light_state_fixtures(root: &PathBuf) {
    let cases: Vec<Value> = [
        (0u32, 0u32, 0u32, 0u32),
        (1, 0, 0, 0),
        (1, 5, 0, 0),
        (2, 0, 0, 0),
        (3, 0, 0, 1),
        (4, 0, 0, 0),
        (6, 2, 10, 3),
        (7, 0, 0, 0),
        (9, 9, 99, 100),
    ]
    .iter()
    .map(|(verified, attested, received, nodes)| {
        let s = compute_light_state(*verified, *attested, *received, *nodes);
        json!({
            "verified_families": verified,
            "attested_families": attested,
            "received_glow": received,
            "activated_nodes": nodes,
            "expected": {
                "level": match s.level {
                    light_core::LightLevel::Dormant => "DORMANT",
                    light_core::LightLevel::Faint => "FAINT",
                    light_core::LightLevel::Growing => "GROWING",
                    light_core::LightLevel::Blazing => "BLAZING",
                    light_core::LightLevel::Deep => "DEEP",
                },
                "knowledge_title": s.knowledge_title.slug(),
                "transmission_title": s.transmission_title.slug(),
            }
        })
    })
    .collect();

    write_json(
        root.join("fixtures").join("light_state.json"),
        &json!({
            "note": "attested_families (E2) and received_glow must never change level or knowledge_title.",
            "cases": cases,
        }),
    );
}

/// Every (state, event, evidence) combination the transmission machine defines.
///
/// The server implements the same machine in TypeScript; this fixture is what
/// keeps the two from drifting apart.
fn write_fsm_fixtures(root: &PathBuf) {
    use light_core::transmission::{Context, Effect, Session, SessionEvent, SessionState};

    let states = [
        ("CREATED", SessionState::Created),
        ("OFFERED", SessionState::Offered),
        ("RECEIVED", SessionState::Received),
        ("ACCEPTED", SessionState::Accepted),
        ("LEARNING", SessionState::Learning),
        ("ACTIVATED", SessionState::Activated),
        ("ACKNOWLEDGED", SessionState::Acknowledged),
        ("CANCELLED", SessionState::Cancelled),
        ("EXPIRED", SessionState::Expired),
        ("REJECTED", SessionState::Rejected),
        ("REVOKED", SessionState::Revoked),
    ];
    let events = [
        ("OFFER", SessionEvent::Offer),
        ("OPEN", SessionEvent::Open),
        ("ACCEPT", SessionEvent::Accept),
        ("CONFIRM", SessionEvent::Confirm),
        ("REJECT", SessionEvent::Reject),
        ("EXPIRE", SessionEvent::Expire),
        ("CANCEL", SessionEvent::Cancel),
        ("ACTIVATE", SessionEvent::Activate),
        ("ACKNOWLEDGE", SessionEvent::Acknowledge),
        ("REVOKE", SessionEvent::Revoke),
    ];

    let mut cases = Vec::new();
    for (state_name, state) in states {
        for (event_name, event) in events {
            for evidence in [false, true] {
                let mut session = Session::new(1_000_000 + 600_000);
                session.state = state;
                // Both confirmations are needed for ACCEPT to form a relation.
                if matches!(state, SessionState::Received) && matches!(event, SessionEvent::Accept) {
                    session.initiator_confirmed = true;
                }
                if matches!(state, SessionState::Received) && matches!(event, SessionEvent::Confirm) {
                    session.receiver_confirmed = true;
                }
                let ctx = Context {
                    now_ms: 1_000_100,
                    expires_at_ms: 1_600_000,
                    activation_evidence_ok: evidence,
                };
                let outcome = session.apply(event, &ctx);
                let (result, next, effects, replayed, error) = match outcome {
                    Ok(t) => (
                        "ok",
                        Some(format!("{:?}", t.state).to_uppercase()),
                        t.effects.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>(),
                        t.replayed,
                        Value::Null,
                    ),
                    Err(e) => {
                        let kind = match &e {
                            light_core::FsmError::Expired { .. } => "Expired",
                            light_core::FsmError::IllegalTransition { .. } => "IllegalTransition",
                            light_core::FsmError::ActivationEvidenceMissing => {
                                "ActivationEvidenceMissing"
                            }
                        };
                        ("error", None, vec![], false, json!(kind))
                    }
                };
                cases.push(json!({
                    "from": state_name,
                    "event": event_name,
                    "evidence": evidence,
                    "result": result,
                    "next": next,
                    "effects": effects,
                    "replayed": replayed,
                    "error": error,
                }));
            }
        }
    }

    write_json(
        root.join("fixtures").join("transmission_fsm.json"),
        &json!({
            "note": "Authoritative transition table. The server ports this machine verbatim; drift fails tests on both sides.",
            "states": states.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            "events": events.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            "effects": ["TokenInvalidated", "EdgeFormed", "EdgeActivated", "EchoQueued", "EdgeRevoked"],
            "cases": cases,
        }),
    );
}

fn write_evidence_fixtures(root: &PathBuf) {
    let policy = EvidencePolicy::v1();
    let cases: Vec<Value> = [
        ("gpt-6-sol", 5u32, 2u32),
        ("gpt-6-sol", 0, 0),
        ("gpt-6-sol", 0, 5),
        ("chatgpt-web/gpt-6-sol", 1, 0),
        ("codex-auto-review", 50, 10),
        ("mystery-model-zzz", 9, 9),
        ("", 3, 1),
        ("deepseek-r1", 2, 1),
    ]
    .iter()
    .map(|(raw, output, turns)| {
        let proof = ResponseProof::from_counts(*output, *turns);
        let decision = policy.evaluate(raw, &proof);
        let (kind, detail) = match &decision {
            light_core::EvidenceDecision::Countable { family, level, .. } => {
                (json!("countable"), json!({"family": family, "level": format!("{level:?}").to_uppercase()}))
            }
            light_core::EvidenceDecision::Observable { level, reason, .. } => (
                json!("observable"),
                json!({"level": format!("{level:?}").to_uppercase(), "reason": format!("{reason:?}")}),
            ),
            light_core::EvidenceDecision::Rejected { reason, .. } => (
                json!("rejected"),
                json!({"reason": format!("{reason:?}")}),
            ),
        };
        json!({
            "raw": raw,
            "output_items": output,
            "completed_turns": turns,
            "decision": kind,
            "detail": detail,
        })
    })
    .collect();

    write_json(
        root.join("fixtures").join("evidence_decisions.json"),
        &json!({
            "note": "Two gates: attribution (registry) AND occurrence (model output items). completed_turns alone is not proof.",
            "policy": {
                "model_output_item_types": EvidencePolicy::DEFAULT_OUTPUT_ITEM_TYPES,
                "non_proof_item_types": EvidencePolicy::NON_PROOF_ITEM_TYPES,
            },
            "cases": cases,
        }),
    );
}
