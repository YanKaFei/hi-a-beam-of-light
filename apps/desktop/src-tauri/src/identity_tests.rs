//! Identity lifecycle tests: A01–A06 of the F3.5 acceptance matrix.

use super::*;
use crate::identity_store::MemoryStore;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.pop();
    d.pop();
    d.pop();
    d.push("target");
    d.push("identity-tests");
    d.push(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a01_first_run_creates_exactly_one_seed() {
    let dir = tmp("first");
    let store = MemoryStore::new();
    let (id, record, created) = load_or_create(&store, &dir, 1000).unwrap();
    assert!(created, "a fresh install must create an identity");
    assert_eq!(record.anon_id, id.anon_id());
    assert!(record.anon_id.starts_with("fl1_"));
    assert_eq!(store.load_seed().unwrap().map(|s| s.len()), Some(32));
    assert!(record_path(&dir).exists());
}

#[test]
fn a02_restart_yields_the_same_public_identity() {
    let dir = tmp("restart");
    let store = MemoryStore::new();
    let (first, rec1, _) = load_or_create(&store, &dir, 1000).unwrap();
    let (second, rec2, created) = load_or_create(&store, &dir, 2000).unwrap();
    assert!(!created, "a restart must not create a new identity");
    assert_eq!(first.anon_id(), second.anon_id());
    assert_eq!(first.root_public_key().as_bytes(), second.root_public_key().as_bytes());
    assert_eq!(rec1, rec2);
}

#[test]
fn a03_missing_record_is_rebuilt_not_regenerated() {
    // Simulates an upgrade that lost the metadata file but kept the key.
    let dir = tmp("no-record");
    let store = MemoryStore::new();
    let (original, _, _) = load_or_create(&store, &dir, 1000).unwrap();
    std::fs::remove_file(record_path(&dir)).unwrap();

    let (restored, record, created) = load_or_create(&store, &dir, 2000).unwrap();
    assert!(!created, "the seed still exists, so this is not a first run");
    assert_eq!(restored.anon_id(), original.anon_id());
    assert_eq!(record.anon_id, original.anon_id());
}

#[test]
fn a03_record_without_seed_refuses_to_invent_a_new_identity() {
    // This is the dangerous case: metadata present, key gone.
    let dir = tmp("no-seed");
    let store = MemoryStore::new();
    let (original, _, _) = load_or_create(&store, &dir, 1000).unwrap();
    store.delete_seed().unwrap();

    let err = load_or_create(&store, &dir, 2000).unwrap_err();
    match err {
        IdentityError::SeedMissingButRecordExists { anon_id } => {
            assert_eq!(anon_id, original.anon_id());
        }
        other => panic!("must not silently regenerate, got {other}"),
    }
    // And it must not have written a new seed behind our back.
    assert!(store.load_seed().unwrap().is_none());
}

#[test]
fn a04_denied_credential_store_is_reported_and_nothing_is_overwritten() {
    let dir = tmp("denied");
    let store = MemoryStore::denying();
    let err = load_or_create(&store, &dir, 1000).unwrap_err();
    assert!(matches!(err, IdentityError::AccessDenied(_)));
    assert!(!record_path(&dir).exists(), "must not write a record it cannot back with a key");
}

#[test]
fn a05_recovery_restores_the_same_identity() {
    let dir = tmp("recover");
    let store = MemoryStore::new();
    let (original, _, _) = load_or_create(&store, &dir, 1000).unwrap();
    let code = export_recovery(&original);

    // New device: no record, no seed.
    let dir2 = tmp("recover-2");
    let store2 = MemoryStore::new();
    let (restored, record) = import_recovery(&store2, &dir2, &code, 2000, false).unwrap();
    assert_eq!(restored.anon_id(), original.anon_id());
    assert_eq!(
        restored.root_public_key().as_bytes(),
        original.root_public_key().as_bytes()
    );
    assert_eq!(record.anon_id, original.anon_id());

    // Import into the same directory is idempotent.
    let (again, _) = import_recovery(&store, &dir, &code, 3000, false).unwrap();
    assert_eq!(again.anon_id(), original.anon_id());
}

#[test]
fn a06_wrong_or_corrupted_recovery_codes_are_refused() {
    let dir = tmp("bad-recovery");
    let store = MemoryStore::new();
    let (original, _, _) = load_or_create(&store, &dir, 1000).unwrap();
    let good = export_recovery(&original);

    assert!(matches!(
        import_recovery(&store, &dir, "not-a-code", 1, true),
        Err(IdentityError::RecoveryInvalid)
    ));

    // Flip one character inside the payload.
    let mut chars: Vec<char> = good.chars().collect();
    let idx = chars.len() - 3;
    chars[idx] = if chars[idx] == 'A' { 'B' } else { 'A' };
    let corrupted: String = chars.into_iter().collect();
    assert!(matches!(
        import_recovery(&store, &dir, &corrupted, 1, true),
        Err(IdentityError::RecoveryInvalid)
    ));

    // Truncated.
    assert!(matches!(
        import_recovery(&store, &dir, &good[..good.len() - 4], 1, true),
        Err(IdentityError::RecoveryInvalid)
    ));
}

#[test]
fn recovery_over_a_different_identity_requires_explicit_confirmation() {
    let dir = tmp("conflict");
    let store = MemoryStore::new();
    let (existing, _, _) = load_or_create(&store, &dir, 1000).unwrap();

    let other_dir = tmp("conflict-other");
    let other_store = MemoryStore::new();
    let (other, _, _) = load_or_create(&other_store, &other_dir, 1000).unwrap();
    let other_code = export_recovery(&other);
    assert_ne!(existing.anon_id(), other.anon_id());

    let err = import_recovery(&store, &dir, &other_code, 2000, false).unwrap_err();
    match err {
        IdentityError::RecoveryWouldReplace { existing: e } => assert_eq!(e, existing.anon_id()),
        other => panic!("expected a refusal, got {other}"),
    }
    // The original identity must be untouched.
    let (still, _, _) = load_or_create(&store, &dir, 3000).unwrap();
    assert_eq!(still.anon_id(), existing.anon_id());
}

#[test]
fn repeated_recovery_does_not_duplicate_history() {
    // Importing twice must leave one record and one seed.
    let dir = tmp("repeat");
    let store = MemoryStore::new();
    let (original, _, _) = load_or_create(&store, &dir, 1000).unwrap();
    let code = export_recovery(&original);

    for _ in 0..3 {
        import_recovery(&store, &dir, &code, 2000, false).unwrap();
    }
    let records = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("identity"))
        .count();
    assert_eq!(records, 1, "one identity record, not one per import");
    let (loaded, _, _) = load_or_create(&store, &dir, 4000).unwrap();
    assert_eq!(loaded.anon_id(), original.anon_id());
}

#[test]
fn a_seed_with_a_mismatched_record_is_a_hard_error() {
    let dir = tmp("mismatch");
    let store = MemoryStore::new();
    let (original, _, _) = load_or_create(&store, &dir, 1000).unwrap();

    // Corrupt the record's anon id.
    let mut record = read_record(&dir).unwrap().unwrap();
    record.anon_id = "fl1_00000000000000000000000000000000".into();
    crate::identity::write_record_for_test(&dir, &record);

    let err = load_or_create(&store, &dir, 2000).unwrap_err();
    assert!(matches!(err, IdentityError::IdentityConflict { .. }));
    assert_eq!(original.anon_id().len(), 36);
}

#[test]
fn the_identity_view_never_contains_the_seed() {
    let dir = tmp("view");
    let store = MemoryStore::new();
    let (id, record, _) = load_or_create(&store, &dir, 1000).unwrap();
    let json = serde_json::to_string(&IdentityView {
        anon_id: record.anon_id.clone(),
        root_pubkey_hex: record.root_pubkey_hex.clone(),
        epoch: record.epoch,
        created_at_ms: record.created_at_ms,
        backend: store.backend(),
        registered: record.registered,
        seed_available: seed_available(&store),
    })
    .unwrap();
    let seed_hex = crate::identity_store::hex_encode(&id.seed());
    assert!(!json.contains(&seed_hex), "the seed must never reach the UI");
    assert!(json.contains(&record.anon_id));
}
