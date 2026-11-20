//! The desktop's light seed: load, create, recover.
//!
//! This is the piece F2 deferred, and the rules here are deliberately strict
//! because losing a seed means losing every transmission ever attached to it:
//!
//! * A record without a seed is an **error**, never a new identity. Silently
//!   generating a replacement would orphan all history while looking like
//!   success.
//! * A seed without a record is recoverable: the record is rebuilt from the seed
//!   rather than a fresh identity being made.
//! * The record and the seed must agree on the anon id, or nothing proceeds.
//!
//! Recovery uses the F1 recovery code (base32 of the root seed plus a checksum,
//! 256 bits of entropy). It is a secret that must be stored by the user; the app
//! never uploads it.

use std::path::{Path, PathBuf};

use light_core::identity::{RecoveryCode, RootIdentity};
use serde::{Deserialize, Serialize};

use crate::identity_store::{Backend, IdentityStore, StoreError};

pub const RECORD_FILE: &str = "identity.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityRecord {
    pub anon_id: String,
    pub root_pubkey_hex: String,
    pub epoch: u32,
    pub created_at_ms: i64,
    pub backend: String,
    /// True once this identity has been registered with a transmission server.
    pub registered: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("系统凭据库拒绝了访问：{0}。历史光点数据仍然安全，程序不会生成新身份。")]
    AccessDenied(String),
    #[error(
        "找到了身份记录（{anon_id}），但系统凭据库中没有对应的私钥。\
         为避免覆盖历史，程序不会自动生成新身份。请使用恢复凭据，或明确重置。"
    )]
    SeedMissingButRecordExists { anon_id: String },
    #[error("身份记录损坏：{0}")]
    RecordCorrupt(String),
    #[error("读取到的私钥与身份记录不一致（记录 {record}，私钥 {derived}）。已停止操作。")]
    IdentityConflict { record: String, derived: String },
    #[error("恢复凭据无效。请检查是否完整复制（区分大小写与连字符）。")]
    RecoveryInvalid,
    #[error("此设备已有一个不同的光种（{existing}）。恢复会覆盖它，需要明确确认。")]
    RecoveryWouldReplace { existing: String },
    #[error("凭据存储错误：{0}")]
    Store(String),
}

impl From<StoreError> for IdentityError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::AccessDenied(m) => IdentityError::AccessDenied(m),
            other => IdentityError::Store(other.to_string()),
        }
    }
}

/// Snapshot handed to the UI: never includes the seed.
#[derive(Debug, Clone, Serialize)]
pub struct IdentityView {
    pub anon_id: String,
    pub root_pubkey_hex: String,
    pub epoch: u32,
    pub created_at_ms: i64,
    pub backend: Backend,
    pub registered: bool,
    /// Whether the seed is currently readable. False means the UI must offer
    /// recovery rather than pretend everything is fine.
    pub seed_available: bool,
}

pub fn record_path(dir: &Path) -> PathBuf {
    dir.join(RECORD_FILE)
}

pub fn read_record(dir: &Path) -> Result<Option<IdentityRecord>, IdentityError> {
    let path = record_path(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(IdentityError::RecordCorrupt(e.to_string())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| IdentityError::RecordCorrupt(e.to_string()))
}

fn write_record(dir: &Path, record: &IdentityRecord) -> Result<(), IdentityError> {
    std::fs::create_dir_all(dir).map_err(|e| IdentityError::Store(e.to_string()))?;
    let text = serde_json::to_string_pretty(record).map_err(|e| IdentityError::Store(e.to_string()))?;
    let tmp = record_path(dir).with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| IdentityError::Store(e.to_string()))?;
    std::fs::rename(&tmp, record_path(dir)).map_err(|e| IdentityError::Store(e.to_string()))?;
    Ok(())
}

fn record_from(identity: &RootIdentity, epoch: u32, created_at_ms: i64, backend: Backend) -> IdentityRecord {
    IdentityRecord {
        anon_id: identity.anon_id(),
        root_pubkey_hex: crate::identity_store::hex_encode(identity.root_public_key().as_bytes()),
        epoch,
        created_at_ms,
        backend: format!("{backend:?}"),
        registered: false,
    }
}

/// Load the identity, or create one on a genuinely first run.
///
/// Returns `(identity, record, created_now)`.
pub fn load_or_create(
    store: &dyn IdentityStore,
    dir: &Path,
    now_ms: i64,
) -> Result<(RootIdentity, IdentityRecord, bool), IdentityError> {
    let record = read_record(dir)?;
    let seed = store.load_seed()?;

    match (record, seed) {
        // Normal path.
        (Some(record), Some(seed)) => {
            let identity = RootIdentity::from_seed(seed);
            let derived = identity.anon_id();
            if derived != record.anon_id {
                return Err(IdentityError::IdentityConflict {
                    record: record.anon_id.clone(),
                    derived,
                });
            }
            Ok((identity, record, false))
        }

        // The seed survived but the record did not: rebuild, never regenerate.
        (None, Some(seed)) => {
            let identity = RootIdentity::from_seed(seed);
            let record = record_from(&identity, 1, now_ms, store.backend());
            write_record(dir, &record)?;
            Ok((identity, record, false))
        }

        // A record without a seed must not become a new identity.
        (Some(record), None) => Err(IdentityError::SeedMissingButRecordExists {
            anon_id: record.anon_id,
        }),

        // First run.
        (None, None) => {
            let identity = RootIdentity::generate();
            store.save_seed(&seed_of(&identity))?;
            let record = record_from(&identity, 1, now_ms, store.backend());
            write_record(dir, &record)?;
            Ok((identity, record, true))
        }
    }
}

fn seed_of(identity: &RootIdentity) -> [u8; 32] {
    identity.seed()
}

/// Export the recovery code for this identity.
pub fn export_recovery(identity: &RootIdentity) -> String {
    identity.recovery_code().as_str().to_string()
}

/// Import a recovery code, restoring the same anon id.
///
/// `allow_replace` must be true to overwrite a different existing identity.
pub fn import_recovery(
    store: &dyn IdentityStore,
    dir: &Path,
    code: &str,
    now_ms: i64,
    allow_replace: bool,
) -> Result<(RootIdentity, IdentityRecord), IdentityError> {
    let parsed = RecoveryCode::parse(code).map_err(|_| IdentityError::RecoveryInvalid)?;
    let seed = parsed.seed().map_err(|_| IdentityError::RecoveryInvalid)?;
    let identity = RootIdentity::from_seed(seed);

    if let Some(existing) = read_record(dir)? {
        if existing.anon_id != identity.anon_id() && !allow_replace {
            return Err(IdentityError::RecoveryWouldReplace {
                existing: existing.anon_id,
            });
        }
    }

    store.save_seed(&seed)?;
    let mut record = record_from(&identity, 1, now_ms, store.backend());
    record.registered = false;
    write_record(dir, &record)?;
    Ok((identity, record))
}

/// Mark the identity as registered with a server.
pub fn mark_registered(dir: &Path) -> Result<(), IdentityError> {
    if let Some(mut record) = read_record(dir)? {
        record.registered = true;
        write_record(dir, &record)?;
    }
    Ok(())
}

/// Whether a seed is currently readable, without returning it.
pub fn seed_available(store: &dyn IdentityStore) -> bool {
    matches!(store.load_seed(), Ok(Some(_)))
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;

/// Test-only: rewrite the record so the conflict path can be exercised.
#[cfg(test)]
pub fn write_record_for_test(dir: &Path, record: &IdentityRecord) {
    write_record(dir, record).unwrap();
}
