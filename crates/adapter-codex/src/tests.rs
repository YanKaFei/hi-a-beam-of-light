//! Adapter tests against synthetic databases shaped like the real client.
//!
//! Synthetic fixtures are used so the suite is reproducible on any machine and
//! contains no personal data. The same code is exercised against the real
//! database by `examples/scan_local.rs`, which is run manually and reported.

use super::*;
use light_core::EvidencePolicy;
use rusqlite::Connection;
use std::sync::atomic::{AtomicU32, Ordering};

static SEQ: AtomicU32 = AtomicU32::new(0);

fn workspace_tmp(tag: &str) -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    dir.push("target");
    dir.push("adapter-codex-test-db");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    dir.push(format!("{tag}-{}-{n}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&dir);
    dir
}

const STATE_SCHEMA: &str = r#"
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
"#;

const HISTORY_SCHEMA: &str = r#"
create table thread_turns (
    thread_id text not null,
    status text not null
);
create table thread_items (
    thread_id text not null,
    item_type text not null,
    item_json text
);
"#;

fn insert_thread(conn: &Connection, id: &str, model: &str) {
    conn.execute(
        "insert into threads (id, model, model_provider, tokens_used,
             first_user_message, preview, title, cwd)
         values (?1, ?2, 'openai', 999, 'SECRET_WORDS', 'SECRET_PREVIEW', 'SECRET_TITLE', '/Users/x/y')",
        rusqlite::params![id, model],
    )
    .expect("insert thread");
}

fn insert_items(conn: &Connection, thread: &str, types: &[&str]) {
    for t in types {
        conn.execute(
            "insert into thread_items (thread_id, item_type, item_json) values (?1, ?2, 'SECRET_JSON')",
            rusqlite::params![thread, t],
        )
        .expect("insert item");
    }
}

fn build(
    tag: &str,
    threads: &[(&str, &str)],
    items: &[(&str, &[&str])],
    turns: &[(&str, &str)],
) -> CodexPaths {
    let state_path = workspace_tmp(&format!("{tag}-state"));
    let history_path = workspace_tmp(&format!("{tag}-history"));

    let state = Connection::open(&state_path).unwrap();
    state.execute_batch(STATE_SCHEMA).unwrap();
    for (id, model) in threads {
        insert_thread(&state, id, model);
    }
    drop(state);

    let history = Connection::open(&history_path).unwrap();
    history.execute_batch(HISTORY_SCHEMA).unwrap();
    for (thread, types) in items {
        insert_items(&history, thread, types);
    }
    for (thread, status) in turns {
        history
            .execute(
                "insert into thread_turns (thread_id, status) values (?1, ?2)",
                rusqlite::params![thread, status],
            )
            .unwrap();
    }
    drop(history);

    CodexPaths::new(state_path, Some(history_path))
}

fn policy() -> EvidencePolicy {
    EvidencePolicy::v1()
}

// ---------------------------------------------------------------------------
// Gate behaviour
// ---------------------------------------------------------------------------

#[test]
fn model_plus_output_is_counted() {
    let paths = build(
        "positive",
        &[("t-gpt", "gpt-6-sol")],
        &[("t-gpt", &["userMessage", "agentMessage"])],
        &[("t-gpt", "completed")],
    );
    let report = scan(&paths, &policy()).unwrap();
    assert_eq!(report.verified_families, vec!["gpt".to_string()]);
    assert_eq!(report.counts.countable_sessions, 1);
    assert_eq!(report.counts.observable_sessions, 0);
}

#[test]
fn model_without_output_is_only_observable() {
    // The measured F0 failure mode: a session row exists with a model but the
    // model never produced anything.
    let paths = build(
        "no-call",
        &[("t-idle", "gpt-6-sol")],
        &[("t-idle", &["userMessage"])],
        &[],
    );
    let report = scan(&paths, &policy()).unwrap();
    assert!(
        report.verified_families.is_empty(),
        "a session without model output must not light up"
    );
    assert_eq!(report.counts.observable_sessions, 1);
    assert_eq!(report.counts.countable_sessions, 0);
}

#[test]
fn task_label_threads_are_never_families_even_with_output() {
    let paths = build(
        "task-label",
        &[("t-guard", "codex-auto-review")],
        &[("t-guard", &["agentMessage", "reasoning"])],
        &[("t-guard", "completed")],
    );
    let report = scan(&paths, &policy()).unwrap();
    assert!(report.verified_families.is_empty());
    assert_eq!(report.counts.rejected_task_label, 1);
}

#[test]
fn empty_model_and_unknown_model_are_rejected() {
    let paths = build(
        "unknowns",
        &[("t-empty", ""), ("t-mystery", "mystery-model-zzz")],
        &[
            ("t-empty", &["agentMessage"]),
            ("t-mystery", &["agentMessage"]),
        ],
        &[],
    );
    let report = scan(&paths, &policy()).unwrap();
    assert!(report.verified_families.is_empty());
    assert_eq!(report.counts.rejected_empty_model, 1);
    assert_eq!(report.counts.rejected_unknown_family, 1);
}

#[test]
fn n_distinct_strings_collapse_to_one_family() {
    // F1 acceptance scenario A, on synthetic data.
    let strings = [
        "gpt-6-sol",
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-6.1-sol",
        "gpt-6-astra",
        "chatgpt-web/gpt-6-sol",
        "chatgpt-web/gpt-5.6-sol",
        "codex-auto-review",
        "",
    ];
    let threads: Vec<(&str, &str)> = strings
        .iter()
        .enumerate()
        .map(|(i, s)| (Box::leak(format!("t{i}").into_boxed_str()) as &str, *s))
        .collect();
    let items: Vec<(&str, &[&str])> = threads
        .iter()
        .map(|(id, _)| (*id, &["agentMessage"][..]))
        .collect();
    let paths = build("collapse", &threads, &items, &[]);

    let report = scan(&paths, &policy()).unwrap();
    assert_eq!(
        report.verified_families,
        vec!["gpt".to_string()],
        "9 raw strings must yield exactly 1 family"
    );
    assert_eq!(report.counts.sessions_seen, 9);
}

#[test]
fn missing_history_database_downgrades_to_observable() {
    let paths = build(
        "no-history",
        &[("t-a", "gpt-6-sol")],
        &[("t-a", &["agentMessage"])],
        &[],
    );
    let missing = CodexPaths::new(paths.state_db.clone(), None);
    let report = scan(&missing, &policy()).unwrap();
    assert!(report.verified_families.is_empty(), "no proof without the history db");
    assert_eq!(report.counts.observable_sessions, 1);
}

#[test]
fn missing_state_database_is_an_error_not_a_widened_search() {
    let mut path = workspace_tmp("does-not-exist");
    path.set_extension("absent");
    let paths = CodexPaths::new(path, None);
    let err = scan(&paths, &policy()).unwrap_err();
    assert!(matches!(err, ScanError::Guard(GuardError::Open(_))), "got {err:?}");
}

#[test]
fn scan_is_deterministic() {
    let paths = build(
        "deterministic",
        &[("t1", "gpt-6-sol"), ("t2", "codex-auto-review")],
        &[("t1", &["agentMessage"]), ("t2", &["agentMessage"])],
        &[],
    );
    let a = scan(&paths, &policy()).unwrap();
    let b = scan(&paths, &policy()).unwrap();
    assert_eq!(a.verified_families, b.verified_families);
    assert_eq!(a.counts.sessions_seen, b.counts.sessions_seen);
    let ja = serde_json::to_string(&a).unwrap();
    let jb = serde_json::to_string(&b).unwrap();
    assert_eq!(ja, jb);
}

// ---------------------------------------------------------------------------
// Privacy
// ---------------------------------------------------------------------------

#[test]
fn adapter_touches_only_allowlisted_columns() {
    let paths = build(
        "columns",
        &[("t1", "gpt-6-sol")],
        &[("t1", &["agentMessage"])],
        &[("t1", "completed")],
    );
    let report = scan(&paths, &policy()).unwrap();

    let allowed: BTreeMap<String, Vec<String>> = allowed_columns().into_iter().collect();
    let used = report.audit.columns_used();
    assert!(!used.is_empty());

    for column in &used {
        let ok = allowed.values().any(|cols| cols.iter().any(|c| c == column));
        assert!(ok, "adapter touched undeclared column `{column}`");
    }
    // Explicitly assert the forbidden ones never appear.
    for forbidden in [
        "first_user_message",
        "preview",
        "title",
        "item_json",
        "feedback_log_body",
        "cwd",
        "rollout_path",
        "tokens_used",
    ] {
        assert!(
            !used.contains(forbidden),
            "adapter must never touch `{forbidden}`"
        );
    }
}

#[test]
fn scan_report_leaks_no_content_and_no_raw_session_ids() {
    let paths = build(
        "leak",
        &[("thread-id-with-user-meaning", "gpt-6-sol")],
        &[("thread-id-with-user-meaning", &["agentMessage"])],
        &[],
    );
    let report = scan(&paths, &policy()).unwrap();
    let json = serde_json::to_string(&report).unwrap();

    for secret in [
        "SECRET_WORDS",
        "SECRET_PREVIEW",
        "SECRET_TITLE",
        "SECRET_JSON",
        "/Users/x/y",
        "thread-id-with-user-meaning",
    ] {
        assert!(
            !json.contains(secret),
            "report must not contain `{secret}`: {json}"
        );
    }
    // The session handle is a digest, short and non-reversible.
    assert_eq!(report.sessions[0].session_ref.len(), 12);
    assert!(report.sessions[0]
        .session_ref
        .chars()
        .all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn manifest_declares_local_call_proof_capability() {
    let m = manifest();
    m.validate().expect("manifest must be valid");
    assert_eq!(m.evidence_capability, EvidenceCapability::LocalCallProof);
    assert!(m.platforms.contains(&Platform::Macos));
    // The manifest must not contain a single globally forbidden column.
    for policy in &m.tables {
        for column in &policy.allow {
            assert!(
                !adapter_sdk::is_globally_denied(column),
                "manifest allows forbidden column `{column}`"
            );
        }
    }
}

#[test]
fn only_three_tables_and_seven_columns_are_reachable() {
    // A guard against silent scope creep: widening this list requires a test
    // update, which forces a review conversation.
    let columns = allowed_columns();
    let total: usize = columns.iter().map(|(_, c)| c.len()).sum();
    assert_eq!(columns.len(), 3, "tables: {:?}", columns);
    assert_eq!(total, 7, "columns: {:?}", columns);
}
