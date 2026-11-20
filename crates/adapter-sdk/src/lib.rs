//! Column-level controlled access to local database files.
//!
//! # Why a whole crate
//!
//! A promise that we "never read your conversations" is worth nothing if it
//! lives in a code comment. F0 established that usable metadata and forbidden
//! content sit in the **same SQLite file** — in the measured client,
//! `threads.model` is safe metadata while `threads.first_user_message` is the
//! user's own words.
//!
//! So the boundary must be at **column granularity**, and it must be enforced by
//! the only code path that can touch the file.
//!
//! # The four mechanisms
//!
//! 1. **A global deny list** ([`GLOBAL_DENY_COLUMNS`]) that no manifest can
//!    override. A manifest that tries is rejected at load time, not at query
//!    time.
//! 2. **Per-adapter allowlists** ([`AdapterManifest`]). Columns not listed are
//!    unreachable.
//! 3. **Generated SQL only.** The connection is private; queries are assembled
//!    from validated identifiers, so `SELECT *` cannot be expressed.
//! 4. **Read-only handle + audit.** The connection is opened read-only with
//!    `query_only` set, and every attempted query is recorded — including
//!    rejections.
//!
//! A rejection **never** causes a wider retry. There is no code path that
//! re-runs a failed query with looser rules (F1 authorisation §2.2).

#![forbid(unsafe_code)]

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Columns that may never be read from a client database, by anyone.
///
/// Grouped by why they are forbidden, because the reason matters when reviewing
/// a new adapter.
pub const GLOBAL_DENY_COLUMNS: &[&str] = &[
    // --- conversation content (product spec §12.3) ---
    "first_user_message",
    "item_json",
    "message_json",
    "messages",
    "preview",
    "feedback_log_body",
    "prompt",
    "prompts",
    "text",
    "content",
    "body",
    "conversation",
    "transcript",
    "title",
    // --- credentials and secrets ---
    "api_key",
    "apikey",
    "authorization",
    "token",
    "access_token",
    "refresh_token",
    "secret",
    "password",
    "cookie",
    "cookies",
    // --- personal contact and precise location ---
    "email",
    "phone",
    "address",
    "latitude",
    "longitude",
    // --- filesystem detail that reveals the user's projects ---
    "rollout_path",
    "file_path",
    "cwd",
    "agent_path",
    "git_origin_url",
    // --- consumption counters (product spec §1.5: never score consumption) ---
    "tokens_used",
    "tokens",
    "cost",
    "usage_cost",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Macos,
    Windows,
    Ios,
    Android,
}

/// What evidence an adapter is capable of producing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCapability {
    /// Config/app presence only (E0/E1).
    DiscoverOnly,
    /// Can prove a call from local metadata (E3).
    LocalCallProof,
    /// Can obtain a signed receipt (E4).
    CallReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TablePolicy {
    pub table: String,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

impl TablePolicy {
    pub fn new(table: &str, allow: &[&str]) -> Self {
        Self {
            table: table.to_string(),
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: Vec::new(),
        }
    }

    /// Explicitly deny a column even though it might otherwise be allowed.
    pub fn deny(mut self, columns: &[&str]) -> Self {
        self.deny.extend(columns.iter().map(|s| s.to_string()));
        self
    }

    pub fn allows(&self, column: &str) -> bool {
        self.allow.iter().any(|c| c == column) && !self.deny.iter().any(|c| c == column)
    }
}

/// The declaration every adapter must ship. Reviewing this file is enough to
/// know exactly which columns an adapter can reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterManifest {
    pub id: String,
    pub version: String,
    pub platforms: Vec<Platform>,
    pub tables: Vec<TablePolicy>,
    pub evidence_capability: EvidenceCapability,
    /// Hard ceiling on rows read per session.
    pub max_rows: usize,
    /// Hard ceiling on wall-clock time per session.
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("adapter id must not be empty")]
    EmptyId,
    #[error("adapter `{adapter}` declares no tables")]
    NoTables { adapter: String },
    #[error("table `{table}` is declared twice")]
    DuplicateTable { table: String },
    #[error("table `{table}` allows globally forbidden column `{column}`")]
    GloballyForbiddenColumn { table: String, column: String },
    #[error("table `{table}` both allows and denies column `{column}`")]
    ContradictoryPolicy { table: String, column: String },
    #[error("table `{table}` lists invalid identifier `{column}`")]
    InvalidIdentifier { table: String, column: String },
    #[error("manifest budgets must be positive")]
    InvalidBudget,
}

impl AdapterManifest {
    /// Validate at load time. A manifest that violates the global deny list can
    /// never be used, so the failure surfaces in CI rather than in production.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.trim().is_empty() {
            return Err(ManifestError::EmptyId);
        }
        if self.tables.is_empty() {
            return Err(ManifestError::NoTables {
                adapter: self.id.clone(),
            });
        }
        if self.max_rows == 0 || self.timeout_ms == 0 {
            return Err(ManifestError::InvalidBudget);
        }

        let mut seen = std::collections::BTreeSet::new();
        for policy in &self.tables {
            if !is_valid_identifier(&policy.table) {
                return Err(ManifestError::InvalidIdentifier {
                    table: policy.table.clone(),
                    column: policy.table.clone(),
                });
            }
            if !seen.insert(policy.table.clone()) {
                return Err(ManifestError::DuplicateTable {
                    table: policy.table.clone(),
                });
            }
            for column in &policy.allow {
                if !is_valid_identifier(column) {
                    return Err(ManifestError::InvalidIdentifier {
                        table: policy.table.clone(),
                        column: column.clone(),
                    });
                }
                if is_globally_denied(column) {
                    return Err(ManifestError::GloballyForbiddenColumn {
                        table: policy.table.clone(),
                        column: column.clone(),
                    });
                }
                if policy.deny.iter().any(|d| d == column) {
                    return Err(ManifestError::ContradictoryPolicy {
                        table: policy.table.clone(),
                        column: column.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn policy_for(&self, table: &str) -> Option<&TablePolicy> {
        self.tables.iter().find(|t| t.table == table)
    }
}

pub fn is_globally_denied(column: &str) -> bool {
    let lower = column.to_ascii_lowercase();
    GLOBAL_DENY_COLUMNS.iter().any(|d| *d == lower)
}

fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ---------------------------------------------------------------------------
// Query specification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Text(String),
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOp {
    Eq,
    In,
    GreaterThan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub column: String,
    pub op: FilterOp,
    pub values: Vec<Value>,
}

impl Filter {
    pub fn eq(column: &str, value: impl Into<Value>) -> Self {
        Self {
            column: column.to_string(),
            op: FilterOp::Eq,
            values: vec![value.into()],
        }
    }

    pub fn is_in(column: &str, values: Vec<Value>) -> Self {
        Self {
            column: column.to_string(),
            op: FilterOp::In,
            values,
        }
    }

    pub fn gt(column: &str, value: impl Into<Value>) -> Self {
        Self {
            column: column.to_string(),
            op: FilterOp::GreaterThan,
            values: vec![value.into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowsSpec {
    pub table: String,
    pub columns: Vec<String>,
    pub filter: Option<Filter>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCountSpec {
    pub table: String,
    pub group_column: String,
    pub filter: Option<Filter>,
    pub limit: usize,
}

/// One returned row, keyed by column name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub values: BTreeMap<String, Value>,
}

impl Row {
    pub fn get(&self, column: &str) -> Option<&Value> {
        self.values.get(column)
    }

    pub fn get_int(&self, column: &str) -> Option<i64> {
        match self.get(column) {
            Some(Value::Int(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn get_text(&self, column: &str) -> Option<&str> {
        match self.get(column) {
            Some(Value::Text(v)) => Some(v.as_str()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub table: String,
    pub columns: Vec<String>,
    pub kind: &'static str,
    pub rows_returned: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub table: String,
    pub column: String,
    pub reason: String,
}

/// What the adapter touched. Contains **column names only**, never values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryAudit {
    pub entries: Vec<AuditEntry>,
    pub rejections: Vec<Rejection>,
    pub rows_read: usize,
}

impl QueryAudit {
    pub fn columns_used(&self) -> std::collections::BTreeSet<&str> {
        self.entries
            .iter()
            .flat_map(|e| e.columns.iter().map(|c| c.as_str()))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GuardError {
    #[error("manifest invalid: {0}")]
    Manifest(#[from] ManifestError),
    #[error("table `{table}` is not declared by this adapter")]
    TableNotAllowed { table: String },
    #[error("column `{column}` is not allowed on table `{table}`")]
    ColumnNotAllowed { table: String, column: String },
    #[error("column `{column}` is globally forbidden")]
    GloballyForbidden { column: String },
    #[error("row budget exceeded ({budget} rows)")]
    RowBudgetExceeded { budget: usize },
    #[error("time budget exceeded ({budget_ms} ms)")]
    TimeBudgetExceeded { budget_ms: u64 },
    #[error("local database could not be opened read-only: {0}")]
    Open(String),
    #[error("query failed: {0}")]
    Sql(String),
    #[error("non-positive limit {0} requested")]
    InvalidLimit(usize),
}

// ---------------------------------------------------------------------------
// The guard
// ---------------------------------------------------------------------------

/// How the local database was ultimately opened.
///
/// SQLite cannot open a **live WAL database** with `SQLITE_OPEN_READ_ONLY`: the
/// reader needs write access to the `-shm` sidecar, so the open appears to
/// succeed and then the first query fails with "unable to open database file".
/// Measured against a real client database during F1.
///
/// So there are two read-only strategies, and which one was used is reported
/// rather than hidden:
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenMode {
    /// Opened with `SQLITE_OPEN_READ_ONLY`. Preferred; impossible on live WAL.
    ReadOnly,
    /// Opened read-write at the OS level but with `PRAGMA query_only = 1`, which
    /// makes SQLite reject every write statement. Used only when the strict
    /// mode cannot read the file.
    QueryOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenOptions {
    /// Permit the `QueryOnly` fallback when strict read-only cannot read the
    /// file. This changes the *access mode*, never the scan scope.
    pub allow_query_only_fallback: bool,
}

impl OpenOptions {
    pub fn strict() -> Self {
        Self {
            allow_query_only_fallback: false,
        }
    }

    pub fn compatible() -> Self {
        Self {
            allow_query_only_fallback: true,
        }
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::compatible()
    }
}

/// The only sanctioned reader for a local client database.
///
/// The inner `Connection` is private on purpose: there is no accessor, so code
/// outside this module cannot issue arbitrary SQL.
pub struct GuardedDb {
    conn: Connection,
    manifest: AdapterManifest,
    audit: QueryAudit,
    started: Instant,
    path: PathBuf,
    mode: OpenMode,
}

impl std::fmt::Debug for GuardedDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the path: it contains the user's home directory.
        f.debug_struct("GuardedDb")
            .field("adapter", &self.manifest.id)
            .field("rows_read", &self.audit.rows_read)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

impl GuardedDb {
    /// Open with the strict read-only strategy.
    pub fn open_readonly(path: &Path, manifest: AdapterManifest) -> Result<Self, GuardError> {
        Self::open(path, manifest, OpenOptions::strict())
    }

    /// Open, falling back to `QueryOnly` when a live WAL database cannot be read
    /// in strict read-only mode. Still cannot write.
    pub fn open_compatible(path: &Path, manifest: AdapterManifest) -> Result<Self, GuardError> {
        Self::open(path, manifest, OpenOptions::compatible())
    }

    /// Open a database read-only. No search for other files, no scope widening.
    pub fn open(
        path: &Path,
        manifest: AdapterManifest,
        options: OpenOptions,
    ) -> Result<Self, GuardError> {
        manifest.validate()?;

        let (conn, mode) = match open_connection(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(conn) => {
                // Probe: on a live WAL database the open succeeds and reading
                // fails, so a successful open alone is not proof.
                match probe_read(&conn) {
                    Ok(()) => {
                        // Belt and braces in both modes: no write statement can
                        // even be prepared.
                        enforce_query_only(&conn)?;
                        (conn, OpenMode::ReadOnly)
                    }
                    Err(_) => {
                        if !options.allow_query_only_fallback {
                            return Err(GuardError::Open("database cannot be read read-only".into()));
                        }
                        let conn = open_connection(
                            path,
                            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                        )?;
                        enforce_query_only(&conn)?;
                        probe_read(&conn).map_err(|_| GuardError::Open("database cannot be read".into()))?;
                        (conn, OpenMode::QueryOnly)
                    }
                }
            }
            Err(_) => {
                if !options.allow_query_only_fallback {
                    return Err(GuardError::Open("database cannot be opened read-only".into()));
                }
                let conn = open_connection(
                    path,
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                enforce_query_only(&conn)?;
                probe_read(&conn).map_err(|_| GuardError::Open("database cannot be read".into()))?;
                (conn, OpenMode::QueryOnly)
            }
        };

        Ok(Self {
            conn,
            manifest,
            audit: QueryAudit::default(),
            started: Instant::now(),
            path: path.to_path_buf(),
            mode,
        })
    }

    /// Which read-only strategy actually worked.
    pub fn open_mode(&self) -> OpenMode {
        self.mode
    }

    pub fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    pub fn audit(&self) -> &QueryAudit {
        &self.audit
    }

    /// True when the handle really is read-only (verified by asking SQLite).
    pub fn is_read_only(&self) -> bool {
        self.conn
            .query_row("PRAGMA query_only", [], |row| row.get::<_, i64>(0))
            .map(|v| v == 1)
            .unwrap_or(false)
    }

    /// Deliberately opaque: the path is never rendered, only compared.
    pub fn same_file(&self, other: &Path) -> bool {
        self.path == other
    }

    // -- query entry points --------------------------------------------------

    pub fn rows(&mut self, spec: RowsSpec) -> Result<Vec<Row>, GuardError> {
        if spec.limit == 0 {
            return Err(GuardError::InvalidLimit(0));
        }
        let effective = self.precheck(spec.limit)?;
        let policy = self.policy(&spec.table)?.clone();
        for column in &spec.columns {
            self.check_column(&policy, column)?;
        }
        if let Some(filter) = &spec.filter {
            self.check_column(&policy, &filter.column)?;
        }

        let mut sql = String::from("SELECT ");
        sql.push_str(
            &spec
                .columns
                .iter()
                .map(|c| format!("\"{c}\""))
                .collect::<Vec<_>>()
                .join(", "),
        );
        sql.push_str(&format!(" FROM \"{}\"", spec.table));
        let mut params: Vec<SqlValue> = Vec::new();
        if let Some(filter) = &spec.filter {
            sql.push_str(" WHERE ");
            sql.push_str(&render_filter(filter, &mut params));
        }
        sql.push_str(&format!(" LIMIT {effective}"));
        self.ensure_filter_only_star(&sql, &spec.table)?;

        let column_names = spec.columns.clone();
        let rows: Vec<Row> = {
            let mut stmt = self.conn.prepare(&sql).map_err(|e| GuardError::Sql(e.to_string()))?;
            let iter = stmt
                .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                    let mut values = BTreeMap::new();
                    for (idx, name) in column_names.iter().enumerate() {
                        let raw = row.get_ref(idx)?;
                        let value = match raw {
                            rusqlite::types::ValueRef::Integer(v) => Value::Int(v),
                            rusqlite::types::ValueRef::Text(t) => {
                                Value::Text(String::from_utf8_lossy(t).to_string())
                            }
                            rusqlite::types::ValueRef::Real(f) => Value::Int(f as i64),
                            rusqlite::types::ValueRef::Null => Value::Text(String::new()),
                            rusqlite::types::ValueRef::Blob(b) => {
                                // Blobs are never evidence. Record only the length.
                                Value::Text(format!("<blob:{}>", b.len()))
                            }
                        };
                        values.insert(name.clone(), value);
                    }
                    Ok(Row { values })
                })
                .map_err(|e| GuardError::Sql(e.to_string()))?;
            iter.collect::<Result<Vec<_>, _>>()
                .map_err(|e| GuardError::Sql(e.to_string()))?
        };

        if rows.len() >= effective {
            // Truncated: we cannot prove we saw every session, so we refuse
            // rather than report a partial (wrong) answer.
            return Err(GuardError::RowBudgetExceeded {
                budget: self.manifest.max_rows,
            });
        }
        let mut touched = spec.columns.clone();
        if let Some(filter) = &spec.filter {
            touched.push(filter.column.clone());
        }
        self.record(&spec.table, touched, "rows", rows.len());
        Ok(rows)
    }

    pub fn count(&mut self, table: &str, filter: Option<Filter>) -> Result<i64, GuardError> {
        let _ = self.precheck(1)?;
        let policy = self.policy(table)?.clone();
        if let Some(f) = &filter {
            self.check_column(&policy, &f.column)?;
        }
        let mut sql = format!("SELECT count(*) FROM \"{table}\"");
        let mut params: Vec<SqlValue> = Vec::new();
        if let Some(filter) = &filter {
            sql.push_str(" WHERE ");
            sql.push_str(&render_filter(filter, &mut params));
        }
        self.ensure_filter_only_star(&sql, table)?;

        let count: i64 = self
            .conn
            .query_row(&sql, rusqlite::params_from_iter(params.iter()), |row| row.get(0))
            .map_err(|e| GuardError::Sql(e.to_string()))?;

        let cols: Vec<String> = filter.map(|f| vec![f.column]).unwrap_or_default();
        self.record(table, cols, "count", 1);
        Ok(count)
    }

    /// `SELECT group_column, count(*) ... GROUP BY group_column`.
    ///
    /// This is the shape that lets an adapter answer "how many model-output
    /// items exist per session" in one pass instead of one query per session.
    pub fn group_count(&mut self, spec: GroupCountSpec) -> Result<Vec<(Value, i64)>, GuardError> {
        if spec.limit == 0 {
            return Err(GuardError::InvalidLimit(0));
        }
        let effective = self.precheck(spec.limit)?;
        let policy = self.policy(&spec.table)?.clone();
        self.check_column(&policy, &spec.group_column)?;
        if let Some(filter) = &spec.filter {
            self.check_column(&policy, &filter.column)?;
        }

        let mut sql = format!(
            "SELECT \"{}\", count(*) FROM \"{}\"",
            spec.group_column, spec.table
        );
        let mut params: Vec<SqlValue> = Vec::new();
        if let Some(filter) = &spec.filter {
            sql.push_str(" WHERE ");
            sql.push_str(&render_filter(filter, &mut params));
        }
        sql.push_str(&format!(" GROUP BY \"{}\" LIMIT {effective}", spec.group_column));
        self.ensure_filter_only_star(&sql, &spec.table)?;

        let rows: Vec<(Value, i64)> = {
            let mut stmt = self.conn.prepare(&sql).map_err(|e| GuardError::Sql(e.to_string()))?;
            let iter = stmt
                .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                    let raw = row.get_ref(0)?;
                    let key = match raw {
                        rusqlite::types::ValueRef::Integer(v) => Value::Int(v),
                        rusqlite::types::ValueRef::Text(t) => {
                            Value::Text(String::from_utf8_lossy(t).to_string())
                        }
                        _ => Value::Text(String::new()),
                    };
                    let count: i64 = row.get(1)?;
                    Ok((key, count))
                })
                .map_err(|e| GuardError::Sql(e.to_string()))?;
            iter.collect::<Result<Vec<_>, _>>()
                .map_err(|e| GuardError::Sql(e.to_string()))?
        };

        if rows.len() >= effective {
            return Err(GuardError::RowBudgetExceeded {
                budget: self.manifest.max_rows,
            });
        }
        let mut touched = vec![spec.group_column.clone()];
        if let Some(filter) = &spec.filter {
            touched.push(filter.column.clone());
        }
        self.record(&spec.table, touched, "group_count", rows.len());
        Ok(rows)
    }

    // -- internals ----------------------------------------------------------

    fn policy(&self, table: &str) -> Result<&TablePolicy, GuardError> {
        self.manifest
            .policy_for(table)
            .ok_or_else(|| GuardError::TableNotAllowed {
                table: table.to_string(),
            })
    }

    fn check_column(&mut self, policy: &TablePolicy, column: &str) -> Result<(), GuardError> {
        if is_globally_denied(column) {
            self.audit.rejections.push(Rejection {
                table: policy.table.clone(),
                column: column.to_string(),
                reason: "globally_forbidden".into(),
            });
            return Err(GuardError::GloballyForbidden {
                column: column.to_string(),
            });
        }
        if !policy.allows(column) {
            self.audit.rejections.push(Rejection {
                table: policy.table.clone(),
                column: column.to_string(),
                reason: "not_in_allowlist".into(),
            });
            return Err(GuardError::ColumnNotAllowed {
                table: policy.table.clone(),
                column: column.to_string(),
            });
        }
        Ok(())
    }

    /// Resolve the effective row limit for this query.
    ///
    /// Returns `min(requested, remaining budget)`. Callers must treat "the query
    /// returned exactly the effective limit" as truncation and fail, because a
    /// partial scan would silently under-count and report a wrong light level.
    fn precheck(&self, requested: usize) -> Result<usize, GuardError> {
        if self.started.elapsed() > Duration::from_millis(self.manifest.timeout_ms) {
            return Err(GuardError::TimeBudgetExceeded {
                budget_ms: self.manifest.timeout_ms,
            });
        }
        let remaining = self
            .manifest
            .max_rows
            .saturating_sub(self.audit.rows_read);
        if remaining == 0 {
            return Err(GuardError::RowBudgetExceeded {
                budget: self.manifest.max_rows,
            });
        }
        Ok(requested.min(remaining))
    }

    fn ensure_filter_only_star(&mut self, sql: &str, table: &str) -> Result<(), GuardError> {
        if !has_no_bare_star(sql) {
            self.audit.rejections.push(Rejection {
                table: table.to_string(),
                column: "*".into(),
                reason: "select_star".into(),
            });
            return Err(GuardError::ColumnNotAllowed {
                table: table.to_string(),
                column: "*".into(),
            });
        }
        Ok(())
    }

    fn record(&mut self, table: &str, columns: Vec<String>, kind: &'static str, rows: usize) {
        self.audit.rows_read = self.audit.rows_read.saturating_add(rows);
        self.audit.entries.push(AuditEntry {
            table: table.to_string(),
            columns,
            kind,
            rows_returned: rows,
        });
    }
}

fn open_connection(path: &Path, flags: OpenFlags) -> Result<Connection, GuardError> {
    let conn = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| GuardError::Open(sanitize(&e).to_string()))?;
    Ok(conn)
}

fn enforce_query_only(conn: &Connection) -> Result<(), GuardError> {
    conn.pragma_update(None, "query_only", true)
        .map_err(|e| GuardError::Open(sanitize(&e).to_string()))
}

/// Touch the schema: this is what fails on an unreadable WAL database even
/// though the open call itself succeeded.
fn probe_read(conn: &Connection) -> Result<(), GuardError> {
    conn.query_row("select count(*) from sqlite_master", [], |r| r.get::<_, i64>(0))
        .map(|_| ())
        .map_err(|e| GuardError::Sql(e.to_string()))
}

fn render_filter(filter: &Filter, params: &mut Vec<SqlValue>) -> String {
    match filter.op {
        FilterOp::Eq => {
            params.push(to_sql(&filter.values[0]));
            format!("\"{}\" = ?", filter.column)
        }
        FilterOp::GreaterThan => {
            params.push(to_sql(&filter.values[0]));
            format!("\"{}\" > ?", filter.column)
        }
        FilterOp::In => {
            let placeholders = filter
                .values
                .iter()
                .map(|v| {
                    params.push(to_sql(v));
                    "?"
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("\"{}\" IN ({placeholders})", filter.column)
        }
    }
}

fn to_sql(value: &Value) -> SqlValue {
    match value {
        Value::Int(v) => SqlValue::Integer(*v),
        Value::Text(v) => SqlValue::Text(v.clone()),
    }
}

/// `count(*)` is the only legal asterisk; anything else would mean `SELECT *`.
pub fn has_no_bare_star(sql: &str) -> bool {
    let without_count = sql.replace("count(*)", "").replace("COUNT(*)", "");
    !without_count.contains('*')
}

fn sanitize(err: &rusqlite::Error) -> &'static str {
    // Never echo the path or the SQL: both can contain the user's directory names.
    match err {
        rusqlite::Error::SqliteFailure(e, _) => match e.code {
            rusqlite::ErrorCode::CannotOpen => "cannot open database",
            rusqlite::ErrorCode::DatabaseBusy => "database is busy",
            rusqlite::ErrorCode::NotADatabase => "not a database",
            _ => "database error",
        },
        rusqlite::Error::QueryReturnedNoRows => "no rows",
        _ => "database error",
    }
}

#[cfg(test)]
mod tests;
