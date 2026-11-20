//! Automated test control channel.
//!
//! A line-delimited JSON protocol over a Unix socket, **only enabled when
//! `FIRSTLIGHT_CONTROL_SOCKET` is set**. It never starts in a normal launch.
//!
//! Why it exists: the T-01 acceptance criteria are about OS-level window
//! behaviour. Without this channel the only way to check them would be to
//! eyeball screenshots, which proves nothing. It lets the verification script
//! ask the running app directly what AppKit thinks, and drive state changes
//! deterministically.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use tauri::{AppHandle, Emitter, Manager};

/// Run a closure on the AppKit main thread and wait for its result.
///
/// AppKit is not thread-safe: reading window state from a socket thread can
/// raise an Objective-C exception, which aborts the process because Rust cannot
/// unwind foreign exceptions. So every AppKit touch goes through the main thread.
fn on_main<T, F>(app: &AppHandle, f: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .ok()?;
    rx.recv_timeout(std::time::Duration::from_secs(5)).ok()
}

pub fn maybe_start(app: AppHandle) {
    let Some(path) = std::env::var_os("FIRSTLIGHT_CONTROL_SOCKET") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let _ = std::fs::remove_file(&path);

    std::thread::spawn(move || {
        let listener = match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("control socket bind failed: {e}");
                return;
            }
        };
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let app = app.clone();
                    std::thread::spawn(move || serve(app, s));
                }
                Err(_) => break,
            }
        }
    });
}

fn serve(app: AppHandle, stream: UnixStream) {
    let reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut writer = stream;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(cmd) => dispatch(&app, &cmd),
            Err(e) => json!({"ok": false, "error": format!("bad json: {e}")}),
        };
        let mut out = response.to_string();
        out.push('\n');
        if writer.write_all(out.as_bytes()).is_err() {
            break;
        }
        let _ = writer.flush();
    }
}

fn str_field(cmd: &Value, key: &str) -> Option<String> {
    cmd.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn bool_field(cmd: &Value, key: &str) -> Option<bool> {
    cmd.get(key).and_then(|v| v.as_bool())
}

fn i32_field(cmd: &Value, key: &str) -> Option<i32> {
    cmd.get(key).and_then(|v| v.as_i64()).map(|v| v as i32)
}

fn dispatch(app: &AppHandle, cmd: &Value) -> Value {
    let name = str_field(cmd, "cmd").unwrap_or_default();
    match name.as_str() {
        "report" => {
            let app2 = app.clone();
            match on_main(app, move || report(&app2)) {
                Some(v) => json!({"ok": true, "report": v}),
                None => json!({"ok": false, "error": "main thread did not answer"}),
            }
        }

        "light" => {
            let app2 = app.clone();
            match on_main(app, move || crate::light_payload(&app2)) {
                Some(v) => json!({"ok": true, "light": v}),
                None => json!({"ok": false, "error": "main thread did not answer"}),
            }
        }
        "hide" => {
            crate::hide_light_point_impl(app);
            json!({"ok": true})
        }
        "show" => {
            crate::show_light_point_impl(app);
            json!({"ok": true})
        }
        "click_through" => {
            let enabled = bool_field(cmd, "enabled").unwrap_or(false);
            crate::set_click_through_impl(app, enabled);
            json!({"ok": true, "enabled": enabled})
        }
        "quiet" => {
            let enabled = bool_field(cmd, "enabled").unwrap_or(false);
            crate::set_quiet_mode_impl(app, enabled);
            json!({"ok": true, "enabled": enabled})
        }
        "move" => {
            let (x, y) = (i32_field(cmd, "x"), i32_field(cmd, "y"));
            match (x, y) {
                (Some(x), Some(y)) => match app.get_webview_window("lightpoint") {
                    Some(w) => {
                        let ok = w
                            .set_position(tauri::PhysicalPosition::new(x, y))
                            .is_ok();
                        crate::update_settings(app, |s| s.position = Some(crate::geometry::Px::new(x, y)));
                        crate::persist(app);
                        json!({"ok": ok, "x": x, "y": y})
                    }
                    None => json!({"ok": false, "error": "no window"}),
                },
                _ => json!({"ok": false, "error": "x and y required"}),
            }
        }
        "reset_position" => {
            crate::reset_position_impl(app);
            json!({"ok": true})
        }
        "panel" => {
            // AppKit calls must happen on the main thread (see `on_main`).
            let app2 = app.clone();
            let res = on_main(app, move || -> Result<(), String> {
                crate::open_panel_window(&app2).map_err(|e| e.to_string())
            });
            match res {
                Some(Ok(())) => json!({"ok": true}),
                Some(Err(e)) => json!({"ok": false, "error": e}),
                None => json!({"ok": false, "error": "main thread did not answer"}),
            }
        }
        "panel_quiet" => {
            let app2 = app.clone();
            let res = on_main(app, move || -> Result<(), String> {
                crate::open_panel_window(&app2).map_err(|e| e.to_string())
            });
            match res {
                Some(Ok(())) => json!({"ok": true}),
                Some(Err(e)) => json!({"ok": false, "error": e}),
                None => json!({"ok": false, "error": "main thread did not answer"}),
            }
        }
        "close_panel" => {
            let app2 = app.clone();
            let res = on_main(app, move || {
                if let Some(w) = app2.get_webview_window("panel") {
                    let _ = w.hide();
                }
            });
            match res {
                Some(()) => json!({"ok": true}),
                None => json!({"ok": false, "error": "main thread did not answer"}),
            }
        }
        "eval" => {
            let js = str_field(cmd, "js").unwrap_or_default();
            let label = str_field(cmd, "label").unwrap_or_else(|| "lightpoint".into());
            match app.get_webview_window(&label) {
                Some(w) => match w.eval(&js) {
                    Ok(()) => json!({"ok": true}),
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                },
                None => json!({"ok": false, "error": "no such window"}),
            }
        }
        "policy" => {
            let value = i32_field(cmd, "value").unwrap_or(1) as i64;
            let res = on_main(app, move || crate::appkit::set_activation_policy(value));
            match res {
                Some(Ok(())) => json!({"ok": true, "policy": value}),
                Some(Err(e)) => json!({"ok": false, "error": e}),
                None => json!({"ok": false, "error": "main thread did not answer"}),
            }
        }
        "spaces" => {
            let behaviour = cmd
                .get("behaviour")
                .and_then(|v| v.as_u64())
                .unwrap_or(1 | 16 | 256);
            let app2 = app.clone();
            let res = on_main(app, move || {
                let w = app2
                    .get_webview_window("lightpoint")
                    .ok_or_else(|| "no window".to_string())?;
                let ptr = w.ns_window().map_err(|e| e.to_string())?;
                crate::appkit::set_spaces(ptr, behaviour)
            });
            match res {
                Some(Ok(())) => json!({"ok": true, "behaviour": behaviour}),
                Some(Err(e)) => json!({"ok": false, "error": e}),
                None => json!({"ok": false, "error": "main thread did not answer"}),
            }
        }
        "scan" => {
            let enabled = bool_field(cmd, "enabled");
            if let Some(e) = enabled {
                crate::update_settings(app, |s| s.scan_enabled = e);
                crate::persist(app);
            }
            // `dry_run` flips the permission switch without scanning, so a test
            // can isolate "permission granted" from "scan confirmed".
            if bool_field(cmd, "dry_run") == Some(true) {
                return json!({
                    "ok": true,
                    "outcome": null,
                    "enabled": crate::read_settings(app).scan_enabled,
                });
            }
            // The control channel is the test surface, so a `scan` call is an
            // explicit confirmation. Product code paths must not fake one.
            let out = crate::scan_and_publish(app);
            json!({"ok": true, "outcome": out})
        }
        // Exercise the real product command, including its consent flag, so a
        // test can prove the command itself refuses without confirmation.
        "scan_now" => {
            let confirmed = bool_field(cmd, "confirmed").unwrap_or(false);
            let out = crate::scan_now(app.clone(), confirmed);
            json!({"ok": true, "result": out})
        }
        // Read counter: proves how many times a scan actually touched disk.
        // Denied scans must leave this number untouched.
        "panel_probe" => {
            let js = "window.__TAURI__.core.invoke('probe_result', { value: window.__panelProbe() })";
            let Some(w) = app.get_webview_window("panel") else {
                return json!({"ok": false, "error": "no panel window"});
            };
            if w.eval(js).is_err() {
                return json!({"ok": false, "error": "eval failed"});
            }
            for _ in 0..240 {
                std::thread::sleep(std::time::Duration::from_millis(25));
                if let Some(v) = crate::take_probe(app) {
                    if v.get("ready") == Some(&json!(true)) {
                        return json!({"ok": true, "probe": v});
                    }
                }
                let _ = w.eval(js);
            }
            json!({"ok": false, "error": "panel did not answer"})
        }
        "panel_resize" => {
            let w = i32_field(cmd, "w").unwrap_or(720) as f64;
            let h = i32_field(cmd, "h").unwrap_or(900) as f64;
            match app.get_webview_window("panel") {
                Some(win) => {
                    let _ = win.set_size(tauri::LogicalSize::new(w, h));
                    json!({"ok": true, "w": w, "h": h})
                }
                None => json!({"ok": false, "error": "no panel window"}),
            }
        }
        "panel_view" => {
            let view = str_field(cmd, "view").unwrap_or_else(|| "mine".into());
            let js = format!("show({view:?})");
            match app.get_webview_window("panel") {
                Some(w) => match w.eval(&js) {
                    Ok(()) => json!({"ok": true}),
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                },
                None => json!({"ok": false, "error": "no panel window"}),
            }
        }
        "scan_stats" => json!({
            "ok": true,
            "executions": crate::scan::executions(),
            "enabled": crate::read_settings(app).scan_enabled,
        }),
        // Attempt the transfer-evidence path; used to prove it never scans.
        "transfer_evidence" => {
            let id = str_field(cmd, "id").unwrap_or_default();
            match crate::complete_transfer(app.clone(), id) {
                Ok(v) => v,
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "pulse" => {
            // Demo trigger for the far-light pulse; real wiring arrives in F3.
            match app.get_webview_window("lightpoint") {
                Some(w) => {
                    let _ = w.emit("light-event", json!({"kind": "far_pulse"}));
                    json!({"ok": true})
                }
                None => json!({"ok": false, "error": "no lightpoint"}),
            }
        }
        "probe" => {
            let js = "window.__TAURI__.core.invoke('probe_result', { value: window.__flProbe() })";
            if let Some(w) = app.get_webview_window("lightpoint") {
                if w.eval(js).is_err() {
                    return json!({"ok": false, "error": "eval failed"});
                }
            } else {
                return json!({"ok": false, "error": "no lightpoint"});
            }
            // The page answers asynchronously, and may still be booting, so retry
            // until it reports ready rather than failing on the first attempt.
            for _ in 0..240 {
                std::thread::sleep(std::time::Duration::from_millis(25));
                if let Some(v) = crate::take_probe(app) {
                    if v.get("ready").and_then(|r| r.as_bool()).unwrap_or(false) {
                        return json!({"ok": true, "probe": v});
                    }
                    if let Some(w) = app.get_webview_window("lightpoint") {
                        let _ = w.eval(js);
                    }
                }
            }
            json!({"ok": false, "error": "probe timed out"})
        }
        "perf" => {
            let ms = cmd.get("window_ms").and_then(|v| v.as_u64()).unwrap_or(1500);
            let s = crate::perf::sample_ms(ms.clamp(100, 30_000));
            json!({"ok": true, "perf": s})
        }
        // ---- F3.5: identity, server and transfer ------------------------
        "identity" => json!({"ok": true, "identity": crate::identity_status(app.clone())}),
        "recovery_export" => match crate::export_recovery(app.clone()) {
            Ok(code) => json!({"ok": true, "code": code}),
            Err(e) => json!({"ok": false, "error": e}),
        },
        "recovery_import" => {
            let code = str_field(cmd, "code").unwrap_or_default();
            let allow = bool_field(cmd, "allowReplace").unwrap_or(false);
            match crate::import_recovery(app.clone(), code, allow) {
                Ok(v) => v,
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "api_register" => {
            let v = crate::api_status(app.clone());
            let mut out = v.clone();
            if let Some(obj) = out.as_object_mut() {
                obj.insert("ok".into(), json!(v.get("connected") == Some(&json!(true))));
            }
            out
        }
        "set_api_base" => {
            let base = str_field(cmd, "base").unwrap_or_default();
            match crate::set_api_base(app.clone(), base) {
                Ok(v) => v,
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "invite" => match crate::create_invitation(app.clone()) {
            Ok(v) => {
                let mut out = v.clone();
                if let Some(obj) = out.as_object_mut() {
                    obj.insert("ok".into(), json!(true));
                }
                out
            }
            Err(e) => json!({"ok": false, "error": e}),
        },
        "accept" => {
            let token = str_field(cmd, "token").unwrap_or_default();
            match crate::accept_invitation(app.clone(), token) {
                Ok(v) => v,
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "confirm" => {
            let id = str_field(cmd, "id").unwrap_or_default();
            match crate::confirm_transfer(app.clone(), id) {
                Ok(v) => {
                    let mut out = v.clone();
                    if let Some(obj) = out.as_object_mut() {
                        obj.insert("ok".into(), json!(true));
                    }
                    out
                }
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "cancel" => {
            let id = str_field(cmd, "id").unwrap_or_default();
            match crate::cancel_transfer(app.clone(), id) {
                Ok(v) => v,
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "complete" => {
            let id = str_field(cmd, "id").unwrap_or_default();
            match crate::complete_transfer(app.clone(), id) {
                Ok(v) => v,
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "paths" => match crate::light_paths(app.clone()) {
            Ok(v) => json!({"ok": true, "paths": v}),
            Err(e) => json!({"ok": false, "error": e}),
        },
        "transfer" => json!({"ok": true, "snapshot": crate::transfer_snapshot(app.clone())}),
        "quit" => {
            app.exit(0);
            json!({"ok": true})
        }
        other => json!({"ok": false, "error": format!("unknown command {other}")}),
    }
}

fn report(app: &AppHandle) -> Value {
    let settings = crate::read_settings(app);
    let windows: Vec<String> = app.webview_windows().keys().cloned().collect();

    let light = app.get_webview_window("lightpoint").map(|w| {
        let scale = w.scale_factor().unwrap_or(1.0);
        let pos = w.outer_position().ok();
        let size = w.inner_size().ok();
        let visible = w.is_visible().unwrap_or(false);
        let monitors: Vec<Value> = w
            .available_monitors()
            .map(|ms| {
                ms.iter()
                    .map(|m| {
                        json!({
                            "x": m.position().x, "y": m.position().y,
                            "w": m.size().width, "h": m.size().height,
                            "scale": m.scale_factor(), "name": m.name(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let appkit = match w.ns_window() {
            Ok(ptr) => match crate::appkit::inspect(ptr) {
                Ok(state) => json!(state),
                Err(e) => json!({"error": e}),
            },
            Err(e) => json!({"error": e.to_string()}),
        };

        json!({
            "scale_factor": scale,
            "position": pos.map(|p| json!({"x": p.x, "y": p.y})),
            "size": size.map(|s| json!({"w": s.width, "h": s.height})),
            "visible": visible,
            "monitors": monitors,
            "appkit": appkit,
        })
    });

    json!({
        "settings": settings,
        "windows": windows,
        "lightpoint": light,
        "panel_open": app.get_webview_window("panel").is_some(),
        "panel_geometry": app.get_webview_window("panel").map(|w| {
            let pos = w.outer_position().ok();
            let size = w.inner_size().ok();
            json!({
                "position": pos.map(|p| json!({"x": p.x, "y": p.y})),
                "size": size.map(|s| json!({"w": s.width, "h": s.height})),
            })
        }),
        "panel_visible": app
            .get_webview_window("panel")
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false),
    })
}
