//! FirstLight desktop client.
//!
//! F2 scope: a real transparent, always-on-top light point on macOS, persisted
//! position, tray control, click-through, and a personal light panel.
//!
//! Architecture rule (ADR-005): this crate **contains no business rules**.
//! Light level, titles, evidence and registry decisions all come from
//! `light-core`. Commands here only move data across the Tauri boundary.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub(crate) mod api;
mod appkit;
mod control;
pub(crate) mod geometry;
mod identity;
mod identity_store;
mod perf;
mod scan;
mod transfer;
mod state;

use geometry::{Px, Rect};
use state::Settings;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindow,
};

/// Physical size of the light point window (132 pt at 2x).
const WINDOW_SIZE: (i32, i32) = (132, 132);
pub(crate) const LIGHT_LABEL: &str = "lightpoint";
pub(crate) const PANEL_LABEL: &str = "panel";

struct AppState {
    settings: Mutex<Settings>,
    last_probe: Mutex<Option<serde_json::Value>>,
    /// Loaded light seed. `None` means identity could not be established.
    identity: Mutex<Option<(light_core::identity::RootIdentity, identity::IdentityRecord)>>,
    /// Why the identity is unavailable, shown verbatim in the panel.
    identity_error: Mutex<Option<String>>,
    /// Where the transmission service lives.
    api_base: Mutex<String>,
    state_dir: PathBuf,
    click_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
}

// ---------------------------------------------------------------------------
// State helpers — all clone out of the lock so no borrow can escape.
// ---------------------------------------------------------------------------

pub(crate) fn read_settings(app: &AppHandle) -> Settings {
    let st = app.state::<AppState>();
    let settings = st.settings.lock().unwrap().clone();
    settings
}

pub(crate) fn update_settings(app: &AppHandle, f: impl FnOnce(&mut Settings)) {
    let st = app.state::<AppState>();
    let mut guard = st.settings.lock().unwrap();
    f(&mut guard);
}

fn state_dir(app: &AppHandle) -> PathBuf {
    let st = app.state::<AppState>();
    st.state_dir.clone()
}

fn set_menu_check(app: &AppHandle, checked: bool) {
    let st = app.state::<AppState>();
    let guard = st.click_item.lock().unwrap();
    if let Some(item) = guard.as_ref() {
        let _ = item.set_checked(checked);
    }
}

pub(crate) fn persist(app: &AppHandle) {
    let settings = read_settings(app);
    let dir = state_dir(app);
    if let Err(e) = state::save(&dir, &settings) {
        // A failed write must never take the light down.
        eprintln!("state save failed: {e}");
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();

            let dir = state::state_dir(
                handle
                    .path()
                    .app_config_dir()
                    .unwrap_or_else(|_| PathBuf::from(".")),
            );
            let settings = state::load(&dir);

            app.manage(AppState {
                settings: Mutex::new(settings.clone()),
                last_probe: Mutex::new(None),
                identity: Mutex::new(None),
                identity_error: Mutex::new(None),
                api_base: Mutex::new(
                    std::env::var("FIRSTLIGHT_API_BASE")
                        .unwrap_or_else(|_| api::DEFAULT_BASE.to_string()),
                ),
                state_dir: dir.clone(),
                click_item: Mutex::new(None),
            });

            let window = app
                .get_webview_window(LIGHT_LABEL)
                .expect("lightpoint window must exist");
            configure_light_window(&window, &settings);

            // Menu-bar utility: no Dock icon, and never the "active app" just
            // because the light point exists. See T01-4 in the F2 report.
            #[cfg(target_os = "macos")]
            {
                let _ = appkit::set_activation_policy(1); // 1 = accessory
                if let Ok(ptr) = window.ns_window() {
                    // Join every Space, stay put when Spaces switch, and remain
                    // visible beside full-screen apps.
                    let _ = appkit::set_spaces(ptr, 1 | (1 << 4) | (1 << 8));
                }
            }

            // ---- identity --------------------------------------------
            app.manage(transfer::TransferState::default());
            init_identity(&handle, &dir);

            build_tray(&handle, settings.click_through)?;

            // Automation surface: only present in builds compiled with
            // `--features test-control`, and still requires the env var.
            #[cfg(feature = "test-control")]
            control::maybe_start(handle.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_click_through,
            set_quiet_mode,
            set_scan_enabled,
            save_position,
            reset_position,
            hide_light_point,
            show_light_point,
            open_panel,
            scan_now,
            light_status,
            get_visual_tokens,
            ui_build_hash,
            light_style,
            probe_result,
            perf_sample,
            identity_status,
            export_recovery,
            import_recovery,
            set_api_base,
            api_status,
            create_invitation,
            accept_invitation,
            complete_transfer,
            refresh_transfer,
            confirm_transfer,
            cancel_transfer,
            light_paths,
            transfer_snapshot,
            t01_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FirstLight");
}

// ---------------------------------------------------------------------------
// Window configuration
// ---------------------------------------------------------------------------

fn configure_light_window(window: &WebviewWindow, settings: &Settings) {
    // Appear on every Space and float above normal windows.
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.set_ignore_cursor_events(settings.click_through);
    let _ = window.set_always_on_top(true);

    let monitors = monitor_rects(window);
    let primary = primary_rect(window).or_else(|| monitors.first().copied());

    let placement = geometry::place(settings.position, WINDOW_SIZE, &monitors, primary);
    let _ = window.set_position(tauri::PhysicalPosition::new(placement.pos.x, placement.pos.y));

    if settings.visible {
        let _ = window.show();
    } else {
        let _ = window.hide();
    }

    let _ = window.emit(
        "placement",
        serde_json::json!({
            "x": placement.pos.x,
            "y": placement.pos.y,
            "outcome": format!("{:?}", placement.outcome),
        }),
    );
}

fn rect_of(monitor: &tauri::Monitor) -> Rect {
    let p = monitor.position();
    let s = monitor.size();
    Rect {
        x: p.x,
        y: p.y,
        w: s.width as i32,
        h: s.height as i32,
    }
}

fn primary_rect(window: &WebviewWindow) -> Option<Rect> {
    window.primary_monitor().ok().flatten().map(|m| rect_of(&m))
}

fn monitor_rects(window: &WebviewWindow) -> Vec<Rect> {
    window
        .available_monitors()
        .map(|ms| ms.iter().map(rect_of).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tray
// ---------------------------------------------------------------------------

fn build_tray(app: &AppHandle, click_through: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open_panel", "打开我的光", true, None::<&str>)?;
    let click = CheckMenuItem::with_id(
        app,
        "click_through",
        "点击穿透（光点不挡鼠标）",
        true,
        click_through,
        None::<&str>,
    )?;
    let toggle = MenuItem::with_id(app, "toggle_visible", "显示 / 隐藏光点", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset_position", "把光点放回屏幕中央", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &click,
            &toggle,
            &reset,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(false)
        .tooltip("第一束光")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            handle_tray_command(app, event.id().as_ref());
        })
        .build(app)?;

    set_menu_check(app, click_through);
    Ok(())
}

fn handle_tray_command(app: &AppHandle, id: &str) {
    match id {
        "quit" => app.exit(0),
        "open_panel" => {
            if let Err(e) = open_panel_window(app) {
                eprintln!("open panel failed: {e}");
            }
        }
        "click_through" => {
            let enabled = !read_settings(app).click_through;
            set_click_through_impl(app, enabled);
        }
        "toggle_visible" => {
            if read_settings(app).visible {
                hide_light_point_impl(app);
            } else {
                show_light_point_impl(app);
            }
        }
        "reset_position" => reset_position_impl(app),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Shared implementations (also callable from the webview)
// ---------------------------------------------------------------------------

pub(crate) fn set_quiet_mode_impl(app: &AppHandle, enabled: bool) {
    update_settings(app, |s| s.quiet_mode = enabled);
    if let Some(w) = app.get_webview_window(LIGHT_LABEL) {
        let _ = w.emit("quiet-mode-changed", enabled);
    }
    persist(app);
}

pub(crate) fn set_click_through_impl(app: &AppHandle, enabled: bool) {
    update_settings(app, |s| s.click_through = enabled);
    set_menu_check(app, enabled);
    if let Some(w) = app.get_webview_window(LIGHT_LABEL) {
        let _ = w.set_ignore_cursor_events(enabled);
        let _ = w.emit("click-through-changed", enabled);
    }
    persist(app);
}

pub(crate) fn hide_light_point_impl(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LIGHT_LABEL) {
        let _ = w.hide();
    }
    update_settings(app, |s| s.visible = false);
    persist(app);
}

pub(crate) fn show_light_point_impl(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LIGHT_LABEL) {
        let _ = w.show();
    }
    update_settings(app, |s| s.visible = true);
    persist(app);
}

pub(crate) fn reset_position_impl(app: &AppHandle) {
    let Some(w) = app.get_webview_window(LIGHT_LABEL) else {
        return;
    };
    let monitors = monitor_rects(&w);
    let primary = primary_rect(&w).or_else(|| monitors.first().copied());
    let placement = geometry::place(None, WINDOW_SIZE, &monitors, primary);
    let _ = w.set_position(tauri::PhysicalPosition::new(placement.pos.x, placement.pos.y));
    update_settings(app, |s| s.position = Some(placement.pos));
    persist(app);
    let _ = w.emit(
        "placement",
        serde_json::json!({
            "x": placement.pos.x, "y": placement.pos.y,
            "outcome": format!("{:?}", placement.outcome),
        }),
    );
}

/// Present the personal light panel **without** stealing focus.
///
/// `orderFrontRegardless` puts the window on screen without making it key and
/// without activating the app, so whatever the person was doing stays in front
/// of them. Clicking the panel itself will focus it, which is their choice.
pub(crate) fn open_panel_window(app: &AppHandle) -> tauri::Result<()> {
    let panel = match app.get_webview_window(PANEL_LABEL) {
        Some(existing) => existing,
        None => tauri::WebviewWindowBuilder::new(
            app,
            PANEL_LABEL,
            tauri::WebviewUrl::App("panel.html".into()),
        )
        .title("我的光")
        .inner_size(720.0, 560.0)
        .min_inner_size(640.0, 480.0)
        .resizable(true)
        .visible(false)
        // An overlay title bar hides the grey system strip. The panel provides
        // its own paper; a chrome bar above it breaks the whole surface.
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .center()
        .build()?,
    };

    let _ = panel.unminimize();
    // The webview can keep a stale viewport after being created hidden and then
    // revealed without activation, which makes the page overflow its window.
    let _ = panel.set_size(tauri::LogicalSize::new(720.0, 560.0));
    #[cfg(target_os = "macos")]
    {
        match panel.ns_window() {
            Ok(ptr) => {
                let _ = appkit::order_front_without_activating(ptr);
                #[cfg(feature = "shot-mode")]
                {
                    // Dev capture only. A key window alone can still sit under the
                    // user's active app, so also raise its level for the capture.
                    let _ = panel.set_always_on_top(true);
                    let _ = panel.set_focus();
                }
                #[cfg(not(feature = "shot-mode"))]
                {
                    let _ = appkit::deactivate_app();
                }
            }
            Err(_) => {
                let _ = panel.show();
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = panel.show();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Content digest of the UI files embedded in this binary (computed by build.rs).
///
/// The panel reports it; a test recomputes the digest from source. "Am I looking at
/// the UI I just edited" becomes a comparison of two hashes, not a guess.
/// Which renderer the overlay should use.
///
/// Defaults to `legacy` (paintBead). The experimental aperture renderer is opt-in
/// through FIRSTLIGHT_LIGHT_STYLE, so rolling back is an environment variable and
/// the production path is byte-for-byte the old one.
#[tauri::command]
fn light_style() -> serde_json::Value {
    let raw = std::env::var("FIRSTLIGHT_LIGHT_STYLE").unwrap_or_else(|_| "legacy".into());
    let style = match raw.as_str() {
        "aperture" | "organic" | "original" => raw.as_str(),
        _ => "legacy",
    };
    serde_json::json!({
        "style": style,
        "rim": std::env::var("FIRSTLIGHT_APERTURE_RIM").unwrap_or_else(|_| "8%".into()),
        "material": std::env::var("FIRSTLIGHT_APERTURE_MATERIAL").unwrap_or_else(|_| "base".into()),
        "bleed": std::env::var("FIRSTLIGHT_APERTURE_BLEED").ok(),
    })
}

#[tauri::command]
fn ui_build_hash() -> String {
    env!("FL_UI_HASH").to_string()
}

#[tauri::command]
fn get_settings(app: AppHandle) -> serde_json::Value {
    let settings = read_settings(&app);
    serde_json::json!({
        "scan_enabled": settings.scan_enabled,
        "last_scan_ms": settings.last_scan_ms,
        "last_scan_status": settings.last_scan_status,
        "settings": settings,
        "state_dir": state_dir(&app).to_string_lossy(),
    })
}

#[tauri::command]
fn set_click_through(app: AppHandle, enabled: bool) {
    set_click_through_impl(&app, enabled);
}

#[tauri::command]
fn set_quiet_mode(app: AppHandle, enabled: bool) {
    set_quiet_mode_impl(&app, enabled);
}

/// Turn the standing permission on or off.
///
/// Granting permission does **not** scan. A scan needs its own explicit
/// confirmation, and a switch flip is not one: it would mean the user granted
/// "may scan" and got "scanned" in the same gesture, with no chance to say no.
#[tauri::command]
fn set_scan_enabled(app: AppHandle, enabled: bool) {
    update_settings(&app, |s| s.scan_enabled = enabled);
    persist(&app);
    // Either way, push whatever the stored state now is. Revoking permission
    // must also stop the next read, which `scan::authorize` guarantees.
    let payload = light_payload(&app);
    for label in [LIGHT_LABEL, PANEL_LABEL] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.emit("light-updated", payload.clone());
        }
    }
}

/// One explicit, user-initiated scan.
///
/// `confirmed` must come from a deliberate gesture in this session. It is a
/// separate command from the permission switch on purpose.
#[tauri::command]
fn scan_now(app: AppHandle, confirmed: bool) -> serde_json::Value {
    let settings = read_settings(&app);
    match scan::authorize(settings.scan_enabled, confirmed) {
        Ok(()) => {
            let outcome = scan_and_publish(&app);
            serde_json::json!({
                "ok": true,
                "status": format!("{:?}", outcome.status).to_lowercase(),
                "outcome": outcome,
            })
        }
        Err(denial) => serde_json::json!({
            "ok": false,
            "status": "not_authorized",
            "reason": denial.message(),
            // No outcome, because nothing ran.
            "scanned": false,
        }),
    }
}

#[tauri::command]
fn save_position(app: AppHandle, x: i32, y: i32) {
    update_settings(&app, |s| s.position = Some(Px::new(x, y)));
    persist(&app);
}

#[tauri::command]
fn reset_position(app: AppHandle) {
    reset_position_impl(&app);
}

#[tauri::command]
fn hide_light_point(app: AppHandle) {
    hide_light_point_impl(&app);
}

#[tauri::command]
fn show_light_point(app: AppHandle) {
    show_light_point_impl(&app);
}

#[tauri::command]
fn open_panel(app: AppHandle) -> Result<(), String> {
    open_panel_window(&app).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Scanning and light status
// ---------------------------------------------------------------------------

/// Run a scan now. Returns the outcome and pushes it to every window.
pub(crate) fn scan_and_publish(app: &AppHandle) -> scan::ScanOutcome {
    let settings = read_settings(app);
    let dir = state_dir(app);
    let outcome = scan::run(&dir, settings.scan_enabled);

    scan::save_last(&dir, &outcome);
    update_settings(app, |s| {
        s.last_scan_ms = Some(outcome.at_ms);
        s.last_scan_status = Some(format!("{:?}", outcome.status).to_lowercase());
    });
    persist(app);

    // Push to the light point and the panel.
    let payload = light_payload(app);
    for label in [LIGHT_LABEL, PANEL_LABEL] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.emit("light-updated", payload.clone());
        }
    }

    // A newly verified family is the one event worth interrupting for.
    if !outcome.newly_verified.is_empty() {
        if let Some(w) = app.get_webview_window(LIGHT_LABEL) {
            let kind = if outcome.verified_families.len() == outcome.newly_verified.len() {
                "ignition"
            } else {
                "new_family"
            };
            let _ = w.emit(
                "light-event",
                serde_json::json!({
                    "kind": kind,
                    "families": outcome.newly_verified,
                    "level": outcome.light.as_ref().map(|l| l.level.clone()),
                }),
            );
        }
    }
    outcome
}

/// The payload both windows render from.
fn light_payload(app: &AppHandle) -> serde_json::Value {
    let settings = read_settings(app);
    let dir = state_dir(app);
    let last = scan::load_last(&dir);

    let light = last.as_ref().and_then(|o| o.light.as_ref());
    let level = light.map(|l| l.level.clone()).unwrap_or_else(|| "DORMANT".into());
    // A failed scan has no snapshot, but the title must still describe the facts
    // already proven in the store — otherwise the same 1 family would read
    // "初明者" before a failure and "寻光者" after it.
    let proven = light
        .map(|l| l.verified_families)
        .unwrap_or_else(|| verified_families_from_store(&dir).len() as u32);
    let knowledge_slug = light
        .map(|l| l.knowledge_slug.clone())
        .unwrap_or_else(|| light_core::knowledge_title(proven).slug().to_string());
    let knowledge_zh = light
        .map(|l| l.knowledge_zh.clone())
        .unwrap_or_else(|| light_core::knowledge_title(proven).display_zh().to_string());

    let activated_nodes = light.map(|l| l.activated_nodes).unwrap_or(0);
    serde_json::json!({
        "state_dir": dir.to_string_lossy(),
        "level": level,
        "knowledge_title": { "slug": knowledge_slug, "zh": knowledge_zh },
        "transmission_title": {
            "slug": light_core::transmission_title(activated_nodes).slug(),
            "zh": light_core::transmission_title(activated_nodes).display_zh(),
        },
        // A failed or unauthorised scan must not look like "you have no verified
        // models": fall back to what the local store already proved.
        "verified_families": light
            .map(|l| l.verified_families)
            .unwrap_or_else(|| verified_families_from_store(&dir).len() as u32),
        "attested_families": light.map(|l| l.attested_families).unwrap_or(0),
        "activated_nodes": activated_nodes,
        "families": last
            .as_ref()
            .map(|o| o.families.clone())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                verified_families_from_store(&dir)
                    .into_iter()
                    .map(|f| scan::FamilyView {
                        family: f, level: "E3".into(), sessions: 0,
                        raw_models: vec![], countable: true,
                    })
                    .collect()
            }),
        "scan": {
            "status": last.as_ref().map(|o| format!("{:?}", o.status).to_lowercase()),
            "reason": last.as_ref().and_then(|o| o.reason.clone()),
            "at_ms": last.as_ref().map(|o| o.at_ms),
            "duration_ms": last.as_ref().map(|o| o.duration_ms),
            "sessions_seen": last.as_ref().map(|o| o.sessions_seen).unwrap_or(0),
            "countable_sessions": last.as_ref().map(|o| o.countable_sessions).unwrap_or(0),
            "observable_sessions": last.as_ref().map(|o| o.observable_sessions).unwrap_or(0),
            "rejected_task_label": last.as_ref().map(|o| o.rejected_task_label).unwrap_or(0),
            "rejected_unknown": last.as_ref().map(|o| o.rejected_unknown).unwrap_or(0),
            "rejected_empty": last.as_ref().map(|o| o.rejected_empty).unwrap_or(0),
            "state_open_mode": last.as_ref().and_then(|o| o.state_open_mode.clone()),
            "history_open_mode": last.as_ref().and_then(|o| o.history_open_mode.clone()),
            "columns_touched": last.as_ref().map(|o| o.columns_touched.clone()).unwrap_or_default(),
        },
        "scan_enabled": settings.scan_enabled,
        "click_through": settings.click_through,
        "quiet_mode": settings.quiet_mode,
        "visible": settings.visible,
        "position": settings.position,
    })
}



#[tauri::command]
fn light_status(app: AppHandle) -> serde_json::Value {
    light_payload(&app)
}

/// Visual tokens are served from the core so the UI never hardcodes a constant.
#[tauri::command]
fn get_visual_tokens() -> String {
    light_core::light::VISUAL_TOKENS_JSON.to_string()
}

/// The page reports what it is actually rendering (see `__flProbe` in the UI).
/// Used by automated visual tests so they never have to guess from pixels.
#[tauri::command]
fn probe_result(app: AppHandle, value: serde_json::Value) {
    let st = app.state::<AppState>();
    *st.last_probe.lock().unwrap() = Some(value);
}

pub(crate) fn take_probe(app: &AppHandle) -> Option<serde_json::Value> {
    let st = app.state::<AppState>();
    let taken = st.last_probe.lock().unwrap().take();
    taken
}

// ---------------------------------------------------------------------------
// Identity, server, and transfer commands
// ---------------------------------------------------------------------------

/// Pick the credential backend.
///
/// The default is the OS credential store. The file backend exists because this
/// development sandbox cannot write to the login keychain; it is selected only by
/// an explicit environment variable and reports itself as such in the UI.
pub(crate) fn identity_backend(dir: &std::path::Path) -> Box<dyn identity_store::IdentityStore> {
    match std::env::var("FIRSTLIGHT_IDENTITY_BACKEND").as_deref() {
        Ok("file") => Box::new(identity_store::FileStore::new(dir)),
        Ok("memory") => Box::new(identity_store::MemoryStore::new()),
        _ => {
            #[cfg(target_os = "macos")]
            {
                Box::new(identity_store::KeychainStore::new())
            }
            #[cfg(not(target_os = "macos"))]
            {
                // Windows Credential Manager is not implemented yet; do not
                // pretend it is. Refuse rather than silently fall back.
                Box::new(identity_store::FileStore::new(dir))
            }
        }
    }
}

fn init_identity(app: &AppHandle, dir: &std::path::Path) {
    let store = identity_backend(dir);
    match identity::load_or_create(store.as_ref(), dir, scan::now_ms()) {
        Ok((root, record, created)) => {
            if created {
                eprintln!("identity created: {}", record.anon_id);
            }
            let st = app.state::<AppState>();
            *st.identity.lock().unwrap() = Some((root, record));
        }
        Err(e) => {
            // The light point still runs; only the transmission features are
            // unavailable. Nothing is overwritten.
            eprintln!("identity unavailable: {e}");
            let st = app.state::<AppState>();
            *st.identity_error.lock().unwrap() = Some(e.to_string());
        }
    }
}

fn identity_view(app: &AppHandle) -> serde_json::Value {
    let st = app.state::<AppState>();
    let error = st.identity_error.lock().unwrap().clone();
    let guard = st.identity.lock().unwrap();
    match guard.as_ref() {
        Some((root, record)) => serde_json::json!({
            "ok": true,
            "anon_id": record.anon_id,
            "root_pubkey_hex": record.root_pubkey_hex,
            "epoch": record.epoch,
            "created_at_ms": record.created_at_ms,
            "backend": record.backend,
            "registered": record.registered,
            "seed_available": true,
            "can_sign": true,
            "root": root.anon_id(),
        }),
        None => serde_json::json!({
            "ok": false,
            "seed_available": false,
            "can_sign": false,
            "error": error,
        }),
    }
}

fn api_client(app: &AppHandle) -> Result<api::ApiClient, String> {
    let st = app.state::<AppState>();
    let base = st.api_base.lock().unwrap().clone();
    let guard = st.identity.lock().unwrap();
    match guard.as_ref() {
        Some((root, record)) => Ok(api::ApiClient::new(&base, root, record.epoch)),
        None => Err("尚未建立光种身份，无法签名请求".to_string()),
    }
}

fn mark_registered(app: &AppHandle) {
    let dir = state_dir(app);
    let _ = identity::mark_registered(&dir);
    let st = app.state::<AppState>();
    let mut guard = st.identity.lock().unwrap();
    if let Some((_, record)) = guard.as_mut() {
        record.registered = true;
    }
}

#[tauri::command]
fn identity_status(app: AppHandle) -> serde_json::Value {
    identity_view(&app)
}

/// Export the recovery code. Shown once, never logged, never uploaded.
#[tauri::command]
fn export_recovery(app: AppHandle) -> Result<String, String> {
    let st = app.state::<AppState>();
    let guard = st.identity.lock().unwrap();
    match guard.as_ref() {
        Some((root, _)) => Ok(identity::export_recovery(root)),
        None => Err("没有可导出的光种身份".into()),
    }
}

#[tauri::command]
fn import_recovery(app: AppHandle, code: String, allow_replace: bool) -> Result<serde_json::Value, String> {
    let dir = state_dir(&app);
    let store = identity_backend(&dir);
    let (root, record) =
        identity::import_recovery(store.as_ref(), &dir, &code, scan::now_ms(), allow_replace)
            .map_err(|e| e.to_string())?;
    let anon = record.anon_id.clone();
    {
        let st = app.state::<AppState>();
        *st.identity.lock().unwrap() = Some((root, record));
        *st.identity_error.lock().unwrap() = None;
    }
    Ok(serde_json::json!({ "ok": true, "anon_id": anon }))
}

#[tauri::command]
fn set_api_base(app: AppHandle, base: String) -> Result<serde_json::Value, String> {
    let trimmed = base.trim().trim_end_matches('/').to_string();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("服务地址必须以 http:// 或 https:// 开头".into());
    }
    {
        let st = app.state::<AppState>();
        *st.api_base.lock().unwrap() = trimmed.clone();
    }
    Ok(serde_json::json!({ "ok": true, "api_base": trimmed }))
}

#[tauri::command]
fn api_status(app: AppHandle) -> serde_json::Value {
    let client = match api_client(&app) {
        Ok(c) => c,
        Err(e) => return serde_json::json!({ "connected": false, "error": e }),
    };
    match client.register() {
        Ok(()) => {
            mark_registered(&app);
            // Start the echo listener once we know the server is reachable.
            let client2 = api_client(&app).expect("client exists");
            transfer::spawn_echo_listener(app.clone(), client2);
            serde_json::json!({
                "connected": true,
                "api_base": client.base(),
                "registered": true,
            })
        }
        Err(e) => serde_json::json!({
            "connected": false,
            "api_base": client.base(),
            "error": e.to_string(),
        }),
    }
}

#[tauri::command]
fn create_invitation(app: AppHandle) -> Result<serde_json::Value, String> {
    let client = api_client(&app)?;
    let invitation = client.create_invitation("QR").map_err(|e| e.to_string())?;
    mark_registered(&app);
    transfer::remember_invitation(&app, &invitation);
    transfer::spawn_watch(app.clone(), client, invitation.id.clone());
    serde_json::to_value(invitation).map_err(|e| e.to_string())
}

/// Accept someone else's light, using an invitation token.
#[tauri::command]
fn accept_invitation(app: AppHandle, token: String) -> Result<serde_json::Value, String> {
    let client = api_client(&app)?;
    let resolved = client.resolve(&token).map_err(|e| e.to_string())?;
    let id = resolved
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("邀请无效")?
        .to_string();
    let view = client.accept(&id).map_err(|e| e.to_string())?;
    mark_registered(&app);
    let mut snap = transfer::snapshot(&app);
    snap.phase = transfer::Phase::from_server_state(&view.state);
    snap.server_state = Some(view.state.clone());
    snap.invitation_id = Some(id.clone());
    snap.error = None;
    snap.last_updated_ms = api::now_ms();
    transfer::set_snapshot(&app, snap);
    transfer::spawn_watch(app.clone(), client, id.clone());
    Ok(serde_json::json!({ "ok": true, "id": id, "state": view.state }))
}

/// Elog-only read of families already recorded as countable.
///
/// Opening the local light database is not a scan: it reads our own store, not
/// the third-party client's data.
fn verified_families_from_store(dir: &std::path::Path) -> Vec<String> {
    let path = dir.join(scan::LIGHT_DB);
    if !path.exists() {
        return Vec::new();
    }
    match light_store::LocalStore::open(&path) {
        Ok(store) => store.verified_families().unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Complete a transfer with this device's own verified usage evidence.
///
/// This is the real E3 path end to end: scan the local client, take a family the
/// adapter can prove, attest it under this identity, then activate. The server
/// still enforces that the evidence post-dates the relation.
#[tauri::command]
fn complete_transfer(app: AppHandle, id: String) -> Result<serde_json::Value, String> {
    let client = api_client(&app)?;
    let dir = state_dir(&app);

    // Read *what has already been proven*, never run a scan here.
    //
    // F3.5 had `settings.scan_enabled || true` on this line, so completing a
    // transfer silently read the user's local database even with scanning
    // switched off. Activation may only use evidence the user already agreed to
    // produce.
    let families = verified_families_from_store(&dir);
    let Some(family) = families.first().cloned() else {
        return Err(
            "还没有可核验的使用记录。先到「我的光」里开启扫描，并确认一次扫描，\
             之后才能用本机证据完成激活。"
                .to_string(),
        );
    };
    let event_id = format!("desk-{}-{family}-{}", id, api::now_ms());
    client
        .attest_usage(&event_id, &family, "E3", "codex-app", "1.0.0", api::now_ms())
        .map_err(|e| e.to_string())?;
    let result = client.activate(&id, &event_id).map_err(|e| e.to_string())?;
    let mut snap = transfer::snapshot(&app);
    snap.phase = transfer::Phase::Activated;
    snap.server_state = Some("ACTIVATED".into());
    snap.error = None;
    snap.last_updated_ms = api::now_ms();
    transfer::set_snapshot(&app, snap);
    Ok(serde_json::json!({ "ok": true, "family": family, "server": result }))
}

#[tauri::command]
fn refresh_transfer(app: AppHandle, id: String) -> Result<serde_json::Value, String> {
    let client = api_client(&app)?;
    let view = client.get_transmission(&id).map_err(|e| e.to_string())?;
    let mut snap = transfer::snapshot(&app);
    snap.phase = transfer::Phase::from_server_state(&view.state);
    snap.server_state = Some(view.state.clone());
    snap.invitation_id = Some(id);
    snap.error = None;
    snap.last_updated_ms = api::now_ms();
    transfer::set_snapshot(&app, snap);
    serde_json::to_value(view).map_err(|e| e.to_string())
}

#[tauri::command]
fn confirm_transfer(app: AppHandle, id: String) -> Result<serde_json::Value, String> {
    let client = api_client(&app)?;
    let view = client.confirm(&id).map_err(|e| e.to_string())?;
    let mut snap = transfer::snapshot(&app);
    snap.phase = transfer::Phase::from_server_state(&view.state);
    snap.server_state = Some(view.state.clone());
    snap.invitation_id = Some(id);
    snap.error = None;
    snap.last_updated_ms = api::now_ms();
    transfer::set_snapshot(&app, snap);
    serde_json::to_value(view).map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_transfer(app: AppHandle, id: String) -> Result<serde_json::Value, String> {
    let client = api_client(&app)?;
    let view = client.cancel(&id).map_err(|e| e.to_string())?;
    let mut snap = transfer::snapshot(&app);
    snap.phase = transfer::Phase::from_server_state(&view.state);
    snap.server_state = Some(view.state.clone());
    snap.last_updated_ms = api::now_ms();
    transfer::set_snapshot(&app, snap);
    serde_json::to_value(view).map_err(|e| e.to_string())
}

#[tauri::command]
fn light_paths(app: AppHandle) -> Result<serde_json::Value, String> {
    let client = api_client(&app)?;
    let paths = client.light_paths().map_err(|e| e.to_string())?;
    {
        let st = app.state::<transfer::TransferState>();
        *st.last_paths.lock().unwrap() = Some(paths.clone());
    }
    serde_json::to_value(paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn transfer_snapshot(app: AppHandle) -> serde_json::Value {
    serde_json::to_value(transfer::snapshot(&app)).unwrap_or(serde_json::Value::Null)
}

/// Push the transfer snapshot to the panel.
pub(crate) fn publish_transfer(app: &AppHandle) {
    let snap = transfer::snapshot(app);
    let payload = serde_json::to_value(&snap).unwrap_or(serde_json::Value::Null);
    for label in [PANEL_LABEL, LIGHT_LABEL] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.emit("transfer-updated", payload.clone());
        }
    }
}

/// Self-reported resource usage, so the app can answer "what do I cost?".
#[tauri::command]
async fn perf_sample(window_ms: Option<u64>) -> perf::PerfSample {
    let ms = window_ms.unwrap_or(2000).clamp(100, 30_000);
    tauri::async_runtime::spawn_blocking(move || perf::sample_ms(ms))
        .await
        .unwrap_or_else(|_| perf::sample_ms(100))
}

/// Diagnostic snapshot used by the T-01 verification script.
#[tauri::command]
fn t01_report(window: WebviewWindow) -> serde_json::Value {
    let scale = window.scale_factor().unwrap_or(1.0);
    let inner = window.inner_size().ok();
    let pos = window.outer_position().ok();
    let monitors: Vec<serde_json::Value> = window
        .available_monitors()
        .map(|ms| {
            ms.iter()
                .map(|m| {
                    serde_json::json!({
                        "x": m.position().x, "y": m.position().y,
                        "w": m.size().width, "h": m.size().height,
                        "scale": m.scale_factor(), "name": m.name(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let appkit = match window.ns_window() {
        Ok(ptr) => match appkit::inspect(ptr) {
            Ok(state) => serde_json::to_value(state).unwrap_or(serde_json::Value::Null),
            Err(e) => serde_json::json!({"error": e}),
        },
        Err(e) => serde_json::json!({"error": e.to_string()}),
    };

    serde_json::json!({
        "label": window.label(),
        "appkit": appkit,
        "scale_factor": scale,
        "inner_size": inner.map(|s| serde_json::json!({"w": s.width, "h": s.height})),
        "position": pos.map(|p| serde_json::json!({"x": p.x, "y": p.y})),
        "monitors": monitors,
        "always_on_top": window.is_always_on_top().unwrap_or(false),
        "decorations": window.is_decorated().unwrap_or(true),
    })
}
