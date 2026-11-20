//! Transmission session state machine.
//!
//! Implements the transition table in `docs/05-state-machines.md`. Every guard
//! that protects "receiving light is not the same as learning" lives here, so it
//! can be tested without a server, a database, or a network.
//!
//! Key invariants:
//!
//! 1. `RECEIVED` never becomes `ACTIVATED` on its own. Activation requires
//!    verified usage evidence that postdates the relation.
//! 2. A successful relation requires **both** parties to confirm.
//! 3. Repeating an event is idempotent and never double-counts.
//! 4. Revoking a relation does **not** retract the receiver's own evidence —
//!    their light is theirs (spec §1.3 "sharing does not diminish").

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionState {
    Created,
    Offered,
    Received,
    Accepted,
    Learning,
    Activated,
    Acknowledged,
    Cancelled,
    Expired,
    Rejected,
    Revoked,
}

impl SessionState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Cancelled | Self::Expired | Self::Rejected | Self::Revoked
        )
    }

    #[allow(clippy::match_like_matches_macro)]
    pub fn can_expire(self) -> bool {
        matches!(self, Self::Created | Self::Offered | Self::Received)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionEvent {
    Offer,
    Open,
    Accept,
    Confirm,
    Reject,
    Expire,
    Cancel,
    Activate,
    Acknowledge,
    Revoke,
}

/// What the caller must do as a result of a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// The one-time token can no longer be used.
    TokenInvalidated,
    /// Persist a light edge in state FORMED.
    EdgeFormed,
    /// Persist a light edge in state ACTIVATED.
    EdgeActivated,
    /// Queue exactly one echo for the helper.
    EchoQueued,
    /// Mark the edge REVOKED.
    EdgeRevoked,
}

/// Inputs the machine needs from the outside world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub now_ms: i64,
    pub expires_at_ms: i64,
    /// Whether the caller has independently verified a countable usage event
    /// that postdates the relation. The machine cannot check this itself.
    pub activation_evidence_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub state: SessionState,
    pub effects: Vec<Effect>,
    /// True when the event was a replay of an already-applied event.
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FsmError {
    #[error("session expired before {event:?}")]
    Expired { event: SessionEvent },
    #[error("cannot apply {event:?} while in {from:?}")]
    IllegalTransition { from: SessionState, event: SessionEvent },
    #[error("activation rejected: no verified usage evidence postdating the relation")]
    ActivationEvidenceMissing,
}

/// A transmission session. Field-for-field compatible with the server row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Session {
    pub state: SessionState,
    pub initiator_confirmed: bool,
    pub receiver_confirmed: bool,
    pub version: u32,
    pub expires_at_ms: i64,
}

impl Session {
    pub fn new(expires_at_ms: i64) -> Self {
        Self {
            state: SessionState::Created,
            initiator_confirmed: false,
            receiver_confirmed: false,
            version: 1,
            expires_at_ms,
        }
    }

    /// Apply one event. Pure: no clock, no I/O — callers supply [`Context`].
    pub fn apply(&mut self, event: SessionEvent, ctx: &Context) -> Result<Transition, FsmError> {
        // 1. Terminal states accept nothing further.
        if self.state.is_terminal() {
            return Err(FsmError::IllegalTransition {
                from: self.state,
                event,
            });
        }

        // 2. Expiry guard for events that require a live session.
        let requires_live = matches!(
            event,
            SessionEvent::Offer
                | SessionEvent::Open
                | SessionEvent::Accept
                | SessionEvent::Confirm
                | SessionEvent::Reject
        );
        if requires_live && ctx.now_ms > self.expires_at_ms {
            return Err(FsmError::Expired { event });
        }

        match event {
            SessionEvent::Offer => {
                self.expect(SessionState::Created, event)?;
                self.state = SessionState::Offered;
                Ok(self.commit(vec![]))
            }

            SessionEvent::Open => {
                self.expect(SessionState::Offered, event)?;
                self.state = SessionState::Received;
                Ok(self.commit(vec![]))
            }

            SessionEvent::Accept => {
                if self.state != SessionState::Received {
                    // Replay after the relation already exists.
                    if matches!(
                        self.state,
                        SessionState::Accepted
                            | SessionState::Learning
                            | SessionState::Activated
                            | SessionState::Acknowledged
                    ) {
                        return Ok(Transition {
                            state: self.state,
                            effects: vec![],
                            replayed: true,
                        });
                    }
                    return Err(FsmError::IllegalTransition {
                        from: self.state,
                        event,
                    });
                }
                self.receiver_confirmed = true;
                self.settle_confirmation()
            }

            SessionEvent::Confirm => {
                if self.state != SessionState::Received {
                    if matches!(
                        self.state,
                        SessionState::Accepted
                            | SessionState::Learning
                            | SessionState::Activated
                            | SessionState::Acknowledged
                    ) {
                        return Ok(Transition {
                            state: self.state,
                            effects: vec![],
                            replayed: true,
                        });
                    }
                    return Err(FsmError::IllegalTransition {
                        from: self.state,
                        event,
                    });
                }
                self.initiator_confirmed = true;
                self.settle_confirmation()
            }

            SessionEvent::Reject => {
                self.expect(SessionState::Received, event)?;
                self.state = SessionState::Rejected;
                Ok(self.commit(vec![Effect::TokenInvalidated]))
            }

            SessionEvent::Cancel => {
                if !matches!(self.state, SessionState::Created | SessionState::Offered) {
                    return Err(FsmError::IllegalTransition {
                        from: self.state,
                        event,
                    });
                }
                self.state = SessionState::Cancelled;
                Ok(self.commit(vec![Effect::TokenInvalidated]))
            }

            SessionEvent::Expire => {
                if !self.state.can_expire() {
                    return Err(FsmError::IllegalTransition {
                        from: self.state,
                        event,
                    });
                }
                self.state = SessionState::Expired;
                Ok(self.commit(vec![Effect::TokenInvalidated]))
            }

            SessionEvent::Activate => {
                if matches!(
                    self.state,
                    SessionState::Activated | SessionState::Acknowledged
                ) {
                    return Ok(Transition {
                        state: self.state,
                        effects: vec![],
                        replayed: true,
                    });
                }
                if self.state != SessionState::Learning {
                    return Err(FsmError::IllegalTransition {
                        from: self.state,
                        event,
                    });
                }
                if !ctx.activation_evidence_ok {
                    return Err(FsmError::ActivationEvidenceMissing);
                }
                self.state = SessionState::Activated;
                Ok(self.commit(vec![Effect::EdgeActivated]))
            }

            SessionEvent::Acknowledge => {
                if self.state == SessionState::Acknowledged {
                    return Ok(Transition {
                        state: self.state,
                        effects: vec![],
                        replayed: true,
                    });
                }
                self.expect(SessionState::Activated, event)?;
                self.state = SessionState::Acknowledged;
                Ok(self.commit(vec![Effect::EchoQueued]))
            }

            SessionEvent::Revoke => {
                if !matches!(
                    self.state,
                    SessionState::Accepted
                        | SessionState::Learning
                        | SessionState::Activated
                        | SessionState::Acknowledged
                ) {
                    return Err(FsmError::IllegalTransition {
                        from: self.state,
                        event,
                    });
                }
                self.state = SessionState::Revoked;
                Ok(self.commit(vec![Effect::EdgeRevoked]))
            }
        }
    }

    /// Both confirmations are required before a relation exists.
    fn settle_confirmation(&mut self) -> Result<Transition, FsmError> {
        if self.receiver_confirmed && self.initiator_confirmed {
            self.state = SessionState::Accepted;
            let t = self.commit(vec![Effect::EdgeFormed, Effect::TokenInvalidated]);
            // The edge exists, so we are now waiting for verified usage.
            self.state = SessionState::Learning;
            let mut t = t;
            t.state = self.state;
            Ok(t)
        } else {
            // Still RECEIVED; a single confirmation is not a relation.
            Ok(self.commit(vec![]))
        }
    }

    fn expect(&self, expected: SessionState, event: SessionEvent) -> Result<(), FsmError> {
        if self.state != expected {
            return Err(FsmError::IllegalTransition {
                from: self.state,
                event,
            });
        }
        Ok(())
    }

    fn commit(&mut self, effects: Vec<Effect>) -> Transition {
        self.version += 1;
        Transition {
            state: self.state,
            effects,
            replayed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_000_000;
    const EXP: i64 = T0 + 600_000;

    fn ctx(now: i64, evidence: bool) -> Context {
        Context {
            now_ms: now,
            expires_at_ms: EXP,
            activation_evidence_ok: evidence,
        }
    }

    /// Drive a session to LEARNING via the happy path.
    fn to_learning() -> Session {
        let mut s = Session::new(EXP);
        s.apply(SessionEvent::Offer, &ctx(T0, false)).unwrap();
        s.apply(SessionEvent::Open, &ctx(T0 + 1, false)).unwrap();
        s.apply(SessionEvent::Accept, &ctx(T0 + 2, false)).unwrap();
        s.apply(SessionEvent::Confirm, &ctx(T0 + 3, false)).unwrap();
        assert_eq!(s.state, SessionState::Learning);
        s
    }

    #[test]
    fn happy_path_reaches_acknowledged() {
        let mut s = to_learning();
        let t = s.apply(SessionEvent::Activate, &ctx(T0 + 10, true)).unwrap();
        assert_eq!(t.state, SessionState::Activated);
        assert!(t.effects.contains(&Effect::EdgeActivated));

        let t = s.apply(SessionEvent::Acknowledge, &ctx(T0 + 11, true)).unwrap();
        assert_eq!(t.state, SessionState::Acknowledged);
        assert!(t.effects.contains(&Effect::EchoQueued));
    }

    #[test]
    fn single_confirmation_does_not_form_a_relation() {
        let mut s = Session::new(EXP);
        s.apply(SessionEvent::Offer, &ctx(T0, false)).unwrap();
        s.apply(SessionEvent::Open, &ctx(T0 + 1, false)).unwrap();
        let t = s.apply(SessionEvent::Accept, &ctx(T0 + 2, false)).unwrap();
        assert_eq!(t.state, SessionState::Received, "receiver alone is not a relation");
        assert!(t.effects.is_empty());
        assert!(s.state == SessionState::Received);
    }

    #[test]
    fn both_confirmations_form_the_edge_once() {
        let mut seen_edge_formed = 0;
        let mut s = Session::new(EXP);
        s.apply(SessionEvent::Offer, &ctx(T0, false)).unwrap();
        s.apply(SessionEvent::Open, &ctx(T0 + 1, false)).unwrap();
        seen_edge_formed += s
            .apply(SessionEvent::Confirm, &ctx(T0 + 2, false))
            .unwrap()
            .effects
            .iter()
            .filter(|e| **e == Effect::EdgeFormed)
            .count();
        seen_edge_formed += s
            .apply(SessionEvent::Accept, &ctx(T0 + 3, false))
            .unwrap()
            .effects
            .iter()
            .filter(|e| **e == Effect::EdgeFormed)
            .count();
        assert_eq!(seen_edge_formed, 1, "edge must be formed exactly once");
        assert_eq!(s.state, SessionState::Learning);
    }

    #[test]
    fn received_alone_can_never_activate() {
        // A07: without both confirmations there is no relation to activate.
        let mut s = Session::new(EXP);
        s.apply(SessionEvent::Offer, &ctx(T0, false)).unwrap();
        s.apply(SessionEvent::Open, &ctx(T0 + 1, false)).unwrap();
        assert_eq!(
            s.apply(SessionEvent::Activate, &ctx(T0 + 2, true)),
            Err(FsmError::IllegalTransition {
                from: SessionState::Received,
                event: SessionEvent::Activate
            })
        );
    }

    #[test]
    fn activation_without_evidence_is_refused() {
        // A10 guard: receiving light is not learning.
        let mut s = to_learning();
        assert_eq!(
            s.apply(SessionEvent::Activate, &ctx(T0 + 5, false)),
            Err(FsmError::ActivationEvidenceMissing)
        );
        assert_eq!(s.state, SessionState::Learning);
    }

    #[test]
    fn repeated_activation_is_idempotent_and_not_double_counted() {
        let mut s = to_learning();
        s.apply(SessionEvent::Activate, &ctx(T0 + 5, true)).unwrap();
        let again = s.apply(SessionEvent::Activate, &ctx(T0 + 6, true)).unwrap();
        assert!(again.replayed);
        assert!(again.effects.is_empty(), "no second EdgeActivated effect");
        assert_eq!(s.state, SessionState::Activated);
    }

    #[test]
    fn repeated_acknowledge_queues_only_one_echo() {
        let mut s = to_learning();
        s.apply(SessionEvent::Activate, &ctx(T0 + 5, true)).unwrap();
        s.apply(SessionEvent::Acknowledge, &ctx(T0 + 6, true)).unwrap();
        let again = s.apply(SessionEvent::Acknowledge, &ctx(T0 + 7, true)).unwrap();
        assert!(again.replayed);
        assert!(again.effects.is_empty());
    }

    #[test]
    fn rejection_forms_no_relation_and_emits_no_success() {
        let mut s = Session::new(EXP);
        s.apply(SessionEvent::Offer, &ctx(T0, false)).unwrap();
        s.apply(SessionEvent::Open, &ctx(T0 + 1, false)).unwrap();
        let t = s.apply(SessionEvent::Reject, &ctx(T0 + 2, false)).unwrap();
        assert_eq!(t.state, SessionState::Rejected);
        assert!(!t.effects.contains(&Effect::EdgeFormed));
        assert!(!t.effects.contains(&Effect::EdgeActivated));
    }

    #[test]
    fn expiry_blocks_late_events() {
        let mut s = Session::new(EXP);
        s.apply(SessionEvent::Offer, &ctx(T0, false)).unwrap();
        assert_eq!(
            s.apply(SessionEvent::Open, &ctx(EXP + 1, false)),
            Err(FsmError::Expired { event: SessionEvent::Open })
        );
        let t = s.apply(SessionEvent::Expire, &ctx(EXP + 2, false)).unwrap();
        assert_eq!(t.state, SessionState::Expired);
    }

    #[test]
    fn cancel_only_before_receipt() {
        let mut s = Session::new(EXP);
        s.apply(SessionEvent::Offer, &ctx(T0, false)).unwrap();
        s.apply(SessionEvent::Open, &ctx(T0 + 1, false)).unwrap();
        assert!(matches!(
            s.apply(SessionEvent::Cancel, &ctx(T0 + 2, false)),
            Err(FsmError::IllegalTransition { .. })
        ));
    }

    #[test]
    fn revoke_after_activation_keeps_the_relational_history_but_ends_it() {
        let mut s = to_learning();
        s.apply(SessionEvent::Activate, &ctx(T0 + 5, true)).unwrap();
        let t = s.apply(SessionEvent::Revoke, &ctx(T0 + 6, true)).unwrap();
        assert_eq!(t.state, SessionState::Revoked);
        assert!(t.effects.contains(&Effect::EdgeRevoked));
        // No effect retracts the receiver's own usage evidence.
        assert!(!t.effects.contains(&Effect::EdgeActivated));
    }

    #[test]
    fn terminal_states_accept_nothing() {
        for terminal in [
            SessionState::Cancelled,
            SessionState::Expired,
            SessionState::Rejected,
            SessionState::Revoked,
        ] {
            let mut s = Session::new(EXP);
            s.state = terminal;
            assert!(
                s.apply(SessionEvent::Activate, &ctx(T0, true)).is_err(),
                "{terminal:?} must reject events"
            );
            assert!(s.state.is_terminal());
        }
    }

    #[test]
    fn version_increments_only_on_real_transitions() {
        let mut s = to_learning();
        let v = s.version;
        s.apply(SessionEvent::Activate, &ctx(T0 + 5, true)).unwrap();
        assert_eq!(s.version, v + 1);
        let v = s.version;
        s.apply(SessionEvent::Activate, &ctx(T0 + 6, true)).unwrap(); // replay
        assert_eq!(s.version, v, "replays must not bump the version");
    }
}
