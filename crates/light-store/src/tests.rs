//! Store tests: dedup, E2 isolation, offline usability, consent default.

use super::*;
use light_core::{EvidenceLevel, ProofMethod};

fn evidence(family: &str, event: &str) -> CountableEvidence {
    CountableEvidence {
        event_id: event.into(),
        family: family.into(),
        level: EvidenceLevel::E3,
        proof_method: ProofMethod::LocalSqliteMetadata,
        adapter_id: "codex-app".into(),
        adapter_version: "1.0.0".into(),
        registry_semver: "1.0.0".into(),
        observed_at_ms: 1_761_868_800_000,
    }
}

fn store() -> LocalStore {
    LocalStore::in_memory().expect("in-memory store")
}

#[test]
fn migrations_are_idempotent() {
    let s = store();
    assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
    drop(s);
    // Re-opening the same in-memory store is not meaningful; assert the
    // migration function itself is safe to re-run.
    let mut conn = Connection::open_in_memory().unwrap();
    apply_migrations(&mut conn).unwrap();
    apply_migrations(&mut conn).unwrap();
    apply_migrations(&mut conn).unwrap();
    let v: i64 = conn.query_row("pragma user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, SCHEMA_VERSION);
}

#[test]
fn identity_roundtrip_and_upsert() {
    let mut s = store();
    assert!(s.identity().unwrap().is_none());

    s.set_identity("fl1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "aabb", 1, 1)
        .unwrap();
    let row = s.identity().unwrap().unwrap();
    assert_eq!(row.key_epoch, 1);
    assert_eq!(row.state, "LOCAL_ONLY");

    // Rotation updates epoch without creating a second identity.
    s.set_identity("fl1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "ccdd", 2, 2)
        .unwrap();
    let row = s.identity().unwrap().unwrap();
    assert_eq!(row.key_epoch, 2);
    assert_eq!(row.pubkey_hex, "ccdd");
}

#[test]
fn one_family_counts_exactly_once() {
    let mut s = store();
    assert!(s.record_countable_evidence(&evidence("gpt", "e1")).unwrap());
    // A second client, a second event, same family: no-op.
    assert!(!s.record_countable_evidence(&evidence("gpt", "e2")).unwrap());
    assert!(!s.record_countable_evidence(&evidence("gpt", "e3")).unwrap());

    assert_eq!(s.verified_family_count().unwrap(), 1);
    assert_eq!(s.verified_families().unwrap(), vec!["gpt".to_string()]);
}

#[test]
fn different_families_accumulate() {
    let mut s = store();
    for (i, f) in ["gpt", "claude", "deepseek", "qwen"].iter().enumerate() {
        assert!(s
            .record_countable_evidence(&evidence(f, &format!("e{i}")))
            .unwrap());
    }
    assert_eq!(s.verified_family_count().unwrap(), 4);
    let light = s.light_state(0).unwrap();
    assert_eq!(light.knowledge_title.display_zh(), "探索者");
}

#[test]
fn non_countable_evidence_is_refused() {
    let mut s = store();
    let mut ev = evidence("gpt", "e1");
    ev.level = EvidenceLevel::E2;
    let err = s.record_countable_evidence(&ev).unwrap_err();
    assert!(err.to_string().contains("non-countable"));
    assert_eq!(s.verified_family_count().unwrap(), 0);
}

#[test]
fn e2_never_changes_the_light_level() {
    // D-03: self-attestation is visible but never deepens the core.
    let mut s = store();
    let before = s.light_state(0).unwrap();
    for f in ["gpt", "claude", "deepseek", "qwen", "gemini", "llama", "mistral"] {
        s.record_attested_family(f, 1).unwrap();
    }
    let after = s.light_state(0).unwrap();

    assert_eq!(after.attested_families, 7);
    assert_eq!(after.verified_families, 0);
    assert_eq!(before.level, after.level, "E2 must not change the level");
    assert_eq!(after.level, light_core::LightLevel::Dormant);
    assert_eq!(after.knowledge_title.display_zh(), "寻光者");
}

#[test]
fn e2_and_e3_are_independent_tallies() {
    let mut s = store();
    s.record_countable_evidence(&evidence("gpt", "e1")).unwrap();
    s.record_attested_family("claude", 1).unwrap();
    let light = s.light_state(0).unwrap();
    assert_eq!(light.verified_families, 1);
    assert_eq!(light.attested_families, 1);
    assert_eq!(light.knowledge_title.display_zh(), "初明者");
}

#[test]
fn observations_are_limited_to_e0_e1() {
    let mut s = store();
    s.record_observation("o1", "gpt-6-sol", Some("codex"), 0, "codex-app", 1)
        .unwrap();
    s.record_observation("o2", "claude-x", None, 1, "codex-app", 2)
        .unwrap();
    assert!(s
        .record_observation("o3", "x", None, 2, "codex-app", 3)
        .is_err());
    // Observations never affect the light.
    assert_eq!(s.verified_family_count().unwrap(), 0);
}

#[test]
fn received_light_does_not_change_the_core() {
    let mut s = store();
    s.record_countable_evidence(&evidence("gpt", "e1")).unwrap();
    let before = s.light_state(0).unwrap();
    s.set_received_glow(25).unwrap();
    let after = s.light_state(0).unwrap();
    assert_eq!(before.level, after.level);
    assert_eq!(before.knowledge_title, after.knowledge_title);
    assert_eq!(after.received_glow, 25);
}

#[test]
fn transmission_title_uses_activated_nodes() {
    let mut s = store();
    assert_eq!(s.light_state(0).unwrap().transmission_title.display_zh(), "持光者");
    assert_eq!(s.light_state(3).unwrap().transmission_title.display_zh(), "引光者");
    assert_eq!(s.light_state(10).unwrap().transmission_title.display_zh(), "播光者");
    assert_eq!(s.light_state(100).unwrap().transmission_title.display_zh(), "燎原者");
}

#[test]
fn outbox_queues_and_drains() {
    let mut s = store();
    let a = s.enqueue("usage.attest", r#"{"event_id":"e1"}"#, 1).unwrap();
    let b = s.enqueue("usage.attest", r#"{"event_id":"e2"}"#, 2).unwrap();
    assert!(b > a);

    let pending = s.pending(10).unwrap();
    assert_eq!(pending.len(), 2);

    s.mark_uploaded(a, 10).unwrap();
    let pending = s.pending(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, b);

    s.bump_attempt(b).unwrap();
    assert_eq!(s.pending(10).unwrap()[0].attempts, 1);
}

#[test]
fn consent_defaults_to_not_granted() {
    // Spec §5.4: the default is *not* contributing.
    let mut s = store();
    assert_eq!(s.consent_state("GLOBAL_FIELD_CONTRIBUTION").unwrap(), None);

    s.set_consent("GLOBAL_FIELD_CONTRIBUTION", "GRANTED", "v1", 1)
        .unwrap();
    assert_eq!(
        s.consent_state("GLOBAL_FIELD_CONTRIBUTION").unwrap(),
        Some("GRANTED".into())
    );

    s.set_consent("GLOBAL_FIELD_CONTRIBUTION", "REVOKED", "v1", 2)
        .unwrap();
    assert_eq!(
        s.consent_state("GLOBAL_FIELD_CONTRIBUTION").unwrap(),
        Some("REVOKED".into())
    );
}

#[test]
fn the_app_is_fully_usable_without_any_network() {
    // Everything that makes "my light" work happens against this database alone.
    let mut s = store();
    s.set_identity("fl1_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "eeff", 1, 1)
        .unwrap();
    s.record_countable_evidence(&evidence("gpt", "e1")).unwrap();
    s.record_countable_evidence(&evidence("deepseek", "e2")).unwrap();

    let light = s.light_state(0).unwrap();
    assert_eq!(light.verified_families, 2);
    assert_eq!(light.level, light_core::LightLevel::Growing);
    assert_eq!(light.knowledge_title.display_zh(), "启迪者");
    // No outbox requirement: local state is already complete.
}

#[test]
fn future_schema_is_refused_rather_than_corrupted() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "user_version", 99i64).unwrap();
    let err = apply_migrations(&mut conn).unwrap_err();
    assert!(matches!(err, StoreError::FutureSchema { found: 99, .. }));
}
