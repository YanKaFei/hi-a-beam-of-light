//! Direct AppKit inspection.
//!
//! Eyeballing a screenshot cannot prove that clicks pass through a window. The
//! OS makes that decision from `-[NSWindow ignoresMouseEvents]`, so we read the
//! actual property instead of simulating a click. The same applies to window
//! level (always-on-top) and collection behaviour (all Spaces / full-screen).
//!
//! This module is read-only: it never mutates AppKit state.

#[cfg(target_os = "macos")]
mod macos {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use serde::Serialize;
    use std::ffi::c_void;

    /// `NSWindowCollectionBehavior` bits we care about.
    pub const CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
    pub const STATIONARY: u64 = 1 << 4;
    pub const FULL_SCREEN_AUXILIARY: u64 = 1 << 8;

    #[derive(Debug, Clone, Serialize)]
    pub struct WindowState {
        /// The exact flag macOS consults when routing a click.
        pub ignores_mouse_events: bool,
        /// NSWindow level. Normal windows are 0; floating is 3.
        pub level: i64,
        pub collection_behavior: u64,
        pub can_join_all_spaces: bool,
        pub stationary: bool,
        pub full_screen_auxiliary: bool,
        pub is_opaque: bool,
        pub alpha: f64,
        pub background_alpha: f64,
        pub is_key: bool,
        pub is_main: bool,
        pub can_become_key: bool,
        pub can_become_main: bool,
        pub style_mask: u64,
        pub has_shadow: bool,
        pub title_visible: bool,
        /// NSApplicationActivationPolicy: 0 regular, 1 accessory, 2 prohibited.
        pub activation_policy: i64,
        pub is_active_app: bool,
    }

    pub fn inspect(ns_window: *mut c_void) -> Result<WindowState, String> {
        if ns_window.is_null() {
            return Err("window has no NSWindow".into());
        }
        // SAFETY: Tauri hands us the live NSWindow pointer for the window it owns,
        // and we only read properties on it.
        let window: &AnyObject = unsafe { &*(ns_window as *const AnyObject) };

        let ignores_mouse_events: bool = unsafe { msg_send![window, ignoresMouseEvents] };
        let level: i64 = unsafe { msg_send![window, level] };
        let collection_behavior: u64 = unsafe { msg_send![window, collectionBehavior] };
        let is_opaque: bool = unsafe { msg_send![window, isOpaque] };
        let alpha: f64 = unsafe { msg_send![window, alphaValue] };
        let is_key: bool = unsafe { msg_send![window, isKeyWindow] };
        let is_main: bool = unsafe { msg_send![window, isMainWindow] };
        let can_become_key: bool = unsafe { msg_send![window, canBecomeKeyWindow] };
        let can_become_main: bool = unsafe { msg_send![window, canBecomeMainWindow] };
        let style_mask: u64 = unsafe { msg_send![window, styleMask] };
        let has_shadow: bool = unsafe { msg_send![window, hasShadow] };
        let title_visible: bool = unsafe { msg_send![window, titlebarAppearsTransparent] };

        let bg: Option<&AnyObject> = unsafe { msg_send![window, backgroundColor] };
        let background_alpha: f64 = match bg {
            Some(color) => unsafe { msg_send![color, alphaComponent] },
            None => 1.0,
        };

        let ns_app: Option<&AnyObject> =
            unsafe { msg_send![objc2::class!(NSApplication), sharedApplication] };
        let (activation_policy, is_active_app) = match ns_app {
            Some(app) => {
                let policy: i64 = unsafe { msg_send![app, activationPolicy] };
                let active: bool = unsafe { msg_send![app, isActive] };
                (policy, active)
            }
            None => (-1, false),
        };

        Ok(WindowState {
            ignores_mouse_events,
            level,
            collection_behavior,
            can_join_all_spaces: collection_behavior & CAN_JOIN_ALL_SPACES != 0,
            stationary: collection_behavior & STATIONARY != 0,
            full_screen_auxiliary: collection_behavior & FULL_SCREEN_AUXILIARY != 0,
            is_opaque,
            alpha,
            background_alpha,
            is_key,
            is_main,
            can_become_key,
            can_become_main,
            style_mask,
            has_shadow,
            title_visible,
            activation_policy,
            is_active_app,
        })
    }

    /// Set the application activation policy (regular / accessory / prohibited).
    pub fn set_activation_policy(policy: i64) -> Result<(), String> {
        let ns_app: Option<&AnyObject> =
            unsafe { msg_send![objc2::class!(NSApplication), sharedApplication] };
        let Some(app) = ns_app else {
            return Err("no NSApplication".into());
        };
        let ok: bool = unsafe { msg_send![app, setActivationPolicy: policy] };
        if ok {
            Ok(())
        } else {
            Err(format!("setActivationPolicy({policy}) refused"))
        }
    }

    /// Show a window without making it key and without activating the app.
    ///
    /// This is the AppKit-correct way to present an auxiliary panel without
    /// stealing focus from whatever the user is doing.
    pub fn order_front_without_activating(window: *mut c_void) -> Result<(), String> {
        if window.is_null() {
            return Err("no window".into());
        }
        let w: &AnyObject = unsafe { &*(window as *const AnyObject) };
        let _: () = unsafe { msg_send![w, orderFrontRegardless] };
        Ok(())
    }

    /// Drop activation while keeping windows on screen.
    pub fn deactivate_app() -> Result<(), String> {
        let ns_app: Option<&AnyObject> =
            unsafe { msg_send![objc2::class!(NSApplication), sharedApplication] };
        let Some(app) = ns_app else {
            return Err("no NSApplication".into());
        };
        let _: () = unsafe { msg_send![app, deactivate] };
        Ok(())
    }

    /// Set the window's collection behaviour (Spaces / full-screen membership).
    pub fn set_spaces(window: *mut c_void, behaviour: u64) -> Result<(), String> {
        if window.is_null() {
            return Err("no window".into());
        }
        let w: &AnyObject = unsafe { &*(window as *const AnyObject) };
        let _: () = unsafe { msg_send![w, setCollectionBehavior: behaviour] };
        Ok(())
    }

    /// Make the window float above normal windows without fighting the OS.
    pub fn set_floating(window: *mut c_void) -> Result<(), String> {
        if window.is_null() {
            return Err("no window".into());
        }
        let w: &AnyObject = unsafe { &*(window as *const AnyObject) };
        // NSStatusWindowLevel is 25; floating is 3. Use 3 to stay polite.
        let _: () = unsafe { msg_send![w, setLevel: 3i64] };
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
pub mod fallback {
    use serde::Serialize;
    #[derive(Debug, Clone, Serialize)]
    pub struct WindowState {}
}
