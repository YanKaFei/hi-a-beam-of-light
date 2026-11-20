//! Local identity: root seed, per-epoch signing keys, recovery code.
//!
//! Design (see `docs/adr/ADR-010-node-identity.md`):
//!
//! - The **root seed** is 32 random bytes. It lives in the OS credential store
//!   on a real device; in this crate it is only ever held in memory.
//! - The **anonymous id** is derived from the root public key, so it survives
//!   key rotation and device migration.
//! - **Signing keys are per epoch**, derived deterministically from the root
//!   seed. Rotating means publishing a new epoch certificate signed by the root
//!   key — the anonymous id does not change.
//! - The **recovery code** encodes the root seed. Because the seed is a signing
//!   authority rather than a session key, a leaked recovery code can be
//!   invalidated by rotating the epoch (mitigates F0 gap G-10).

use crate::codec::{base32_decode, base32_encode, hex_encode};
use crate::ids::anon_id_from_root_pubkey;
use crate::CoreError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use sha2::{Digest, Sha256};

const EPOCH_KEY_DOMAIN: &[u8] = b"firstlight/epoch-key/v1";
const CERT_DOMAIN: &[u8] = b"firstlight/epoch-cert/v1";
const RECOVERY_PREFIX: &str = "FL1-";

/// Lifecycle of a light seed (state machine in `docs/05-state-machines.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityState {
    /// Instantaneous: keys being generated.
    New,
    /// Seed created, never contacted a server. Fully usable offline.
    LocalOnly,
    /// Root key registered; eligible to participate in the public field.
    Registered,
    /// A newer epoch exists; the previous epoch may no longer sign.
    Rotated,
    /// User destroyed the identity server-side.
    Revoked,
}

/// A per-epoch signing key certificate, verifiable from the anon id alone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EpochCertificate {
    pub epoch: u32,
    /// Root public key, hex. `anon_id` must equal `hash(root_pubkey)`.
    pub root_pubkey_hex: String,
    /// Epoch public key, hex.
    pub epoch_pubkey_hex: String,
    /// Root signature over the epoch binding message, hex.
    pub signature_hex: String,
}

fn epoch_binding_message(root_pubkey: &[u8], epoch: u32, epoch_pubkey: &[u8]) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(CERT_DOMAIN);
    msg.extend_from_slice(root_pubkey);
    msg.extend_from_slice(&epoch.to_le_bytes());
    msg.extend_from_slice(epoch_pubkey);
    msg
}

/// The device-held identity root.
///
/// Deliberately not `Clone`: a root seed should not be casually duplicated.
pub struct RootIdentity {
    seed: [u8; 32],
}

/// Redacting `Debug`: the seed must never reach a log line.
impl std::fmt::Debug for RootIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootIdentity")
            .field("seed", &"<redacted>")
            .finish()
    }
}

impl RootIdentity {
    /// Generate a fresh identity from the OS CSPRNG.
    pub fn generate() -> Self {
        let key = SigningKey::generate(&mut OsRng);
        Self { seed: key.to_bytes() }
    }

    /// Deterministic constructor — for tests and for recovery-code restore.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self { seed }
    }

    pub fn root_signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.seed)
    }

    pub fn root_public_key(&self) -> VerifyingKey {
        self.root_signing_key().verifying_key()
    }

    /// Stable across devices and across epochs.
    pub fn anon_id(&self) -> String {
        anon_id_from_root_pubkey(self.root_public_key().as_bytes())
    }

    /// Deterministic per-epoch signing key.
    pub fn epoch_signing_key(&self, epoch: u32) -> SigningKey {
        let mut hasher = Sha256::new();
        hasher.update(EPOCH_KEY_DOMAIN);
        hasher.update(self.seed);
        hasher.update(epoch.to_le_bytes());
        let digest = hasher.finalize();
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&digest);
        SigningKey::from_bytes(&key_bytes)
    }

    pub fn epoch_public_key(&self, epoch: u32) -> VerifyingKey {
        self.epoch_signing_key(epoch).verifying_key()
    }

    /// Prove to a server that `epoch`'s key belongs to this anon id.
    pub fn epoch_certificate(&self, epoch: u32) -> EpochCertificate {
        let root = self.root_signing_key();
        let root_pubkey = root.verifying_key();
        let epoch_pubkey = self.epoch_public_key(epoch);
        let msg = epoch_binding_message(root_pubkey.as_bytes(), epoch, epoch_pubkey.as_bytes());
        let signature = root.sign(&msg);
        EpochCertificate {
            epoch,
            root_pubkey_hex: hex_encode(root_pubkey.as_bytes()),
            epoch_pubkey_hex: hex_encode(epoch_pubkey.as_bytes()),
            signature_hex: hex_encode(&signature.to_bytes()),
        }
    }

    /// The raw root seed.
    ///
    /// This exists for exactly one legitimate consumer: the OS credential store,
    /// which has to persist the seed to make the identity survive restarts.
    /// Nothing else should call it, and nothing may log the result.
    pub fn seed(&self) -> [u8; 32] {
        self.seed
    }

    /// Export a recovery code. Treat the result as a secret.
    pub fn recovery_code(&self) -> RecoveryCode {
        RecoveryCode::encode(&self.seed)
    }

    /// Sign arbitrary bytes with the given epoch's key.
    pub fn sign_with_epoch(&self, epoch: u32, message: &[u8]) -> Signature {
        self.epoch_signing_key(epoch).sign(message)
    }
}

/// A human-transportable encoding of the root seed (base32 + checksum).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryCode(String);

impl RecoveryCode {
    fn checksum(seed: &[u8; 32]) -> u8 {
        let mut hasher = Sha256::new();
        hasher.update(b"firstlight/recovery-checksum/v1");
        hasher.update(seed);
        hasher.finalize()[0]
    }

    fn encode(seed: &[u8; 32]) -> Self {
        let mut payload = seed.to_vec();
        payload.push(Self::checksum(seed));
        RecoveryCode(format!("{RECOVERY_PREFIX}{}", base32_encode(&payload)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse a user-supplied recovery code and verify its checksum.
    pub fn parse(text: &str) -> Result<Self, CoreError> {
        let candidate = RecoveryCode(text.trim().to_string());
        candidate.clone().to_identity()?;
        Ok(candidate)
    }

    /// The seed this code carries. Callers must treat it as a secret.
    pub fn seed(&self) -> Result<[u8; 32], CoreError> {
        let identity = self.clone().to_identity()?;
        Ok(identity.seed())
    }

    /// Parse and verify. Returns the recovered identity.
    pub fn to_identity(&self) -> Result<RootIdentity, CoreError> {
        let body = self.0.strip_prefix(RECOVERY_PREFIX).ok_or(CoreError::RecoveryCodeInvalid)?;
        let bytes = base32_decode(body).ok_or(CoreError::RecoveryCodeInvalid)?;
        if bytes.len() != 33 {
            return Err(CoreError::RecoveryCodeInvalid);
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes[..32]);
        if bytes[32] != Self::checksum(&seed) {
            return Err(CoreError::RecoveryCodeInvalid);
        }
        Ok(RootIdentity::from_seed(seed))
    }
}

/// Verify an epoch certificate against a claimed anon id.
///
/// This is the server-side check that makes multi-device identity safe: the
/// certificate is self-contained, and the anon id must match the hash of the
/// embedded root public key.
pub fn verify_epoch_certificate(anon_id: &str, cert: &EpochCertificate) -> Result<(), CoreError> {
    let root_pubkey = hex_to_array32(&cert.root_pubkey_hex).ok_or(CoreError::EpochMismatch)?;
    let epoch_pubkey = hex_to_array32(&cert.epoch_pubkey_hex).ok_or(CoreError::EpochMismatch)?;

    if anon_id_from_root_pubkey(&root_pubkey) != anon_id {
        return Err(CoreError::EpochMismatch);
    }
    if epoch_pubkey == root_pubkey {
        // A certificate must not just echo the root key as the epoch key.
        return Err(CoreError::EpochMismatch);
    }

    let root_vk = VerifyingKey::from_bytes(&root_pubkey).map_err(|_| CoreError::SignatureInvalid)?;
    let sig_bytes = hex_to_array64(&cert.signature_hex).ok_or(CoreError::SignatureInvalid)?;
    let signature = Signature::from_bytes(&sig_bytes);
    let msg = epoch_binding_message(&root_pubkey, cert.epoch, &epoch_pubkey);
    root_vk.verify(&msg, &signature).map_err(|_| CoreError::SignatureInvalid)?;
    Ok(())
}

fn hex_to_array32(hex: &str) -> Option<[u8; 32]> {
    let bytes = hex_to_vec(hex)?;
    bytes.try_into().ok()
}

fn hex_to_array64(hex: &str) -> Option<[u8; 64]> {
    let bytes = hex_to_vec(hex)?;
    bytes.try_into().ok()
}

fn hex_to_vec(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = (chunk[0] as char).to_digit(16)?;
        let lo = (chunk[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::is_valid_anon_id;

    fn fixed() -> RootIdentity {
        RootIdentity::from_seed([42u8; 32])
    }

    #[test]
    fn generated_identity_is_well_formed() {
        let id = RootIdentity::generate();
        assert!(is_valid_anon_id(&id.anon_id()));
        // Two generated identities must differ.
        assert_ne!(id.anon_id(), RootIdentity::generate().anon_id());
    }

    #[test]
    fn anon_id_is_stable_across_epochs() {
        let id = fixed();
        let before = id.anon_id();
        let _ = id.epoch_signing_key(7);
        assert_eq!(before, id.anon_id());
        assert!(is_valid_anon_id(&before));
    }

    #[test]
    fn epoch_keys_differ_per_epoch_and_are_deterministic() {
        let id = fixed();
        assert_ne!(
            id.epoch_public_key(1).as_bytes(),
            id.epoch_public_key(2).as_bytes()
        );
        assert_eq!(id.epoch_public_key(3).as_bytes(), id.epoch_public_key(3).as_bytes());
    }

    #[test]
    fn certificate_verifies_and_rejects_wrong_identity() {
        let id = fixed();
        let cert = id.epoch_certificate(1);
        verify_epoch_certificate(&id.anon_id(), &cert).expect("valid cert");

        let other = RootIdentity::from_seed([9u8; 32]);
        assert_eq!(
            verify_epoch_certificate(&other.anon_id(), &cert),
            Err(CoreError::EpochMismatch)
        );
    }

    #[test]
    fn certificate_rejects_epoch_echo_of_root_key() {
        let id = fixed();
        let root = id.root_public_key();
        let mut cert = id.epoch_certificate(1);
        cert.epoch_pubkey_hex = hex_encode(root.as_bytes());
        // Signature no longer covers the swapped key either way.
        assert!(verify_epoch_certificate(&id.anon_id(), &cert).is_err());
    }

    #[test]
    fn tampered_certificate_fails() {
        let id = fixed();
        let mut cert = id.epoch_certificate(1);
        let mut sig = cert.signature_hex.clone().into_bytes();
        sig[0] = if sig[0] == b'0' { b'1' } else { b'0' };
        cert.signature_hex = String::from_utf8(sig).unwrap();
        assert!(verify_epoch_certificate(&id.anon_id(), &cert).is_err());
    }

    #[test]
    fn recovery_roundtrip_preserves_identity_and_epoch_keys() {
        let id = fixed();
        let code = id.recovery_code();
        assert!(code.as_str().starts_with(RECOVERY_PREFIX));
        let restored = code.to_identity().expect("restore");
        assert_eq!(restored.anon_id(), id.anon_id());
        assert_eq!(restored.epoch_public_key(1).as_bytes(), id.epoch_public_key(1).as_bytes());
    }

    #[test]
    fn recovery_code_rejects_corruption() {
        let id = fixed();
        let code = id.recovery_code();
        let mut chars: Vec<char> = code.as_str().chars().collect();
        let idx = 20;
        chars[idx] = if chars[idx] == 'A' { 'B' } else { 'A' };
        let corrupted: String = chars.into_iter().collect();
        assert_eq!(
            RecoveryCode(corrupted).to_identity().unwrap_err(),
            CoreError::RecoveryCodeInvalid
        );
        assert_eq!(
            RecoveryCode("nope".into()).to_identity().unwrap_err(),
            CoreError::RecoveryCodeInvalid
        );
        assert_eq!(
            RecoveryCode("FL1-".into()).to_identity().unwrap_err(),
            CoreError::RecoveryCodeInvalid
        );
    }
}
