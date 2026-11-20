//! Privacy negative tests and guard behaviour.
//!
//! These tests are the evidence for F1 acceptance scenario B: sensitive columns
//! cannot be reached through the adapter data-access layer, and the attempt is
//! refused visibly rather than silently satisfied.

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static SEQ: AtomicU32 = AtomicU32::new(0);

fn temp_db_path(tag: &str) -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop(); // crates/
    dir.pop(); // repo root
    dir.push("target");
    dir.push("adapter-sdk-test-db");
    std::fs::create_dir_all(&dir).expect("create test db dir");
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    dir.push(format!("{tag}-{}-{n}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&dir);
    dir
}

/// Mirrors the shape observed on a real client, including the sensitive columns
/// that share the table with usable metadata.
fn seed_client_db(path: &PathBuf) {
    let conn = Connection::open(path).expect("create db");
    conn.execute_batch(
        r#"
        create table threads (
            id text primary key,
            model text,
            model_provider text,
            tokens_used integer,
            first_user_message text,
            preview text,
            title text,
            cwd text
        );
        create table thread_items (
            thread_id text not null,
            item_type text not null,
            item_json text
        );
        "#,
    )
    .expect("schema");

    conn.execute(
        "insert into threads (id, model, model_provider, tokens_used, first_user_message, preview, title, cwd)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            "t1",
            "gpt-6-sol",
            "openai",
            1234i64,
            "SECRET_USER_WORDS_alpha",
            "SECRET_PREVIEW_beta",
            "SECRET_TITLE_gamma",
            "/Users/someone/secret-project"
        ],
    )
    .expect("insert t1");

    conn.execute(
        "insert into threads (id, model, model_provider, tokens_used, first_user_message, preview, title, cwd)
         values ('t2', 'gpt-5.6-sol', 'openai', 0, 'SECRET_USER_WORDS_delta', 'p', 'ttl', '/Users/someone/x')",
        [],
    )
    .expect("insert t2");

    conn.execute(
        "insert into thread_items (thread_id, item_type, item_json) values
            ('t1','userMessage','SECRET_ITEM_json_one'),
            ('t1','agentMessage','SECRET_ITEM_json_two'),
            ('t1','reasoning','SECRET_ITEM_json_three'),
            ('t2','userMessage','SECRET_ITEM_json_four')",
        [],
    )
    .expect("insert items");
}

fn allowed_manifest() -> AdapterManifest {
    AdapterManifest {
        id: "test-adapter".into(),
        version: "1.0.0".into(),
        platforms: vec![Platform::Macos],
        tables: vec![
            TablePolicy::new("threads", &["id", "model", "model_provider"]),
            TablePolicy::new("thread_items", &["thread_id", "item_type"]),
        ],
        evidence_capability: EvidenceCapability::LocalCallProof,
        max_rows: 1_000,
        timeout_ms: 5_000,
    }
}

fn guarded(tag: &str) -> GuardedDb {
    let path = temp_db_path(tag);
    seed_client_db(&path);
    GuardedDb::open_readonly(&path, allowed_manifest()).expect("open guarded")
}

// ---------------------------------------------------------------------------
// A. Manifest validation: the global deny list cannot be overridden
// ---------------------------------------------------------------------------

#[test]
fn manifest_cannot_allow_conversation_columns() {
    for column in [
        "first_user_message",
        "preview",
        "title",
        "item_json",
        "feedback_log_body",
        "prompt",
        "text",
        "content",
    ] {
        let mut manifest = allowed_manifest();
        manifest.tables.push(TablePolicy::new("threads_extra", &[column]));
        let err = manifest.validate().expect_err("must reject");
        assert!(
            matches!(err, ManifestError::GloballyForbiddenColumn { .. }),
            "column `{column}` must be refused, got {err:?}"
        );
    }
}

#[test]
fn manifest_cannot_allow_consumption_counters() {
    // Spec §1.5: never score consumption. We go further and never read it.
    for column in ["tokens_used", "tokens", "cost", "usage_cost"] {
        let mut manifest = allowed_manifest();
        manifest.tables.push(TablePolicy::new("threads_extra", &[column]));
        assert!(
            manifest.validate().is_err(),
            "consumption counter `{column}` must be globally forbidden"
        );
    }
}

#[test]
fn manifest_cannot_allow_secrets_or_contact_data() {
    for column in ["api_key", "authorization", "token", "password", "email", "phone", "latitude"] {
        let mut manifest = allowed_manifest();
        manifest.tables.push(TablePolicy::new("t", &[column]));
        assert!(manifest.validate().is_err(), "`{column}` must be forbidden");
    }
}

#[test]
fn manifest_cannot_allow_filesystem_detail() {
    for column in ["rollout_path", "cwd", "file_path", "git_origin_url"] {
        let mut manifest = allowed_manifest();
        manifest.tables.push(TablePolicy::new("t", &[column]));
        assert!(manifest.validate().is_err(), "`{column}` must be forbidden");
    }
}

#[test]
fn manifest_rejects_contradiction_and_duplicates() {
    let mut contradictory = allowed_manifest();
    contradictory.tables.push(
        TablePolicy::new("x", &["model"]).deny(&["model"]),
    );
    assert!(matches!(
        contradictory.validate().unwrap_err(),
        ManifestError::ContradictoryPolicy { .. }
    ));

    let mut duplicate = allowed_manifest();
    duplicate.tables.push(TablePolicy::new("threads", &["id"]));
    assert!(matches!(
        duplicate.validate().unwrap_err(),
        ManifestError::DuplicateTable { .. }
    ));

    let mut no_budget = allowed_manifest();
    no_budget.max_rows = 0;
    assert_eq!(no_budget.validate().unwrap_err(), ManifestError::InvalidBudget);

    let mut no_tables = allowed_manifest();
    no_tables.tables.clear();
    assert!(matches!(
        no_tables.validate().unwrap_err(),
        ManifestError::NoTables { .. }
    ));
}

#[test]
fn invalid_identifiers_are_rejected() {
    let mut bad = allowed_manifest();
    bad.tables.push(TablePolicy::new("ok", &["id; drop table threads"]));
    assert!(matches!(
        bad.validate().unwrap_err(),
        ManifestError::InvalidIdentifier { .. }
    ));
}

// ---------------------------------------------------------------------------
// B. Runtime refusals
// ---------------------------------------------------------------------------

#[test]
fn reading_a_sensitive_column_is_refused_and_recorded() {
    let mut db = guarded("sensitive-read");
    for column in ["first_user_message", "preview", "title", "tokens_used", "cwd"] {
        let err = db
            .rows(RowsSpec {
                table: "threads".into(),
                columns: vec![column.to_string()],
                filter: None,
                limit: 10,
            })
            .expect_err("must refuse");
        assert!(
            matches!(err, GuardError::GloballyForbidden { .. }),
            "`{column}` must be refused, got {err:?}"
        );
    }
    // Every attempt is on record.
    assert_eq!(db.audit().rejections.len(), 5);
    assert!(db
        .audit()
        .rejections
        .iter()
        .all(|r| r.reason == "globally_forbidden"));
    // And nothing was read.
    assert_eq!(db.audit().entries.len(), 0);
}

#[test]
fn reading_a_column_outside_the_allowlist_is_refused() {
    let mut db = guarded("not-allowlisted");
    // A column that exists in the table but is not declared, and is not on the
    // global deny list either: it must still be unreachable.
    let err = db
        .rows(RowsSpec {
            table: "threads".into(),
            columns: vec!["some_undeclared_column".into()],
            filter: None,
            limit: 5,
        })
        .unwrap_err();
    assert!(matches!(err, GuardError::ColumnNotAllowed { .. }), "got {err:?}");
    assert_eq!(db.audit().rejections[0].reason, "not_in_allowlist");

    // A table that exists but is not declared by this adapter.
    let mut db2 = guarded("undeclared-table-2");
    let err = db2
        .rows(RowsSpec {
            table: "sqlite_master".into(),
            columns: vec!["name".into()],
            filter: None,
            limit: 5,
        })
        .unwrap_err();
    assert!(matches!(err, GuardError::TableNotAllowed { .. }), "got {err:?}");
}

#[test]
fn filtering_on_a_forbidden_column_is_refused() {
    // A forbidden column must not be reachable through a WHERE clause either.
    let mut db = guarded("forbidden-filter");
    let err = db
        .rows(RowsSpec {
            table: "threads".into(),
            columns: vec!["model".into()],
            filter: Some(Filter::eq("first_user_message", "SECRET_USER_WORDS_alpha")),
            limit: 5,
        })
        .unwrap_err();
    assert!(matches!(err, GuardError::GloballyForbidden { .. }));
}

#[test]
fn undeclared_table_is_refused() {
    let mut db = guarded("undeclared-table");
    let err = db
        .rows(RowsSpec {
            table: "sqlite_master".into(),
            columns: vec!["name".into()],
            filter: None,
            limit: 5,
        })
        .unwrap_err();
    assert!(matches!(err, GuardError::TableNotAllowed { .. }));
}

#[test]
fn select_star_is_structurally_impossible() {
    // The only asterisk the generator can emit is count(*).
    assert!(has_no_bare_star("SELECT \"id\", \"model\" FROM \"threads\" LIMIT 5"));
    assert!(has_no_bare_star("SELECT count(*) FROM \"threads\""));
    assert!(!has_no_bare_star("SELECT * FROM \"threads\""));
    assert!(!has_no_bare_star("SELECT \"id\", * FROM \"threads\""));

    // And a real query never produces one.
    let mut db = guarded("no-star");
    db.rows(RowsSpec {
        table: "threads".into(),
        columns: vec!["id".into(), "model".into()],
        filter: None,
        limit: 10,
    })
    .unwrap();
    let n = db.count("threads", None).unwrap();
    assert_eq!(n, 2);
}

// ---------------------------------------------------------------------------
// C. Read-only and audit hygiene
// ---------------------------------------------------------------------------

#[test]
fn connection_is_read_only() {
    let db = guarded("readonly");
    assert!(db.is_read_only(), "query_only must be enabled");
}

#[test]
fn audit_records_names_never_values() {
    let mut db = guarded("audit");
    let rows = db
        .rows(RowsSpec {
            table: "threads".into(),
            columns: vec!["id".into(), "model".into(), "model_provider".into()],
            filter: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get_text("model"), Some("gpt-6-sol"));

    // The audit and its debug rendering must not leak any stored value.
    let rendered = format!("{:?}", db.audit());
    for secret in [
        "SECRET_USER_WORDS_alpha",
        "SECRET_PREVIEW_beta",
        "SECRET_TITLE_gamma",
        "SECRET_ITEM_json_one",
        "/Users/someone",
        "gpt-6-sol",
    ] {
        assert!(
            !rendered.contains(secret),
            "audit must not contain `{secret}`: {rendered}"
        );
    }
    // The guarded handle must not leak the path either.
    let handle = format!("{db:?}");
    assert!(!handle.contains("/Users/"), "guard debug must not leak paths: {handle}");
}

#[test]
fn a_refusal_never_widens_the_scan() {
    let mut db = guarded("no-widen");
    let _ = db.rows(RowsSpec {
        table: "threads".into(),
        columns: vec!["first_user_message".into()],
        filter: None,
        limit: 10,
    });
    // A subsequent legal query still only touches what it asked for.
    db.rows(RowsSpec {
        table: "threads".into(),
        columns: vec!["id".into()],
        filter: None,
        limit: 10,
    })
    .unwrap();

    assert_eq!(db.audit().rejections.len(), 1, "exactly one refusal recorded");
    assert_eq!(db.audit().entries.len(), 1, "no implicit retry with wider scope");
    let used = db.audit().columns_used();
    assert_eq!(used.len(), 1);
    assert!(used.contains("id"));
}

#[test]
fn row_budget_stops_a_runaway_scan() {
    let mut manifest = allowed_manifest();
    manifest.max_rows = 1;
    let path = temp_db_path("budget");
    seed_client_db(&path);
    let mut db = GuardedDb::open_readonly(&path, manifest).unwrap();
    let err = db
        .rows(RowsSpec {
            table: "threads".into(),
            columns: vec!["id".into()],
            filter: None,
            limit: 100,
        })
        .unwrap_err();
    assert!(matches!(err, GuardError::RowBudgetExceeded { .. }));
}

#[test]
fn zero_limit_is_rejected() {
    let mut db = guarded("zero-limit");
    assert_eq!(
        db.rows(RowsSpec {
            table: "threads".into(),
            columns: vec!["id".into()],
            filter: None,
            limit: 0,
        })
        .unwrap_err(),
        GuardError::InvalidLimit(0)
    );
}

// ---------------------------------------------------------------------------
// D. The shapes adapters actually need work
// ---------------------------------------------------------------------------

#[test]
fn group_count_answers_per_session_item_counts() {
    let mut db = guarded("group-count");
    let counts = db
        .group_count(GroupCountSpec {
            table: "thread_items".into(),
            group_column: "thread_id".into(),
            filter: Some(Filter::is_in(
                "item_type",
                vec![
                    Value::from("agentMessage"),
                    Value::from("reasoning"),
                    Value::from("commandExecution"),
                ],
            )),
            limit: 100,
        })
        .unwrap();
    let map: BTreeMap<_, _> = counts
        .into_iter()
        .map(|(k, v)| match k {
            Value::Text(t) => (t, v),
            Value::Int(i) => (i.to_string(), v),
        })
        .collect();
    assert_eq!(map.get("t1"), Some(&2)); // agentMessage + reasoning
    assert_eq!(map.get("t2"), None); // only a userMessage
}

#[test]
fn filter_in_and_gt_are_bound_as_parameters() {
    let mut db = guarded("filters");
    let rows = db
        .rows(RowsSpec {
            table: "threads".into(),
            columns: vec!["id".into(), "model".into()],
            filter: Some(Filter::is_in(
                "model",
                vec![Value::from("gpt-6-sol"), Value::from("nope")],
            )),
            limit: 10,
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get_text("id"), Some("t1"));
}

#[test]
fn opening_a_missing_file_fails_without_searching_for_alternatives() {
    let missing = temp_db_path("missing");
    let err = GuardedDb::open_readonly(&missing, allowed_manifest()).unwrap_err();
    assert!(matches!(err, GuardError::Open(_)), "got {err:?}");
    // The error must not echo the path.
    assert!(!err.to_string().contains("/Users/"));
}

// ---------------------------------------------------------------------------
// E. Live WAL databases (measured against a real client)
// ---------------------------------------------------------------------------

fn seed_wal_db(path: &PathBuf) {
    let conn = Connection::open(path).expect("create wal db");
    conn.pragma_update(None, "journal_mode", "WAL").expect("wal");
    conn.pragma_update(None, "wal_autocheckpoint", 0).expect("no ckpt");
    seed_schema(&conn);
    // Leave the connection open so the -shm/-wal sidecars stay live.
    std::mem::forget(conn);
}

fn seed_schema(conn: &Connection) {
    conn.execute_batch(
        r#"
        create table threads (
            id text primary key, model text, model_provider text,
            tokens_used integer, first_user_message text
        );
        create table thread_items (thread_id text not null, item_type text not null);
        insert into threads (id, model, model_provider, tokens_used, first_user_message)
            values ('t1','gpt-6-sol','openai',10,'SECRET_WORDS');
        insert into thread_items (thread_id, item_type) values ('t1','agentMessage');
        "#,
    )
    .expect("schema");
}

#[test]
fn live_wal_database_reports_the_strategy_it_used() {
    let path = temp_db_path("wal");
    seed_wal_db(&path);

    // Strict read-only cannot read a live WAL database (documented SQLite
    // limitation), so the compatible path must be available and must say so.
    let db = GuardedDb::open_compatible(&path, allowed_manifest_for_wal());
    match db {
        Ok(db) => {
            assert!(db.is_read_only(), "query_only must be enabled");
            assert!(matches!(
                db.open_mode(),
                OpenMode::ReadOnly | OpenMode::QueryOnly
            ));
            let mode = format!("{:?}", db.open_mode());
            assert!(mode == "ReadOnly" || mode == "QueryOnly");
        }
        Err(e) => panic!("compatible open must succeed on a live wal db: {e:?}"),
    }
}

fn allowed_manifest_for_wal() -> AdapterManifest {
    AdapterManifest {
        id: "wal-adapter".into(),
        version: "1.0.0".into(),
        platforms: vec![Platform::Macos],
        tables: vec![
            TablePolicy::new("threads", &["id", "model"]),
            TablePolicy::new("thread_items", &["thread_id", "item_type"]),
        ],
        evidence_capability: EvidenceCapability::LocalCallProof,
        max_rows: 1000,
        timeout_ms: 5_000,
    }
}

#[test]
fn query_only_mode_still_cannot_write() {
    let path = temp_db_path("wal-write");
    seed_wal_db(&path);
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .expect("open rw");
    conn.pragma_update(None, "query_only", true).expect("query_only");
    let err = conn
        .execute("insert into threads (id) values ('x')", [])
        .expect_err("query_only must reject writes");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("readonly") || msg.contains("read-only"),
        "unexpected error: {msg}"
    );
}
