//! Identity storage backends.
//!
//! The business logic never talks to the macOS Keychain directly; it talks to
//! [`IdentityStore`]. That keeps three things possible:
//!
//! 1. the same identity layer works on Windows (Credential Manager) without
//!    touching call sites;
//! 2. the failure modes (denied access, missing item) are testable without a GUI;
//! 3. an explicit, clearly-labelled development backend can run where no keychain
//!    is reachable (like this sandbox).
//!
//! The development backend is **never** selected implicitly.

use std::path::{Path, PathBuf};

pub const KEYCHAIN_SERVICE: &str = "firstlight.identity";
pub const KEYCHAIN_ACCOUNT: &str = "root-seed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// OS credential store. The production path.
    Keychain,
    /// Plain file with 0600 permissions. Development only, opt-in.
    FileDev,
    /// Process memory. Tests only.
    Memory,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("the system credential store denied access: {0}")]
    AccessDenied(String),
    #[error("no identity seed is stored")]
    NotFound,
    #[error("credential store error: {0}")]
    Other(String),
    #[error("this backend is not available on this platform")]
    Unsupported,
}

pub trait IdentityStore: Send + Sync {
    fn load_seed(&self) -> Result<Option<[u8; 32]>, StoreError>;
    fn save_seed(&self, seed: &[u8; 32]) -> Result<(), StoreError>;
    fn delete_seed(&self) -> Result<(), StoreError>;
    fn backend(&self) -> Backend;
}

// ---------------------------------------------------------------------------
// Keychain (macOS). Windows maps to the Credential Manager through the same
// trait; that implementation is not written yet and must not be faked.
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub struct KeychainStore;

#[cfg(target_os = "macos")]
impl KeychainStore {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
impl IdentityStore for KeychainStore {
    fn load_seed(&self) -> Result<Option<[u8; 32]>, StoreError> {
        use security_framework::passwords::get_generic_password;
        match get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
            Ok(bytes) => {
                if bytes.len() != 32 {
                    return Err(StoreError::Other(format!(
                        "stored seed has {} bytes, expected 32",
                        bytes.len()
                    )));
                }
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&bytes);
                Ok(Some(seed))
            }
            Err(e) => {
                let text = e.to_string();
                if text.contains("could not be found") || text.contains("-25300") {
                    Ok(None)
                } else {
                    Err(StoreError::AccessDenied(text))
                }
            }
        }
    }

    fn save_seed(&self, seed: &[u8; 32]) -> Result<(), StoreError> {
        use security_framework::passwords::set_generic_password;
        set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, seed)
            .map_err(|e| StoreError::AccessDenied(e.to_string()))
    }

    fn delete_seed(&self) -> Result<(), StoreError> {
        use security_framework::passwords::delete_generic_password;
        match delete_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
            Ok(()) => Ok(()),
            Err(e) => {
                let text = e.to_string();
                if text.contains("could not be found") || text.contains("-25300") {
                    Ok(())
                } else {
                    Err(StoreError::AccessDenied(text))
                }
            }
        }
    }

    fn backend(&self) -> Backend {
        Backend::Keychain
    }
}

// ---------------------------------------------------------------------------
// Development file backend. Opt-in only, and it says so.
// ---------------------------------------------------------------------------

pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("dev-identity.seed"),
        }
    }
}

impl IdentityStore for FileStore {
    fn load_seed(&self) -> Result<Option<[u8; 32]>, StoreError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(StoreError::Other(format!("read seed file: {e}"))),
        };
        let bytes = hex_decode(text.trim())
            .ok_or_else(|| StoreError::Other("seed file is not valid hex".into()))?;
        if bytes.len() != 32 {
            return Err(StoreError::Other("seed file has the wrong length".into()));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        Ok(Some(seed))
    }

    fn save_seed(&self, seed: &[u8; 32]) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StoreError::Other(format!("create dir: {e}")))?;
        }
        std::fs::write(&self.path, hex_encode(seed))
            .map_err(|e| StoreError::Other(format!("write seed: {e}")))?;
        // Restrict permissions: this file is a secret.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn delete_seed(&self) -> Result<(), StoreError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::Other(format!("delete seed: {e}"))),
        }
    }

    fn backend(&self) -> Backend {
        Backend::FileDev
    }
}

// ---------------------------------------------------------------------------
// Memory backend for tests.
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct MemoryStore {
    seed: std::sync::Mutex<Option<[u8; 32]>>,
    /// Simulate a denied keychain, for the failure-path tests.
    pub deny: std::sync::atomic::AtomicBool,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn denying() -> Self {
        let s = Self::default();
        s.deny.store(true, std::sync::atomic::Ordering::SeqCst);
        s
    }
}

impl IdentityStore for MemoryStore {
    fn load_seed(&self) -> Result<Option<[u8; 32]>, StoreError> {
        if self.deny.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::AccessDenied("simulated denial".into()));
        }
        Ok(*self.seed.lock().unwrap())
    }
    fn save_seed(&self, seed: &[u8; 32]) -> Result<(), StoreError> {
        if self.deny.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::AccessDenied("simulated denial".into()));
        }
        *self.seed.lock().unwrap() = Some(*seed);
        Ok(())
    }
    fn delete_seed(&self) -> Result<(), StoreError> {
        *self.seed.lock().unwrap() = None;
        Ok(())
    }
    fn backend(&self) -> Backend {
        Backend::Memory
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.pop();
        d.pop();
        d.pop();
        d.push("target");
        d.push("identity-store-tests");
        d.push(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn file_store_roundtrips_and_restricts_permissions() {
        let dir = tmp("file");
        let store = FileStore::new(&dir);
        assert!(store.load_seed().unwrap().is_none());

        let seed = [7u8; 32];
        store.save_seed(&seed).unwrap();
        assert_eq!(store.load_seed().unwrap(), Some(seed));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("dev-identity.seed"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "the seed file must not be world readable");
        }

        store.delete_seed().unwrap();
        assert!(store.load_seed().unwrap().is_none());
    }

    #[test]
    fn file_store_detects_corruption() {
        let dir = tmp("corrupt");
        let store = FileStore::new(&dir);
        std::fs::write(dir.join("dev-identity.seed"), "not-hex!!").unwrap();
        assert!(matches!(store.load_seed(), Err(StoreError::Other(_))));

        std::fs::write(dir.join("dev-identity.seed"), "abcd").unwrap();
        assert!(matches!(store.load_seed(), Err(StoreError::Other(_))));
    }

    #[test]
    fn memory_store_denial_is_reported() {
        let store = MemoryStore::denying();
        assert!(matches!(store.load_seed(), Err(StoreError::AccessDenied(_))));
    }

    #[test]
    fn hex_helpers() {
        assert_eq!(hex_encode(&[0x00, 0xff]), "00ff");
        assert_eq!(hex_decode("00ff"), Some(vec![0x00, 0xff]));
        assert_eq!(hex_decode("0"), None);
        assert_eq!(hex_decode("zz"), None);
    }
}
