//! Persisted desktop settings.
//!
//! Written as a single JSON file. Loading is deliberately forgiving: a corrupt
//! or older file must never prevent the light point from appearing.

use crate::geometry::Px;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STATE_VERSION: u32 = 1;
pub const STATE_FILE: &str = "desktop-state.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// Physical pixels. `None` means "never placed yet".
    pub position: Option<Px>,
    /// When true the light point ignores the mouse entirely.
    pub click_through: bool,
    /// No animation, minimal glow.
    pub quiet_mode: bool,
    /// Honours the system "reduce motion" preference.
    pub reduce_motion: bool,
    pub visible: bool,
    pub autostart: bool,
    /// Whether local scanning is allowed at all.
    pub scan_enabled: bool,
    /// When the last scan finished, for the "last scan" line in the panel.
    pub last_scan_ms: Option<i64>,
    /// Short status word from the last scan (`ok`, `not_authorized`, ...).
    pub last_scan_status: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            position: None,
            click_through: false,
            quiet_mode: false,
            reduce_motion: false,
            visible: true,
            autostart: false,
            scan_enabled: false,
            last_scan_ms: None,
            last_scan_status: None,
        }
    }
}

/// Directory for persisted state.
///
/// `FIRSTLIGHT_STATE_DIR` overrides it so tests can run without touching the
/// user's Library folder.
pub fn state_dir(fallback: PathBuf) -> PathBuf {
    match std::env::var_os("FIRSTLIGHT_STATE_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => fallback,
    }
}

/// Load settings, or defaults when the file is missing or unreadable.
pub fn load(dir: &Path) -> Settings {
    let path = dir.join(STATE_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(mut s) => {
            s.version = STATE_VERSION;
            s
        }
        Err(_) => Settings::default(),
    }
}

pub fn save(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(STATE_FILE);
    let text = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    // Write to a temporary file first so a crash cannot leave a truncated file.
    let tmp = dir.join(format!("{STATE_FILE}.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.pop();
        d.pop();
        d.pop();
        d.push("target");
        d.push("desktop-state-tests");
        d.push(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_file_yields_defaults() {
        let d = tmpdir("missing");
        let s = load(&d);
        assert_eq!(s, Settings::default());
        assert!(!s.click_through, "click-through must default to off");
        assert!(!s.scan_enabled, "scanning must default to off");
    }

    #[test]
    fn roundtrip_persists_position_and_flags() {
        let d = tmpdir("roundtrip");
        let mut s = Settings::default();
        s.position = Some(Px::new(1234, 567));
        s.click_through = true;
        s.quiet_mode = true;
        save(&d, &s).unwrap();

        let back = load(&d);
        assert_eq!(back.position, Some(Px::new(1234, 567)));
        assert!(back.click_through);
        assert!(back.quiet_mode);
    }

    #[test]
    fn corrupt_file_does_not_brick_the_app() {
        let d = tmpdir("corrupt");
        std::fs::write(d.join(STATE_FILE), b"{not json at all").unwrap();
        assert_eq!(load(&d), Settings::default());
    }

    #[test]
    fn partial_file_fills_in_defaults() {
        let d = tmpdir("partial");
        std::fs::write(d.join(STATE_FILE), br#"{"click_through": true}"#).unwrap();
        let s = load(&d);
        assert!(s.click_through);
        assert_eq!(s.version, STATE_VERSION);
        assert!(s.visible, "unspecified fields keep their defaults");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let d = tmpdir("unknown");
        std::fs::write(
            d.join(STATE_FILE),
            br#"{"click_through": false, "future_field": 42}"#,
        )
        .unwrap();
        assert_eq!(load(&d), Settings::default());
    }

    #[test]
    fn negative_coordinates_survive_a_roundtrip() {
        let d = tmpdir("negative");
        let mut s = Settings::default();
        s.position = Some(Px::new(-1500, -80));
        save(&d, &s).unwrap();
        assert_eq!(load(&d).position, Some(Px::new(-1500, -80)));
    }

    #[test]
    fn no_temporary_file_is_left_behind() {
        let d = tmpdir("notmp");
        save(&d, &Settings::default()).unwrap();
        assert!(!d.join(format!("{STATE_FILE}.tmp")).exists());
    }

    #[test]
    fn env_override_wins() {
        let explicit = tmpdir("env");
        // SAFETY: single-threaded test with a unique var name.
        unsafe { std::env::set_var("FIRSTLIGHT_STATE_DIR", &explicit) };
        assert_eq!(state_dir(PathBuf::from("/should/not/be/used")), explicit);
        unsafe { std::env::remove_var("FIRSTLIGHT_STATE_DIR") };
    }
}
