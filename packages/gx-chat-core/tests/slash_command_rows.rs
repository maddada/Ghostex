//! A slash command and what it printed, for every shape an agent records them in: the command is
//! the user's own bubble (in monospace) and its output a status note under it.
//!
//! The shapes: Claude Code's own `<command-name>` / `<local-command-stdout>` records; gxserver's
//! archived rows (`local-command:<id>`), which are how every other agent (Codex, OpenCode, Pi,
//! Hermes, Empryo, Cursor and the rest, none of which records a slash command itself) gets its
//! commands into the chat; and the live acknowledgement (`app-command-local:<id>`) before the
//! archive takes over.

use ghostex_gx_chat_core::protocol::ChatMessage;
use ghostex_gx_chat_core::session::app_commands::{app_commands_as_messages, local_command_texts};
use ghostex_gx_chat_core::session::composition::surface_skill_invocation_user_turns;
use ghostex_gx_chat_core::transcript::line_breaks::AgentLineBreaks;
use ghostex_gx_chat_core::transcript::noise::is_hidden_message;
use ghostex_gx_chat_core::transcript::presentation::project_message;
use ghostex_gx_chat_core::transcript::turns::opens_turn;
use ghostex_gx_chat_core::ChatContext;
use serde_json::{json, Value};
use std::cell::OnceCell;
use std::collections::HashMap;

fn user_row(id: &str, text: &str) -> ChatMessage {
    serde_json::from_value(json!({
        "id": id,
        "role": "user",
        "blocks": [{ "type": "text", "text": text }],
        "timestamp": 1_700_000_000_000_i64,
        "source": "transcript",
    }))
    .unwrap()
}

fn projected(message: &ChatMessage) -> Value {
    project_message(
        message,
        &OnceCell::new(),
        std::slice::from_ref(message),
        "claude",
        None,
        AgentLineBreaks::default(),
        &ChatContext::default(),
        &HashMap::new(),
        &[],
        false,
        false,
    )
}

fn surfaced(rows: &[ChatMessage]) -> Vec<Value> {
    surface_skill_invocation_user_turns(rows)
        .iter()
        .map(projected)
        .collect()
}

/// The command row as the user's bubble: plain text, monospace, a turn of its own.
fn assert_bubble(row: &Value, text: &str) {
    assert!(row["suppressed"].is_null(), "a bubble, not a marker: {row}");
    assert_eq!(row["copyText"], text);
    assert_eq!(row["slashCommand"], true);
}

#[test]
fn claude_rename_is_a_bubble_then_a_note() {
    let rows = surfaced(&[
        user_row(
            "c1",
            "<command-message>rename</command-message>\n<command-name>/rename</command-name>\n<command-args>Agent status tracking via hooks</command-args>",
        ),
        user_row(
            "c2",
            "<local-command-stdout>Session renamed to: Agent status tracking via hooks</local-command-stdout>",
        ),
    ]);
    assert_bubble(&rows[0], "/rename Agent status tracking via hooks");
    let note = &rows[1]["suppressed"];
    assert_eq!(note["kind"], "status");
    assert_eq!(note["subject"], "command");
    assert_eq!(note["tone"], "neutral");
    assert_eq!(
        note["label"],
        "Session renamed to: Agent status tracking via hooks"
    );
    assert_eq!(note["expandable"], false);
    assert_eq!(rows[1]["slashCommand"], false);
}

#[test]
fn claude_built_in_without_output_is_only_the_bubble() {
    let rows = surfaced(&[user_row("c1", "<command-name>/clear</command-name>")]);
    assert_eq!(rows.len(), 1);
    assert_bubble(&rows[0], "/clear");
}

#[test]
fn claude_plugin_skill_keeps_the_name_the_user_typed() {
    let rows = surfaced(&[user_row(
        "c1",
        "<command-message>review</command-message>\n<command-name>/toolkit:review</command-name>\n<command-args>the parser</command-args>",
    )]);
    assert_bubble(&rows[0], "/review the parser");
}

#[test]
fn multi_line_output_shows_its_first_line_and_opens_to_the_rest() {
    let rows = surfaced(&[user_row(
        "c2",
        "<local-command-stdout>\u{1b}[1mCurrent session\u{1b}[22m\n5h 12% used\n7d 40% used</local-command-stdout>",
    )]);
    let note = &rows[0]["suppressed"];
    assert_eq!(note["kind"], "status");
    assert_eq!(note["label"], "Current session");
    assert_eq!(note["expandable"], true);
    assert_eq!(note["text"], "Current session\n5h 12% used\n7d 40% used");
}

#[test]
fn stderr_output_is_an_error_note() {
    let rows = surfaced(&[user_row(
        "c2",
        "<local-command-stderr>Unknown skill: nope</local-command-stderr>",
    )]);
    assert_eq!(rows[0]["suppressed"]["tone"], "error");
    assert_eq!(rows[0]["suppressed"]["label"], "Unknown skill: nope");
}

/// Codex, OpenCode, Pi, Hermes, Empryo and Cursor record no slash command themselves: gxserver
/// archives the ones sent from chat and replays them as these escaped rows on every read.
#[test]
fn archived_command_for_any_agent_is_a_bubble_then_a_note() {
    let texts = local_command_texts("/status", Some("Model: gpt-6\nDirectory: ~/dev <main>"));
    let rows = surfaced(&[
        user_row("local-command:a1", &texts[0]),
        user_row("local-command:a1:output", &texts[1]),
    ]);
    assert_bubble(&rows[0], "/status");
    let note = &rows[1]["suppressed"];
    assert_eq!(note["kind"], "status");
    assert_eq!(note["label"], "Model: gpt-6");
    assert_eq!(note["text"], "Model: gpt-6\nDirectory: ~/dev <main>");
    assert_eq!(note["expandable"], true);
}

#[test]
fn archived_command_with_arguments_keeps_them() {
    let texts = local_command_texts("/goal ship <the> parser", None);
    assert_eq!(texts.len(), 1);
    let rows = surfaced(&[user_row("local-command:a2", &texts[0])]);
    assert_bubble(&rows[0], "/goal ship <the> parser");
}

/// The live acknowledgement of a command sent from chat, before the archive replays it.
#[test]
fn live_command_sent_from_chat_is_a_bubble_then_a_note() {
    let live = app_commands_as_messages(
        &[json!({
            "id": "k1",
            "command": "/rename Fix parser",
            "localCommand": true,
            "output": "Session renamed to: Fix parser",
            "sentAt": "2026-11-02T10:00:00.000Z",
        })],
        &[],
    );
    let rows = surfaced(&live);
    assert_eq!(rows.len(), 2);
    assert_bubble(&rows[0], "/rename Fix parser");
    assert_eq!(rows[1]["suppressed"]["kind"], "status");
    assert_eq!(
        rows[1]["suppressed"]["label"],
        "Session renamed to: Fix parser"
    );
}

/// The model, effort and Fast mode controls type their commands; the result row is the record.
#[test]
fn model_command_stays_hidden_behind_its_result() {
    let rows = surface_skill_invocation_user_turns(&[
        user_row(
            "c1",
            "<command-name>/model</command-name>\n<command-args>sonnet</command-args>",
        ),
        user_row(
            "c2",
            "<local-command-stdout>Set model to Sonnet 5.5 for this session only</local-command-stdout>",
        ),
    ]);
    let projected: Vec<Value> = rows.iter().map(projected).collect();
    assert!(is_hidden_message(&rows[0]));
    assert_eq!(
        projected[1]["suppressed"]["label"],
        "Set model to Sonnet 5.5"
    );
    assert_eq!(projected[1]["suppressed"]["subject"], "setting");
}

/// A slash command opens its own turn, so it keeps its scroll mark; its output opens none.
#[test]
fn the_command_opens_a_turn_and_its_output_does_not() {
    let rows = surface_skill_invocation_user_turns(&[
        user_row(
            "c1",
            "<command-name>/rename</command-name>\n<command-args>X</command-args>",
        ),
        user_row(
            "c2",
            "<local-command-stdout>Session renamed to: X</local-command-stdout>",
        ),
    ]);
    assert!(opens_turn(&rows[0]));
    assert!(!opens_turn(&rows[1]));
}

#[test]
fn a_path_or_a_sentence_is_not_a_slash_command() {
    for text in [
        "/Users/me/notes.md is broken",
        "please run /review",
        "/review\nand then the tests",
    ] {
        assert_eq!(
            projected(&user_row("u1", text))["slashCommand"],
            false,
            "{text}"
        );
    }
    assert_eq!(
        projected(&user_row("u1", "/review the parser"))["slashCommand"],
        true
    );
}
