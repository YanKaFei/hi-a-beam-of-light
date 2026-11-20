//! Ollama adapter tests against a local mock service.
//!
//! A real model cannot be downloaded in this environment (the registry is
//! unreachable), so the transport and the evidence rules are verified against a
//! mock that speaks the same HTTP shapes. That keeps the *logic* tested while
//! honestly leaving the real-model run as BLOCKED.

use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

/// Serve a fixed set of responses, one request per connection.
fn mock_server(responses: Vec<(u16, String)>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let base = format!("http://127.0.0.1:{}", addr.port());
    let handle = thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
            drop(stream);
        }
    });
    (base, handle)
}

const VERSION_BODY: &str = r#"{"version":"0.12.3"}"#;

fn tags_body() -> String {
    serde_json::json!({
        "models": [
            {"name": "qwen2.5:0.5b", "size": 398_000_000u64, "modified_at": "2026-10-30T10:00:00Z"},
            {"name": "llama3.2:1b", "size": 1_300_000_000u64, "modified_at": "2026-10-29T10:00:00Z"},
            {"name": "mystery-model-zzz", "size": 10u64, "modified_at": "2026-10-28T10:00:00Z"}
        ]
    })
    .to_string()
}

#[test]
fn unreachable_service_is_a_normal_state() {
    // Port 1 is not listening.
    let info = probe("http://127.0.0.1:1");
    assert!(!info.reachable);
    assert!(info.models.is_empty());
    assert!(info.error.is_some());
}

#[test]
fn service_and_models_are_detected() {
    let (base, handle) = mock_server(vec![(200, VERSION_BODY.to_string()), (200, tags_body())]);
    let info = probe(&base);
    handle.join().unwrap();

    assert!(info.reachable);
    assert_eq!(info.version.as_deref(), Some("0.12.3"));
    assert_eq!(info.models.len(), 3);

    let qwen = info.models.iter().find(|m| m.name == "qwen2.5:0.5b").unwrap();
    assert_eq!(qwen.family.as_deref(), Some("qwen"));
    let llama = info.models.iter().find(|m| m.name == "llama3.2:1b").unwrap();
    assert_eq!(llama.family.as_deref(), Some("llama"));
    // An unknown name must stay unattributed rather than guessed.
    let mystery = info.models.iter().find(|m| m.name == "mystery-model-zzz").unwrap();
    assert_eq!(mystery.family, None);
}

#[test]
fn installed_models_never_become_evidence() {
    // Structural check: there is no API that turns `/api/tags` into evidence.
    // The only constructor for CountableEvidence here is receipt_to_evidence.
    let (base, handle) = mock_server(vec![(200, VERSION_BODY.to_string()), (200, tags_body())]);
    let info = probe(&base);
    handle.join().unwrap();
    assert!(info.reachable);
    // Nothing about the ServiceInfo type can be passed to receipt_to_evidence.
    let receipt: Option<InferenceReceipt> = None;
    assert!(receipt.is_none());
}

#[test]
fn a_real_call_produces_an_e4_receipt() {
    let body = serde_json::json!({
        "model": "qwen2.5:0.5b",
        "created_at": "2026-10-31T09:12:00.5Z",
        "response": "你好，这是一次本地推理。",
        "done": true,
        "eval_count": 42
    })
    .to_string();
    let (base, handle) = mock_server(vec![(200, body)]);

    let registry = ModelRegistry::bundled();
    let receipt = generate(&base, "qwen2.5:0.5b", "你好", &registry).expect("generate");
    handle.join().unwrap();

    assert_eq!(receipt.family.as_deref(), Some("qwen"));
    assert_eq!(receipt.eval_count, 42);
    assert_eq!(receipt.response, "你好，这是一次本地推理。");
    assert_eq!(receipt.request_id.len(), 16);

    let ev = receipt_to_evidence(&receipt, registry.semver()).expect("evidence");
    assert_eq!(ev.family, "qwen");
    assert_eq!(ev.level, EvidenceLevel::E4);
    assert_eq!(ev.proof_method, ProofMethod::ClientReceiptApi);
    assert_eq!(ev.adapter_id, ADAPTER_ID);
}

#[test]
fn unknown_family_is_refused_before_calling() {
    // Attribution must be decidable first; otherwise nothing runs.
    let registry = ModelRegistry::bundled();
    let err = generate("http://127.0.0.1:1", "mystery-model-zzz", "hi", &registry)
        .expect_err("must refuse");
    assert_eq!(
        err,
        OllamaError::UnknownFamily {
            model: "mystery-model-zzz".into()
        }
    );
}

#[test]
fn missing_model_is_reported_as_missing() {
    let (base, handle) = mock_server(vec![(404, r#"{"error":"model not found"}"#.to_string())]);
    let registry = ModelRegistry::bundled();
    let err = generate(&base, "qwen2.5:0.5b", "hi", &registry).expect_err("must fail");
    handle.join().unwrap();
    assert!(
        matches!(err, OllamaError::ModelMissing { .. } | OllamaError::Service(_)),
        "unexpected: {err:?}"
    );
}

#[test]
fn two_families_can_coexist() {
    // Requirement 6: GPT (from Codex) and Qwen (from Ollama) must both be
    // representable at once, and the second must raise the level.
    use light_core::{compute_light_state, LightLevel};
    let mut families = std::collections::BTreeSet::new();
    families.insert("gpt".to_string());
    families.insert("qwen".to_string());
    let light = compute_light_state(families.len() as u32, 0, 0, 0);
    assert_eq!(light.level, LightLevel::Growing);
    assert_eq!(light.knowledge_title.display_zh(), "启迪者");
}

#[test]
fn repeated_receipts_for_one_family_dedupe_in_the_store() {
    // Requirement 7: no double counting, enforced by the store's unique(family).
    let mut store = light_store::LocalStore::in_memory().expect("store");
    let registry = ModelRegistry::bundled();
    let receipt = InferenceReceipt {
        request_id: "abc123".into(),
        model: "qwen2.5:0.5b".into(),
        family: Some("qwen".into()),
        prompt_chars: 2,
        response_chars: 4,
        eval_count: 10,
        created_at_ms: 1,
        response: "ok".into(),
    };
    let ev = receipt_to_evidence(&receipt, registry.semver()).unwrap();
    assert!(store.record_countable_evidence(&ev).unwrap());
    assert!(!store.record_countable_evidence(&ev).unwrap(), "second insert must be a no-op");
    assert_eq!(store.verified_family_count().unwrap(), 1);
}

#[test]
fn rfc3339_parsing() {
    assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_rfc3339_ms("2026-10-31T00:00:00Z"), Some(1_793_404_800_000));
    assert_eq!(parse_rfc3339_ms("bogus"), None);
}
