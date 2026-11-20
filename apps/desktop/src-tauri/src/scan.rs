//! Local evidence scanning, wired from the UI down to the core.
//!
//! Two rules shape this module:
//!
//! 1. **No business logic here.** Whether a session counts, what the title is,
//!    what colour the light becomes — all decided in `light-core`. This module
//!    only discovers paths, calls the adapter, persists evidence, and shapes the
//!    result for the UI.
//! 2. **No silent widening.** If scanning is not authorised, or the client data
//!    is missing, the outcome says so. It never falls back to a broader search.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use adapter_codex::{scan, CodexPaths, ScanReport};
use light_core::{CountableEvidence, EvidenceDecision, EvidencePolicy, LightState};
use light_store::LocalStore;
use serde::{Deserialize, Serialize};

/// Where the personal light is stored.
pub const LIGHT_DB: &str = "light.sqlite";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    /// Scan completed.
    Ok,
    /// The user has not granted scanning permission.
    NotAuthorized,
    /// Permission granted, but no supported client data was found.
    ClientNotFound,
    /// Permission granted, client found, but the OS refused access.
    AccessDenied,
    /// Permission granted, client found, but the data source could not be read.
    Unreadable,
    /// Permission granted, client found, but reading failed for another reason.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyView {
    pub family: String,
    /// Highest evidence level reached across sessions: `E3` or `E1`.
    pub level: String,
    /// How many sessions contributed.
    pub sessions: usize,
    /// Raw model strings seen (public model names, never user content).
    pub raw_models: Vec<String>,
    pub countable: bool,
}

/// The parts of the light state worth persisting with a scan snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightSnapshot {
    pub level: String,
    pub knowledge_slug: String,
    pub knowledge_zh: String,
    pub verified_families: u32,
    pub attested_families: u32,
    pub activated_nodes: u32,
}

impl From<&LightState> for LightSnapshot {
    fn from(s: &LightState) -> Self {
        Self {
            level: format!("{:?}", s.level).to_uppercase(),
            knowledge_slug: s.knowledge_title.slug().to_string(),
            knowledge_zh: s.knowledge_title.display_zh().to_string(),
            verified_families: s.verified_families,
            attested_families: s.attested_families,
            activated_nodes: s.activated_nodes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOutcome {
    pub status: ScanStatus,
    /// Human-readable reason when status is not `Ok`.
    pub reason: Option<String>,
    pub at_ms: i64,
    pub duration_ms: u64,
    pub sessions_seen: usize,
    pub countable_sessions: usize,
    pub observable_sessions: usize,
    pub rejected_task_label: usize,
    pub rejected_unknown: usize,
    pub rejected_empty: usize,
    pub verified_families: Vec<String>,
    pub newly_verified: Vec<String>,
    pub families: Vec<FamilyView>,
    pub light: Option<LightSnapshot>,
    /// Read-only strategy actually used, reported not hidden (ADR-012).
    pub state_open_mode: Option<String>,
    pub history_open_mode: Option<String>,
    /// Columns the adapter touched. Empty on failure.
    pub columns_touched: Vec<String>,
}

impl ScanOutcome {
    pub fn simple(status: ScanStatus, reason: impl Into<String>, at_ms: i64) -> Self {
        Self {
            status,
            reason: Some(reason.into()),
            at_ms,
            duration_ms: 0,
            sessions_seen: 0,
            countable_sessions: 0,
            observable_sessions: 0,
            rejected_task_label: 0,
            rejected_unknown: 0,
            rejected_empty: 0,
            verified_families: vec![],
            newly_verified: vec![],
            families: vec![],
            light: None,
            state_open_mode: None,
            history_open_mode: None,
            columns_touched: vec![],
        }
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Path discovery — explicit, shallow, never recursive
// ---------------------------------------------------------------------------

/// Locate the Codex client databases.
///
/// Deliberately shallow: exactly one directory, a known filename prefix, no
/// recursion and no directory walking. `FIRSTLIGHT_CODEX_DIR` exists so tests
/// can point at a fixture without touching the real client.
pub fn codex_paths() -> Option<CodexPaths> {
    let dir = match std::env::var_os("FIRSTLIGHT_CODEX_DIR") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".codex"),
    };
    if !dir.is_dir() {
        return None;
    }
    let state = newest_matching(&dir, "state_", ".sqlite")?;
    let history = newest_matching(&dir, "thread_history_", ".sqlite");
    Some(CodexPaths::new(state, history))
}

/// Pick the highest-numbered `<prefix><n>.sqlite` in one directory.
fn newest_matching(dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(mid) = name
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
        else {
            continue;
        };
        let Ok(n) = mid.parse::<u64>() else { continue };
        if best.as_ref().map(|(bn, _)| n > *bn).unwrap_or(true) {
            best = Some((n, entry.path()));
        }
    }
    best.map(|(_, p)| p)
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------
//
// There is exactly one place that decides whether a scan may touch the local
// model-client database. Every caller — the panel button, the transfer flow, a
// future background job — goes through `authorize`. This exists because F3.5
// shipped `settings.scan_enabled || true`, which made the permission switch
// meaningless: a transfer scan ran even with scanning switched off.

use std::sync::atomic::{AtomicU64, Ordering};

/// How many times a scan actually began reading on-disk client data.
///
/// A denied scan never increments this, which is what makes the negative tests
/// meaningful: they assert on reads performed, not on messages returned.
static SCAN_EXECUTIONS: AtomicU64 = AtomicU64::new(0);

pub fn executions() -> u64 {
    SCAN_EXECUTIONS.load(Ordering::SeqCst)
}

#[cfg(test)]
pub fn reset_executions_for_test() {
    SCAN_EXECUTIONS.store(0, Ordering::SeqCst);
}

/// Why a scan was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanDenial {
    /// The standing permission switch is off.
    Disabled,
    /// The switch is on, but this particular scan was not explicitly confirmed.
    NotConfirmed,
}

impl ScanDenial {
    pub fn message(self) -> &'static str {
        match self {
            ScanDenial::Disabled => {
                "自动扫描已关闭。光点保持沉睡，不会读取本机任何数据。"
            }
            ScanDenial::NotConfirmed => {
                "扫描需要你这次明确同意。授权开关已打开，但本次尚未确认。"
            }
        }
    }
}

/// The single authorization decision.
///
/// Both conditions are required: the standing switch AND a per-scan confirmation.
/// A transfer, a page transition or a restart must never supply the second one.
pub fn authorize(scan_enabled: bool, user_confirmed: bool) -> Result<(), ScanDenial> {
    if !scan_enabled {
        return Err(ScanDenial::Disabled);
    }
    if !user_confirmed {
        return Err(ScanDenial::NotConfirmed);
    }
    Ok(())
}


/// Classify a reader error at the adapter boundary.
///
/// Kept as a pure function so the three outcomes can be asserted independently:
/// "the OS refused us", "the file is there but unreadable as a database", and the
/// caller's separate `ClientNotFound` when discovery finds nothing at all.
pub fn classify_read_error(raw: &str) -> ScanStatus {
    let t = raw.to_lowercase();
    // The adapter flattens sqlite's error codes into two distinct sentences:
    //   EACCES / ENOENT at open  -> "cannot open database"
    //   opened but unreadable    -> "database cannot be read"
    // That distinction is real (verified against a chmod-000 source and a corrupted
    // file), so it is the boundary we classify on. It is string-coupled: if the
    // adapter changes its wording, `read_errors_classify_into_the_right_status`
    // fails and this mapping has to be revisited with it.
    let denied = t.contains("operation not permitted")
        || t.contains("permission denied")
        || t.contains("access denied")
        || t.contains("not authorized")
        || t.contains("unable to open database file")
        || t.contains("cannot open database");
    if denied {
        return ScanStatus::AccessDenied;
    }
    ScanStatus::Unreadable
}

/// Run one scan and persist whatever it proves.
pub fn run(state_dir: &Path, scan_enabled: bool) -> ScanOutcome {
    let at = now_ms();
    if !scan_enabled {
        // Not an error: the product is fully usable without scanning.
        return ScanOutcome::simple(
            ScanStatus::NotAuthorized,
            "尚未授权扫描本机 AI 使用情况。光点保持沉睡，不会读取任何数据。",
            at,
        );
    }

    // Past this point the process touches on-disk client data.
    SCAN_EXECUTIONS.fetch_add(1, Ordering::SeqCst);

    let Some(paths) = codex_paths() else {
        return ScanOutcome::simple(
            ScanStatus::ClientNotFound,
            "未找到支持的 AI 客户端。安装后可以再试一次。",
            at,
        );
    };

    let started = Instant::now();
    let policy = EvidencePolicy::v1();
    let report = match scan(&paths, &policy) {
        Ok(r) => r,
        Err(e) => {
            // The raw error can contain a database path or an SDK message, so it is
            // logged for the developer and never handed to the UI.
            eprintln!("scan read error: {e}");
            let status = classify_read_error(&e.to_string());
            let reason = match &status {
                ScanStatus::AccessDenied => "系统拒绝了访问本机数据。可以在系统设置中检查权限后重试。",
                _ => "数据源暂时无法读取。稍后可以再试一次。",
            };
            return ScanOutcome::simple(status, reason, at);
        }
    };

    let mut store = match LocalStore::open(&state_dir.join(LIGHT_DB)) {
        Ok(s) => s,
        Err(e) => {
            return ScanOutcome::simple(
                ScanStatus::Failed,
                format!("本地光点库不可用：{e}"),
                at,
            )
        }
    };

    // Persist countable evidence. The store dedupes by family, so re-scanning is
    // idempotent and can never raise the level twice for one family.
    let mut newly: Vec<String> = Vec::new();
    for decision in &report.sessions {
        if let EvidenceDecision::Countable { family, level, .. } = &decision.decision {
            let ev = CountableEvidence {
                event_id: decision.session_ref.clone(),
                family: family.clone(),
                level: *level,
                proof_method: light_core::ProofMethod::LocalSqliteMetadata,
                adapter_id: report.adapter_id.clone(),
                adapter_version: report.adapter_version.clone(),
                registry_semver: report.registry_semver.clone(),
                observed_at_ms: at,
            };
            match store.record_countable_evidence(&ev) {
                Ok(true) => {
                    if !newly.contains(family) {
                        newly.push(family.clone());
                    }
                }
                Ok(false) => {}
                Err(e) => eprintln!("record evidence failed: {e}"),
            }
        }
    }

    let light = store.light_state(0).ok().map(|s| LightSnapshot::from(&s));

    let mut status = ScanStatus::Ok;
    let mut reason: Option<String> = None;
    if report.verified_families.is_empty() && report.counts.observable_sessions == 0 {
        status = ScanStatus::ClientNotFound;
        reason = Some("找到客户端数据，但没有任何可确认的模型使用记录。".to_string());
    }

    ScanOutcome {
        status,
        reason,
        at_ms: at,
        duration_ms: started.elapsed().as_millis() as u64,
        sessions_seen: report.counts.sessions_seen,
        countable_sessions: report.counts.countable_sessions,
        observable_sessions: report.counts.observable_sessions,
        rejected_task_label: report.counts.rejected_task_label,
        rejected_unknown: report.counts.rejected_unknown_family,
        rejected_empty: report.counts.rejected_empty_model,
        verified_families: report.verified_families.clone(),
        newly_verified: newly,
        families: family_views(&report),
        light,
        state_open_mode: Some(mode_name(&report, true)),
        history_open_mode: report.history_open_mode.map(|m| format!("{m:?}")),
        columns_touched: report
            .audit
            .columns_used()
            .into_iter()
            .map(String::from)
            .collect(),
    }
}

pub const LAST_SCAN_FILE: &str = "last-scan.json";

pub fn save_last(state_dir: &Path, outcome: &ScanOutcome) {
    let path = state_dir.join(LAST_SCAN_FILE);
    if let Ok(text) = serde_json::to_string_pretty(outcome) {
        let tmp = state_dir.join(format!("{LAST_SCAN_FILE}.tmp"));
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

pub fn load_last(state_dir: &Path) -> Option<ScanOutcome> {
    let text = std::fs::read_to_string(state_dir.join(LAST_SCAN_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

fn mode_name(report: &ScanReport, state: bool) -> String {
    let mode = if state {
        report.state_open_mode
    } else {
        report.history_open_mode.unwrap_or(report.state_open_mode)
    };
    format!("{mode:?}")
}

/// Collapse per-session verdicts into one row per family, for the UI.
fn family_views(report: &ScanReport) -> Vec<FamilyView> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, FamilyView> = BTreeMap::new();

    for session in &report.sessions {
        let (family, countable, level) = match &session.decision {
            EvidenceDecision::Countable { family, .. } => (family.clone(), true, "E3"),
            EvidenceDecision::Observable { level, normalization, .. } => {
                // Only meaningful if the model string resolved to a family.
                match normalization.family.clone() {
                    Some(f) => (f, false, if level.as_u8() == 1 { "E1" } else { "E2" }),
                    None => continue,
                }
            }
            EvidenceDecision::Rejected { .. } => continue,
        };

        let raw = session.decision.normalization().raw.clone();
        let entry = map.entry(family.clone()).or_insert_with(|| FamilyView {
            family: family.clone(),
            level: if countable { "E3".into() } else { level.into() },
            sessions: 0,
            raw_models: vec![],
            countable: false,
        });
        entry.sessions += 1;
        entry.countable |= countable;
        if countable {
            entry.level = "E3".into();
        }
        if !raw.is_empty() && !entry.raw_models.contains(&raw) && entry.raw_models.len() < 6 {
            entry.raw_models.push(raw);
        }
    }

    map.into_values().collect()
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
        d.push("scan-tests");
        d.push(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn scanning_without_permission_reads_nothing() {
        let d = tmp("no-permission");
        let out = run(&d, false);
        assert_eq!(out.status, ScanStatus::NotAuthorized);
        assert_eq!(out.sessions_seen, 0);
        assert!(out.verdict_is_quiet(), "must not look like a failure");
    }

    #[test]
    fn missing_client_is_reported_not_searched_for() {
        let d = tmp("no-client");
        // Point discovery at an empty directory.
        let empty = tmp("empty-client");
        std::env::set_var("FIRSTLIGHT_CODEX_DIR", &empty);
        let out = run(&d, true);
        std::env::remove_var("FIRSTLIGHT_CODEX_DIR");
        assert_eq!(out.status, ScanStatus::ClientNotFound);
        assert!(out.reason.unwrap().contains("未找到"));
    }

    #[test]
    fn newest_matching_picks_the_highest_number() {
        let d = tmp("newest");
        std::fs::write(d.join("state_1.sqlite"), b"").unwrap();
        std::fs::write(d.join("state_10.sqlite"), b"").unwrap();
        std::fs::write(d.join("state_2.sqlite"), b"").unwrap();
        std::fs::write(d.join("unrelated.sqlite"), b"").unwrap();
        let p = newest_matching(&d, "state_", ".sqlite").unwrap();
        assert!(p.file_name().unwrap().to_string_lossy() == "state_10.sqlite");
    }

    #[test]
    fn newest_matching_ignores_sidecars_and_bad_names() {
        let d = tmp("sidecar");
        std::fs::write(d.join("state_3.sqlite-wal"), b"").unwrap();
        std::fs::write(d.join("state_x.sqlite"), b"").unwrap();
        assert!(newest_matching(&d, "state_", ".sqlite").is_none());
    }

    impl ScanOutcome {
        fn verdict_is_quiet(&self) -> bool {
            matches!(self.status, ScanStatus::NotAuthorized)
        }
    }

    // ---- P0: scan authorization ------------------------------------------
    //
    // These three are the negative tests the F3.5 report asked for. They assert
    // the decision, and the integration test in tests/f3_6_scan_auth.py asserts
    // the stronger property: zero reads actually happened.

    #[test]
    fn scan_disabled_blocks_transfer_scan() {
        // The transfer flow used to pass `scan_enabled || true`, so this exact
        // call — permission off, flow wants a scan — ran anyway.
        assert_eq!(authorize(false, true), Err(ScanDenial::Disabled));
        assert_eq!(authorize(false, false), Err(ScanDenial::Disabled));
    }

    #[test]
    fn scan_requires_explicit_consent() {
        // Permission granted is not the same as "scan now".
        assert_eq!(authorize(true, false), Err(ScanDenial::NotConfirmed));
        assert_eq!(authorize(true, true), Ok(()));
    }

    #[test]
    fn permission_revoked_blocks_next_scan() {
        // Grant, scan, revoke, scan again: the second must be refused.
        assert_eq!(authorize(true, true), Ok(()));
        assert_eq!(authorize(false, true), Err(ScanDenial::Disabled));
    }

    /// `SCAN_EXECUTIONS` is process-wide, so tests that read or reset it must not
    /// run concurrently with tests that perform scans.
    static COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn a_denied_scan_never_counts_as_an_execution() {
        let _guard = COUNTER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_executions_for_test();
        // A scan with permission off must not increment the read counter.
        let dir = tmp("no-reads");
        let outcome = run(&dir, false);
        assert!(outcome.verdict_is_quiet());
        assert_eq!(executions(), 0, "a refused scan must not read anything");
    }

    #[test]
    fn read_errors_classify_into_the_right_status() {
        // AccessDenied: the OS refused us.
        for m in [
            "Operation not permitted (os error 1)",
            "permission denied",
            "SQLITE_CANTOPEN: unable to open database file",
            "not authorized",
            // the two sentences the codex adapter actually produces
            "local database could not be opened read-only: cannot open database",
        ] {
            assert_eq!(classify_read_error(m), ScanStatus::AccessDenied, "{m}");
        }
        // Unreadable: the source exists but is not usable data.
        for m in [
            "file is not a database",
            "database disk image is malformed",
            "no such column: item_type",
            "unexpected end of file",
            "local database could not be opened read-only: database cannot be read",
        ] {
            assert_eq!(classify_read_error(m), ScanStatus::Unreadable, "{m}");
        }
    }

    #[test]
    fn a_read_failure_is_never_reported_as_client_not_found() {
        for m in ["Operation not permitted", "file is not a database"] {
            assert_ne!(classify_read_error(m), ScanStatus::ClientNotFound, "{m}");
        }
    }

    #[test]
    fn denial_messages_are_actionable() {
        // The UI shows these verbatim; they must say what happened and what now.
        assert!(ScanDenial::Disabled.message().contains("关闭"));
        assert!(ScanDenial::NotConfirmed.message().contains("确认"));
    }
}
