//! Anonymous identifier derivation.
//!
//! The anonymous id is derived from the **root public key**, never from a
//! device, a machine fingerprint, or a signing key. This is what makes one
//! light seed recoverable across devices while still counting as exactly one
//! node (see ADR-010 in `docs/adr/`).
//!
//! Format: `fl1_` + 32 lowercase hex chars (128 bits of SHA-256).

use crate::codec::hex_encode;
use sha2::{Digest, Sha256};

pub const ANON_ID_PREFIX: &str = "fl1_";
const ANON_ID_DOMAIN: &[u8] = b"firstlight/anon-id/v1";
const ANON_ID_BYTES: usize = 16;

/// Derive the anonymous id from a root public key (32 bytes).
pub fn anon_id_from_root_pubkey(root_pubkey: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ANON_ID_DOMAIN);
    hasher.update(root_pubkey);
    let digest = hasher.finalize();
    format!("{ANON_ID_PREFIX}{}", hex_encode(&digest[..ANON_ID_BYTES]))
}

/// Structural validation only. Does not prove the id was derived from any
/// particular root key — that requires the epoch certificate.
pub fn is_valid_anon_id(candidate: &str) -> bool {
    let Some(rest) = candidate.strip_prefix(ANON_ID_PREFIX) else {
        return false;
    };
    rest.len() == ANON_ID_BYTES * 2 && rest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_prefixed() {
        let pk = [7u8; 32];
        let a = anon_id_from_root_pubkey(&pk);
        let b = anon_id_from_root_pubkey(&pk);
        assert_eq!(a, b);
        assert!(a.starts_with(ANON_ID_PREFIX));
        assert!(is_valid_anon_id(&a));
    }

    #[test]
    fn different_keys_differ() {
        assert_ne!(anon_id_from_root_pubkey(&[1u8; 32]), anon_id_from_root_pubkey(&[2u8; 32]));
    }

    #[test]
    fn rejects_malformed() {
        assert!(!is_valid_anon_id(""));
        assert!(!is_valid_anon_id("fl1_"));
        assert!(!is_valid_anon_id("fl2_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!is_valid_anon_id(&format!("{ANON_ID_PREFIX}{}", "A".repeat(32))));
        assert!(!is_valid_anon_id(&format!("{ANON_ID_PREFIX}{}", "a".repeat(31))));
    }
}
