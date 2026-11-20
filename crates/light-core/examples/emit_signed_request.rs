//! Emit a Rust-produced signed request so the TypeScript server can prove it
//! verifies real client signatures, not just its own test helper's.
//!
//! Run: `cargo run -p light-core --example emit_signed_request`

use light_core::identity::RootIdentity;
use serde_json::json;
use std::path::PathBuf;

fn main() {
    // Fixed seed: the fixture is deterministic and contains no real user key.
    let id = RootIdentity::from_seed([0x5a; 32]);
    let anon_id = id.anon_id();
    let cert = id.epoch_certificate(1);

    let method = "POST";
    let path = "/v1/transmissions";
    let body = r#"{"channel":"QR"}"#;
    let body_hash = {
        use sha2::{Digest, Sha256};
        let d = Sha256::digest(body.as_bytes());
        d.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let timestamp: i64 = 1_761_868_800_000;
    let nonce = "rustfixture-nonce-0001";
    let canonical = format!("FL1\n{method}\n{path}\n{body_hash}\n{timestamp}\n{nonce}");
    let signature = id.sign_with_epoch(1, canonical.as_bytes());

    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out.pop();
    out.pop();
    out.push("packages");
    out.push("protocol");
    out.push("fixtures");
    std::fs::create_dir_all(&out).expect("fixtures dir");
    let file = out.join("signed_request_rust.json");

    let payload = json!({
        "note": "Produced by crates/light-core (Rust). The TypeScript server must verify this signature, which proves the two implementations agree on key derivation and canonicalisation.",
        "anon_id": anon_id,
        "epoch": 1,
        "cert": {
            "epoch": cert.epoch,
            "root_pubkey_hex": cert.root_pubkey_hex,
            "epoch_pubkey_hex": cert.epoch_pubkey_hex,
            "signature_hex": cert.signature_hex
        },
        "request": {
            "method": method,
            "path": path,
            "body": body,
            "body_sha256_hex": body_hash,
            "timestamp": timestamp,
            "nonce": nonce
        },
        "signature_hex": signature.to_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>()
    });
    std::fs::write(&file, serde_json::to_string_pretty(&payload).unwrap() + "\n").expect("write");
    println!("wrote {}", file.display());
}
