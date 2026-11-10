//! A message gxserver delivered with a note under it shows once (CDXC:SessionChat 2026-11-06 in
//! `session/appended_notes.rs`). The fixture is the coordinator message the user saw twice on
//! 2026-11-06: a picture and text sent while Claude worked, which Claude took into the running
//! turn as a `queued_command` row carrying the playbook note gxserver added; the echo stayed under
//! it as a second copy with the queued echo's buttons.

use ghostex_gx_chat_core::composer::send::send_to_agent;
use ghostex_gx_chat_core::session::appended_notes::{
    with_note, PLAYBOOK_CHANGED_BY_UPDATE_NOTE, PLAYBOOK_CHANGED_NOTE,
};
use ghostex_gx_chat_core::session::before_compose;
use ghostex_gx_chat_core::session::composition::compose;
use ghostex_gx_chat_core::{ChatContext, ChatState};
use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::json;

const SENT_AT: i64 = 1_793_933_658_000;
const MESSAGE: &str = "[Image #1](/Users/madda/.local/share/ghostex/i/1793933604436.png) when i click update on one of these please show a spinner next to it and show spinner on the download button";

/// The rows as the core holds them (`ChatRow`).
fn shared(rows: &[ChatMessage]) -> Vec<ghostex_gx_chat_core::state::ChatRow> {
    rows.iter().cloned().map(std::sync::Arc::new).collect()
}

fn at(now_ms: i64) -> ChatContext {
    ChatContext {
        now_ms: now_ms as f64,
        ..Default::default()
    }
}

fn row(id: &str, role: &str, text: &str, timestamp: i64) -> ChatMessage {
    serde_json::from_value(json!({
        "id": id, "role": role, "source": "transcript", "timestamp": timestamp,
        "blocks": [{"type": "text", "text": text}],
    }))
    .expect("a transcript row")
}

/// A Claude coordinator mid-turn: the last row is its running tool call.
fn working_coordinator() -> ChatState {
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state.session.server_working = true;
    state.messages.list = shared(&[
        row("u0", "user", "check the release patches", SENT_AT - 60_000),
        row(
            "a0",
            "assistant",
            "Applying those two with fuzz.",
            SENT_AT - 5_000,
        ),
    ]);
    state.messages.list_revision += 1;
    state
}

fn user_rows(state: &ChatState) -> Vec<String> {
    compose(state, None, true)
        .iter()
        .filter(|message| matches!(message.role, ChatRole::User))
        .map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ChatBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

fn records(state: &mut ChatState, rows: Vec<ChatMessage>, now_ms: i64) {
    state.messages.list.extend(shared(&rows));
    state.messages.list_revision += 1;
    before_compose(state, &at(now_ms));
}

fn sends_while_working(state: &mut ChatState) {
    let _ = send_to_agent(state, &at(SENT_AT), MESSAGE, None, &[]);
    before_compose(state, &at(SENT_AT));
    assert_eq!(state.pending.sends.len(), 1, "the echo is drawn on Enter");
    assert!(state.pending.sends[0].sent_while_working);
    assert_eq!(user_rows(state).last().map(String::as_str), Some(MESSAGE));
}

#[test]
fn a_message_taken_mid_turn_with_the_playbook_note_shows_once() {
    let mut state = working_coordinator();
    sends_while_working(&mut state);
    let delivered = with_note(MESSAGE, PLAYBOOK_CHANGED_BY_UPDATE_NOTE);

    // The `queued_command` row, before the turn answers it.
    records(
        &mut state,
        vec![row("q1", "user", &delivered, SENT_AT + 400)],
        SENT_AT + 1_000,
    );
    assert_eq!(
        user_rows(&state)[1..],
        [delivered.clone()],
        "the recorded turn replaces the echo instead of sitting above it"
    );

    // The turn goes on and answers it: the echo is gone for good.
    records(
        &mut state,
        vec![row("a1", "assistant", "On it.", SENT_AT + 5_000)],
        SENT_AT + 6_000,
    );
    assert!(state.pending.sends.is_empty(), "the echo was pruned");
    assert_eq!(user_rows(&state)[1..], [delivered]);
}

#[test]
fn a_message_recorded_at_the_turn_end_with_a_project_playbook_note_shows_once() {
    let mut state = working_coordinator();
    sends_while_working(&mut state);
    let delivered = with_note(MESSAGE, PLAYBOOK_CHANGED_NOTE);

    records(
        &mut state,
        vec![
            row(
                "a1",
                "assistant",
                "Done with the patches.",
                SENT_AT + 20_000,
            ),
            row("u1", "user", &delivered, SENT_AT + 21_000),
            row("a2", "assistant", "On it.", SENT_AT + 25_000),
        ],
        SENT_AT + 26_000,
    );
    assert!(state.pending.sends.is_empty(), "the echo was pruned");
    assert_eq!(user_rows(&state)[1..], [delivered]);
}

#[test]
fn a_different_message_that_ends_with_the_note_does_not_take_the_echo() {
    let mut state = working_coordinator();
    sends_while_working(&mut state);
    let other = with_note("something else entirely", PLAYBOOK_CHANGED_BY_UPDATE_NOTE);

    records(
        &mut state,
        vec![row("q1", "user", &other, SENT_AT + 400)],
        SENT_AT + 1_000,
    );
    assert_eq!(state.pending.sends.len(), 1, "the echo still waits");
    assert_eq!(user_rows(&state)[1..], [other, MESSAGE.to_string()]);
}
