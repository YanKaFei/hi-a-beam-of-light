//! Local SQLite store.
//!
//! This is the source of truth for the personal light. The app works fully with
//! only this database — no account, no network (product spec §1.3 "可仅本地使用").
//! The server is an optional replica for the public field.
//!
//! Two guarantees are enforced at the schema level rather than in code:
//!
//! 1. **One countable record per family.** `unique(family)` on
//!    `countable_evidence` means the same family can never be counted twice, no
//!    matter how many clients or sessions use it.
//! 2. **Self-attestation cannot masquerade as verification.** E2 records live in
//!    a *different table* (`attested_family`), so no query that computes the
//!    light level can accidentally include them.

#![forbid(unsafe_code)]

use light_core::{compute_light_state, CountableEvidence, LightState};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(String),
    #[error("store schema version {found} is newer than supported {supported}")]
    FutureSchema { found: i64, supported: i64 },
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

const MIGRATION_V1: &str = r#"
create table if not exists local_identity (
    singleton   integer primary key check (singleton = 1),
    anon_id     text    not null,
    pubkey_hex  text    not null,
    key_epoch   integer not null default 1,
    state       text    not null default 'LOCAL_ONLY',
    created_at_ms integer not null
);

-- Countable (E3/E4) evidence. One row per family, forever.
create table if not exists countable_evidence (
    family          text primary key,
    event_id        text not null,
    level           integer not null check (level in (3,4)),
    proof_method    text not null,
    adapter_id      text not null,
    adapter_version text not null,
    registry_semver text not null,
    observed_at_ms  integer not null,
    upload_state    text not null default 'LOCAL_ONLY'
        check (upload_state in ('LOCAL_ONLY','QUEUED','UPLOADED','REJECTED'))
);

-- Self-attested (E2) families. Deliberately a separate table so they can never
-- be joined into the light level by accident.
create table if not exists attested_family (
    family        text primary key,
    attested_at_ms integer not null
);

-- Discovered/connected observations (E0/E1), display only.
create table if not exists observation (
    id           text primary key,
    raw_model    text,
    client_id    text,
    level        integer not null check (level between 0 and 1),
    adapter_id   text not null,
    observed_at_ms integer not null
);

create table if not exists received_glow (
    singleton integer primary key check (singleton = 1),
    count     integer not null default 0
);

create table if not exists local_edge (
    edge_id      text primary key,
    role         text not null check (role in ('HELPER','HELPED')),
    peer_anon_id text not null,
    state        text not null,
    visibility   text not null default 'PRIVATE',
    created_at_ms integer not null,
    activated_at_ms integer
);

create table if not exists consent (
    scope          text primary key,
    state          text not null check (state in ('GRANTED','DECLINED','REVOKED')),
    policy_version text not null,
    changed_at_ms  integer not null
);

create table if not exists outbox (
    id           integer primary key autoincrement,
    kind         text not null,
    payload      text not null,
    created_at_ms integer not null,
    attempts     integer not null default 0,
    uploaded_at_ms integer
);
"#;

fn apply_migrations(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.query_row("pragma user_version", [], |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(StoreError::FutureSchema {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    if current < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V1)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRow {
    pub anon_id: String,
    pub pubkey_hex: String,
    pub key_epoch: i64,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRow {
    pub id: i64,
    pub kind: String,
    pub payload: String,
    pub attempts: i64,
}

pub struct LocalStore {
    conn: Connection,
}

impl LocalStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON")?;
        apply_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        apply_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn.query_row("pragma user_version", [], |r| r.get(0))?)
    }

    // -- identity ---------------------------------------------------------

    pub fn set_identity(
        &mut self,
        anon_id: &str,
        pubkey_hex: &str,
        key_epoch: i64,
        now_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "insert into local_identity (singleton, anon_id, pubkey_hex, key_epoch, state, created_at_ms)
             values (1, ?1, ?2, ?3, 'LOCAL_ONLY', ?4)
             on conflict(singleton) do update set
                anon_id = excluded.anon_id,
                pubkey_hex = excluded.pubkey_hex,
                key_epoch = excluded.key_epoch",
            rusqlite::params![anon_id, pubkey_hex, key_epoch, now_ms],
        )?;
        Ok(())
    }

    pub fn identity(&self) -> Result<Option<IdentityRow>> {
        Ok(self
            .conn
            .query_row(
                "select anon_id, pubkey_hex, key_epoch, state from local_identity where singleton = 1",
                [],
                |row| {
                    Ok(IdentityRow {
                        anon_id: row.get(0)?,
                        pubkey_hex: row.get(1)?,
                        key_epoch: row.get(2)?,
                        state: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    // -- evidence ---------------------------------------------------------

    /// Record countable evidence. Returns `false` when this family was already
    /// counted — the write is a no-op, which is what makes multi-client usage of
    /// one family count exactly once.
    pub fn record_countable_evidence(&mut self, ev: &CountableEvidence) -> Result<bool> {
        if !ev.level.is_countable() {
            return Err(StoreError::Sqlite(
                "refusing to store non-countable evidence".into(),
            ));
        }
        let changed = self.conn.execute(
            "insert into countable_evidence
                (family, event_id, level, proof_method, adapter_id, adapter_version,
                 registry_semver, observed_at_ms)
             values (?1,?2,?3,?4,?5,?6,?7,?8)
             on conflict(family) do nothing",
            rusqlite::params![
                ev.family,
                ev.event_id,
                ev.level.as_u8() as i64,
                format!("{:?}", ev.proof_method).to_lowercase(),
                ev.adapter_id,
                ev.adapter_version,
                ev.registry_semver,
                ev.observed_at_ms,
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn verified_families(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("select family from countable_evidence order by family")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn verified_family_count(&self) -> Result<u32> {
        let n: i64 = self
            .conn
            .query_row("select count(*) from countable_evidence", [], |r| r.get(0))?;
        Ok(n as u32)
    }

    /// Record a self-attested (E2) family. Never affects the light level.
    pub fn record_attested_family(&mut self, family: &str, now_ms: i64) -> Result<()> {
        self.conn.execute(
            "insert into attested_family (family, attested_at_ms) values (?1, ?2)
             on conflict(family) do nothing",
            rusqlite::params![family, now_ms],
        )?;
        Ok(())
    }

    pub fn attested_family_count(&self) -> Result<u32> {
        let n: i64 = self
            .conn
            .query_row("select count(*) from attested_family", [], |r| r.get(0))?;
        Ok(n as u32)
    }

    pub fn record_observation(
        &mut self,
        id: &str,
        raw_model: &str,
        client_id: Option<&str>,
        level: u8,
        adapter_id: &str,
        now_ms: i64,
    ) -> Result<()> {
        if level > 1 {
            return Err(StoreError::Sqlite(
                "observations are limited to E0/E1".into(),
            ));
        }
        self.conn.execute(
            "insert into observation (id, raw_model, client_id, level, adapter_id, observed_at_ms)
             values (?1,?2,?3,?4,?5,?6) on conflict(id) do nothing",
            rusqlite::params![id, raw_model, client_id, level as i64, adapter_id, now_ms],
        )?;
        Ok(())
    }

    // -- received light ---------------------------------------------------

    pub fn set_received_glow(&mut self, count: i64) -> Result<()> {
        self.conn.execute(
            "insert into received_glow (singleton, count) values (1, ?1)
             on conflict(singleton) do update set count = excluded.count",
            rusqlite::params![count],
        )?;
        Ok(())
    }

    pub fn received_glow(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("select count from received_glow where singleton = 1", [], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or(0))
    }

    /// The personal light state, computed from this database alone.
    pub fn light_state(&self, activated_nodes: u32) -> Result<LightState> {
        Ok(compute_light_state(
            self.verified_family_count()?,
            self.attested_family_count()?,
            self.received_glow()?.max(0) as u32,
            activated_nodes,
        ))
    }

    // -- outbox -----------------------------------------------------------

    pub fn enqueue(&mut self, kind: &str, payload: &str, now_ms: i64) -> Result<i64> {
        self.conn.execute(
            "insert into outbox (kind, payload, created_at_ms) values (?1, ?2, ?3)",
            rusqlite::params![kind, payload, now_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn pending(&self, limit: usize) -> Result<Vec<OutboxRow>> {
        let mut stmt = self.conn.prepare(
            "select id, kind, payload, attempts from outbox
             where uploaded_at_ms is null order by id limit ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |r| {
                Ok(OutboxRow {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    payload: r.get(2)?,
                    attempts: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn mark_uploaded(&mut self, id: i64, now_ms: i64) -> Result<()> {
        self.conn.execute(
            "update outbox set uploaded_at_ms = ?1, attempts = attempts + 1 where id = ?2",
            rusqlite::params![now_ms, id],
        )?;
        Ok(())
    }

    pub fn bump_attempt(&mut self, id: i64) -> Result<()> {
        self.conn
            .execute("update outbox set attempts = attempts + 1 where id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    // -- consent ----------------------------------------------------------

    pub fn set_consent(
        &mut self,
        scope: &str,
        state: &str,
        policy_version: &str,
        now_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "insert into consent (scope, state, policy_version, changed_at_ms)
             values (?1,?2,?3,?4)
             on conflict(scope) do update set
                state = excluded.state,
                policy_version = excluded.policy_version,
                changed_at_ms = excluded.changed_at_ms",
            rusqlite::params![scope, state, policy_version, now_ms],
        )?;
        Ok(())
    }

    /// Absence means "not granted" — spec §5.4 default of not contributing.
    pub fn consent_state(&self, scope: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("select state from consent where scope = ?1", rusqlite::params![scope], |r| {
                r.get(0)
            })
            .optional()?)
    }
}

#[cfg(test)]
mod tests;
