//! Transfer orchestration: the bridge between the server's session state and
//! what the user sees on their light point.
//!
//! The important rule here: **animation never sets business state**. The phase
//! shown to the user is derived from the server's session state after each poll.
//! If the network drops, the phase simply stops advancing; it never jumps to
//! "delivered" because a timer expired.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use light_core::identity::RootIdentity;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::{ApiClient, Invitation, PathsView};

/// User-visible ceremony phase. Derived, never assigned by a timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Nothing in flight.
    Idle,
    /// An invitation exists and is waiting for someone to open it.
    Waiting,
    /// Someone opened it; both sides are confirming.
    Received,
    /// Both confirmed; a relation exists and we are waiting for real usage.
    Learning,
    /// The receiver proved real usage: the light is lit.
    Activated,
    /// The helper's echo has been delivered.
    Acknowledged,
    Cancelled,
    Expired,
    Rejected,
    Revoked,
    /// Local or server error; carries a message.
    Failed,
}

impl Phase {
    pub fn from_server_state(state: &str) -> Self {
        match state {
            "CREATED" | "OFFERED" => Phase::Waiting,
            "RECEIVED" => Phase::Received,
            "ACCEPTED" | "LEARNING" => Phase::Learning,
            "ACTIVATED" => Phase::Activated,
            "ACKNOWLEDGED" => Phase::Acknowledged,
            "CANCELLED" => Phase::Cancelled,
            "EXPIRED" => Phase::Expired,
            "REJECTED" => Phase::Rejected,
            "REVOKED" => Phase::Revoked,
            _ => Phase::Failed,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Phase::Activated
                | Phase::Acknowledged
                | Phase::Cancelled
                | Phase::Expired
                | Phase::Rejected
                | Phase::Revoked
                | Phase::Failed
        )
    }

    /// The light-point event this phase transition should trigger, if any.
    ///
    /// Phases only drive *presentation*. Nothing here can mark a transmission as
    /// successful; that is the server's call and it is already recorded.
    pub fn event_name(self) -> Option<&'static str> {
        match self {
            Phase::Waiting => Some("transfer_waiting"),
            Phase::Received => Some("transfer_received"),
            Phase::Learning => Some("transfer_confirmed"),
            Phase::Activated => Some("transfer_activated"),
            Phase::Acknowledged => Some("transfer_echo"),
            Phase::Cancelled | Phase::Expired | Phase::Rejected | Phase::Revoked | Phase::Failed => {
                Some("transfer_failed")
            }
            Phase::Idle => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TransferSnapshot {
    pub phase: Phase,
    pub invitation_id: Option<String>,
    pub receive_url: Option<String>,
    pub qr_url: Option<String>,
    pub expires_at_ms: Option<i64>,
    pub server_state: Option<String>,
    pub peer_label: Option<String>,
    pub error: Option<String>,
    pub last_updated_ms: i64,
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Idle
    }
}

pub struct TransferState {
    pub snapshot: Mutex<TransferSnapshot>,
    pub watching: Arc<AtomicBool>,
    pub last_paths: Mutex<Option<PathsView>>,
    pub last_event_id: Mutex<Option<i64>>,
}

impl Default for TransferState {
    fn default() -> Self {
        Self {
            snapshot: Mutex::new(TransferSnapshot::default()),
            watching: Arc::new(AtomicBool::new(false)),
            last_paths: Mutex::new(None),
            last_event_id: Mutex::new(None),
        }
    }
}

pub fn snapshot(app: &AppHandle) -> TransferSnapshot {
    let st = app.state::<TransferState>();
    let s = st.snapshot.lock().unwrap();
    let mut copy = s.clone();
    if copy.last_updated_ms == 0 {
        copy.last_updated_ms = crate::api::now_ms();
    }
    copy
}

pub fn set_snapshot(app: &AppHandle, next: TransferSnapshot) {
    let st = app.state::<TransferState>();
    let previous_phase = {
        let mut guard = st.snapshot.lock().unwrap();
        let prev = guard.phase;
        *guard = next.clone();
        prev
    };
    crate::publish_transfer(app);
    // Only fire a ceremony event when the phase actually changed.
    if previous_phase != next.phase {
        if let Some(event) = next.phase.event_name() {
            if let Some(w) = app.get_webview_window(crate::LIGHT_LABEL) {
                let _ = w.emit(
                    "light-event",
                    serde_json::json!({
                        "kind": event,
                        "phase": next.phase,
                        "ok": !matches!(next.phase, Phase::Failed | Phase::Cancelled | Phase::Expired | Phase::Rejected | Phase::Revoked),
                    }),
                );
            }
        }
    }
}

/// Poll the session until it reaches a terminal phase.
///
/// Polling (rather than trusting the client) keeps the UI honest under packet
/// loss: the phase can only move forward when the server says so.
pub fn spawn_watch(app: AppHandle, client: ApiClient, transmission_id: String) {
    let st = app.state::<TransferState>();
    if st.watching.swap(true, Ordering::SeqCst) {
        return; // one watcher at a time
    }
    let flag = st.watching.clone();

    std::thread::spawn(move || {
        let mut last_state: Option<String> = None;
        for _ in 0..600 {
            std::thread::sleep(Duration::from_millis(1500));
            match client.get_transmission(&transmission_id) {
                Ok(view) => {
                    let phase = Phase::from_server_state(&view.state);
                    if last_state.as_deref() != Some(view.state.as_str()) {
                        last_state = Some(view.state.clone());
                        let mut snap = snapshot(&app);
                        snap.phase = phase;
                        snap.server_state = Some(view.state.clone());
                        snap.invitation_id = Some(transmission_id.clone());
                        snap.error = None;
                        snap.last_updated_ms = crate::api::now_ms();
                        set_snapshot(&app, snap);
                    }
                    if phase.is_terminal() {
                        break;
                    }
                }
                Err(e) => {
                    // Keep the last known phase: do not invent progress offline.
                    let mut snap = snapshot(&app);
                    snap.error = Some(e.to_string());
                    snap.last_updated_ms = crate::api::now_ms();
                    set_snapshot(&app, snap);
                }
            }
        }
        flag.store(false, Ordering::SeqCst);
    });
}

/// Subscribe to SSE echoes in the background.
///
/// Echoes are the helper's reward: one event per activation. Event ids make a
/// reconnect idempotent — a replayed id is skipped, so the wave does not play
/// twice for the same activation.
pub fn spawn_echo_listener(app: AppHandle, client: ApiClient) {
    std::thread::spawn(move || loop {
        let last = { *app.state::<TransferState>().last_event_id.lock().unwrap() };
        let result = client.stream_events(last, |event, id, data| {
            if event == "heartbeat" || event == "hello" {
                return;
            }
            if let Some(id) = id {
                let st = app.state::<TransferState>();
                let mut guard = st.last_event_id.lock().unwrap();
                if *guard == Some(id) {
                    return; // already consumed
                }
                *guard = Some(id);
            }
            let payload = serde_json::from_str::<serde_json::Value>(data).unwrap_or_default();
            let message = payload
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("你传出的一束光，已经被点亮。")
                .to_string();
            if let Some(w) = app.get_webview_window(crate::LIGHT_LABEL) {
                let _ = w.emit(
                    "light-event",
                    serde_json::json!({ "kind": "transfer_echo", "message": message }),
                );
            }
            let mut snap = snapshot(&app);
            snap.phase = Phase::Acknowledged;
            snap.last_updated_ms = crate::api::now_ms();
            set_snapshot(&app, snap);
        });
        if result.is_err() {
            std::thread::sleep(Duration::from_secs(5));
        } else {
            std::thread::sleep(Duration::from_millis(800));
        }
    });
}

/// Build a client from the configured base URL and the loaded identity.
pub fn client_for(base: &str, identity: &RootIdentity, epoch: u32) -> ApiClient {
    ApiClient::new(base, identity, epoch)
}

pub fn remember_invitation(app: &AppHandle, invitation: &Invitation) {
    let mut snap = snapshot(app);
    snap.invitation_id = Some(invitation.id.clone());
    snap.receive_url = Some(invitation.receive_url.clone());
    snap.qr_url = Some(format!("{}/qr.svg", invitation.receive_url));
    snap.expires_at_ms = Some(invitation.expires_at_ms);
    snap.session_auto_offered();
    set_snapshot(app, snap);
}

impl TransferSnapshot {
    fn session_auto_offered(&mut self) {
        self.phase = Phase::Waiting;
        self.server_state = Some("OFFERED".into());
        self.error = None;
        self.last_updated_ms = crate::api::now_ms();
    }

    pub fn reset(&mut self) {
        *self = TransferSnapshot {
            last_updated_ms: crate::api::now_ms(),
            ..Default::default()
        };
    }
}

/// Map an outgoing edge to a short human label for the panel.
pub fn describe_edge(kind: &str, state: &str) -> (&'static str, &'static str) {
    let kind_label = match kind {
        "FIRST_LIGHT" => "首次点亮",
        "NEW_DISCOVERY" => "新发现",
        "RESONANCE" => "共鸣",
        _ => "传光",
    };
    let state_label = match state {
        "FORMED" => "等待真实使用",
        "ACTIVATED" => "已点亮",
        "REVOKED" => "已撤回",
        _ => "进行中",
    };
    (kind_label, state_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_map_from_server_states() {
        assert_eq!(Phase::from_server_state("OFFERED"), Phase::Waiting);
        assert_eq!(Phase::from_server_state("RECEIVED"), Phase::Received);
        assert_eq!(Phase::from_server_state("LEARNING"), Phase::Learning);
        assert_eq!(Phase::from_server_state("ACTIVATED"), Phase::Activated);
        assert_eq!(Phase::from_server_state("EXPIRED"), Phase::Expired);
    }

    #[test]
    fn received_is_not_activated() {
        // The one rule this whole product rests on.
        assert_ne!(Phase::from_server_state("RECEIVED"), Phase::Activated);
        assert_ne!(Phase::from_server_state("LEARNING"), Phase::Activated);
        assert!(!Phase::Received.is_terminal());
    }

    #[test]
    fn only_real_activation_maps_to_the_activation_event() {
        assert_eq!(Phase::from_server_state("ACTIVATED").event_name(), Some("transfer_activated"));
        assert_eq!(Phase::from_server_state("RECEIVED").event_name(), Some("transfer_received"));
        assert_eq!(Phase::from_server_state("LEARNING").event_name(), Some("transfer_confirmed"));
    }

    #[test]
    fn failures_have_their_own_event() {
        for state in ["EXPIRED", "CANCELLED", "REJECTED", "REVOKED"] {
            assert_eq!(Phase::from_server_state(state).event_name(), Some("transfer_failed"));
        }
    }

    #[test]
    fn terminal_phases_stop_polling() {
        for state in ["ACTIVATED", "ACKNOWLEDGED", "EXPIRED", "REVOKED"] {
            assert!(Phase::from_server_state(state).is_terminal());
        }
        for state in ["OFFERED", "RECEIVED", "LEARNING"] {
            assert!(!Phase::from_server_state(state).is_terminal());
        }
    }
}
