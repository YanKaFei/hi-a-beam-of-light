//! Cross-language vector check for fl.usage.credential/v1.
//!
//! The vector is produced by the Node verifier. Rust does not implement JCS yet, so this
//! test proves the *signature* covers the agreed bytes — not that Rust could reproduce
//! them. Stated here so the gap is not mistaken for full interop.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

const VECTOR: &str = include_str!("../../../packages/protocol/vectors/credential-v1.json");

fn hex_to_vec(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn doc() -> serde_json::Value {
    serde_json::from_str(VECTOR).expect("vector json")
}

#[test]
fn the_node_signature_verifies_over_the_agreed_bytes() {
    let v = doc();
    let pk: [u8; 32] = hex_to_vec(v["pubkey_hex"].as_str().unwrap()).try_into().unwrap();
    let sig = hex_to_vec(v["signature_hex"].as_str().unwrap());
    let input = hex_to_vec(v["signing_input_hex"].as_str().unwrap());

    let key = VerifyingKey::from_bytes(&pk).expect("public key");
    let sig = Signature::from_slice(&sig).expect("signature");
    assert!(key.verify(&input, &sig).is_ok(), "vector signature must verify");
}

#[test]
fn the_signing_input_starts_with_the_domain_tag() {
    let v = doc();
    let bytes = hex_to_vec(v["signing_input_hex"].as_str().unwrap());
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.starts_with("firstlight/usage-credential/v1\n"),
        "domain separation is what stops a credential being replayed as another kind of signature"
    );
}

#[test]
fn canonical_json_orders_keys_and_has_no_whitespace() {
    let v = doc();
    let canon = v["canonical_json"].as_str().unwrap();
    assert!(!canon.contains(' '), "JCS emits no insignificant whitespace");
    let adapter = canon.find("\"adapter_id\"").unwrap();
    let subject = canon.find("\"subject_identity_id\"").unwrap();
    assert!(adapter < subject, "keys must be lexicographically ordered");
}

#[test]
fn tampered_payloads_do_not_verify() {
    let v = doc();
    let pk: [u8; 32] = hex_to_vec(v["pubkey_hex"].as_str().unwrap()).try_into().unwrap();
    let sig = Signature::from_slice(&hex_to_vec(v["signature_hex"].as_str().unwrap())).unwrap();
    let key = VerifyingKey::from_bytes(&pk).unwrap();
    let original = hex_to_vec(v["signing_input_hex"].as_str().unwrap());

    for patch in ["claude", "INDEPENDENTLY_CORROBORATED"] {
        let mut tampered = String::from_utf8(original.clone()).unwrap().replace("gpt", patch)
            .replace("PLUGIN_OBSERVED", patch);
        if tampered == String::from_utf8(original.clone()).unwrap() {
            tampered = String::from_utf8(original.clone()).unwrap().replace("ollama-proxy", patch);
        }
        assert!(key.verify(tampered.as_bytes(), &sig).is_err(), "tampered input must not verify");
    }
}
