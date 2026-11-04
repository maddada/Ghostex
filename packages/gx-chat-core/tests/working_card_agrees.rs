//! GitHub #403: the working card above the composer and Stop versus Send must give one answer.

use ghostex_gx_chat_core::extras::working_card::working_card;
use ghostex_gx_chat_core::session::working::{is_working, presents_working};
use ghostex_gx_chat_core::ChatState;
use ghostex_gx_chat_core::WorkingStrip;
use ghostex_gx_protocol::{TurnLifecycle, TurnLifecycleState};
use serde_json::json;

const TURN_STARTED: i64 = 1_800_000_000_000;

/// The server still says working (an OpenCode TUI that never reported the turn's end), while the
/// transcript records that the lead's turn completed a minute after it began.
fn finished_turn_with_stale_working() -> ChatState {
    let mut state = ChatState::default();
    state.session.server_working = true;
    state.session.working_started_at_ms = Some(TURN_STARTED as f64);
    state.session.lifecycle = Some(TurnLifecycle {
        state: TurnLifecycleState::Completed,
        turn_id: "msg_user".into(),
        timestamp: Some(TURN_STARTED + 60_000),
    });
    state
}

fn strip(label: Option<&str>) -> WorkingStrip {
    WorkingStrip {
        label: label.map(str::to_string),
        ..Default::default()
    }
}

#[test]
fn a_completed_turn_settles_the_card_and_the_send_button_together() {
    let state = finished_turn_with_stale_working();
    assert!(!is_working(&state), "the composer shows Send");
    assert!(!presents_working(&state), "the card must not say working");
    let card = working_card(&state, &strip(None), &[]);
    assert!(!card.working);
    assert_ne!(card.phase, "working");
}

#[test]
fn a_running_turn_is_working_on_both() {
    let mut state = finished_turn_with_stale_working();
    state.session.lifecycle = Some(TurnLifecycle {
        state: TurnLifecycleState::Working,
        turn_id: "msg_user".into(),
        timestamp: Some(TURN_STARTED),
    });
    assert!(is_working(&state));
    assert!(presents_working(&state));
    assert!(working_card(&state, &strip(Some("Sprouting…")), &[]).working);
}

#[test]
fn a_subagent_still_working_keeps_the_card_working_after_the_lead_finished() {
    let mut state = finished_turn_with_stale_working();
    state.session.agent_fleet = Some(json!({"agents": [
        {"name": "explore", "status": "idle"},
        {"name": "general", "status": "working"},
    ]}));
    assert!(presents_working(&state));
    // Every child finished: the stale flag no longer holds the card.
    state.session.agent_fleet = Some(json!({"agents": [
        {"name": "explore", "status": "idle"},
        {"name": "general", "status": "idle"},
    ]}));
    assert!(!presents_working(&state));
}

#[test]
fn an_interrupt_ends_the_card_even_with_working_subagents() {
    let mut state = finished_turn_with_stale_working();
    state.session.agent_fleet = Some(json!({"agents": [{"name": "general", "status": "working"}]}));
    state.session.interrupted = true;
    assert!(!is_working(&state));
    assert!(!presents_working(&state));
}
