//! Run the Codex adapter against a real local installation and print a report.
//!
//! Usage:
//! ```text
//! cargo run -p adapter-codex --example scan_local -- <state.db> [history.db]
//! ```
//!
//! The report contains: family count, per-session verdicts keyed by an opaque
//! digest, and the column audit. It contains **no** message content, no prompt
//! text, no paths, and no token counts — see the privacy assertions in
//! `src/tests.rs`.

use adapter_codex::{scan, CodexPaths};
use light_core::EvidencePolicy;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(state_db) = args.next() else {
        eprintln!("usage: scan_local <state.db> [history.db]");
        return ExitCode::from(2);
    };
    let history_db = args.next().map(PathBuf::from);

    let paths = CodexPaths::new(PathBuf::from(state_db), history_db);
    let policy = EvidencePolicy::v1();

    match scan(&paths, &policy) {
        Ok(report) => {
            println!("adapter           : {} v{}", report.adapter_id, report.adapter_version);
            println!("registry          : {}", report.registry_semver);
            println!("sessions seen     : {}", report.counts.sessions_seen);
            println!("countable         : {}", report.counts.countable_sessions);
            println!("observable (no op): {}", report.counts.observable_sessions);
            println!("task-label rejects: {}", report.counts.rejected_task_label);
            println!("unknown-family    : {}", report.counts.rejected_unknown_family);
            println!("empty-model       : {}", report.counts.rejected_empty_model);
            println!("VERIFIED FAMILIES : {}", report.verified_families.len());
            for family in &report.verified_families {
                println!("  - {family}");
            }

            let used: Vec<&str> = report.audit.columns_used().into_iter().collect();
            println!("columns touched   : {used:?}");
            println!("refusals recorded : {}", report.audit.rejections.len());

            let json = serde_json::to_string_pretty(&report).expect("serialize");
            println!("\n--- report json ---\n{json}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("scan failed: {err}");
            ExitCode::FAILURE
        }
    }
}
