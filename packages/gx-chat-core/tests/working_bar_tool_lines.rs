//! The working bar above the composer names a running call in words, never by its arguments as
//! JSON (CDXC:SessionChat 2026-11-04 in `transcript/tool_summary.rs`), and the transcript's tool
//! rows preview the same way.

use ghostex_gx_chat_core::extras::working_card::working_card;
use ghostex_gx_chat_core::transcript::tool_summary::{
    summarize_tool_input, tool_display_name, tool_input_gist,
};
use ghostex_gx_chat_core::{ChatState, WorkingStrip};
use ghostex_gx_protocol::{ChatMessage, TurnLifecycle, TurnLifecycleState};
use serde_json::{json, Value};

const T0: i64 = 1_793_818_000_000;

/// The rows as the core holds them (`ChatRow`).
fn shared(rows: &[ChatMessage]) -> Vec<ghostex_gx_chat_core::state::ChatRow> {
    rows.iter().cloned().map(std::sync::Arc::new).collect()
}

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat message")
}

/// A turn that is running one call the transcript has recorded but not answered.
fn running(name: &str, input: Value) -> ChatState {
    let mut state = ChatState::default();
    state.session.server_working = true;
    state.session.working_started_at_ms = Some(T0 as f64);
    state.session.lifecycle = Some(TurnLifecycle {
        state: TurnLifecycleState::Working,
        turn_id: "u1".into(),
        timestamp: Some(T0),
    });
    state.messages.list = shared(&[
        message(json!({
            "id": "u1", "role": "user", "timestamp": T0, "source": "transcript",
            "blocks": [{ "type": "text", "text": "go" }],
        })),
        message(json!({
            "id": "a1", "role": "assistant", "timestamp": T0 + 1_000, "source": "transcript",
            "blocks": [{ "type": "tool-call", "name": name, "callId": "c1", "input": input }],
        })),
    ]);
    state
}

fn bar(name: &str, input: Value) -> String {
    let card = working_card(&running(name, input), &WorkingStrip::default(), &[]);
    card.label.expect("the bar names the running call")
}

fn assert_plain(line: &str) {
    for mark in ['{', '}', '[', ']', '"'] {
        assert!(!line.contains(mark), "{line:?} carries {mark:?}");
    }
}

#[test]
fn the_artifact_call_from_the_screenshot_reads_as_its_action() {
    let line = bar(
        "Artifact",
        json!({"action":"quickstart","intent":"other","design_systems":false}),
    );
    assert_eq!(line, "Artifact: quickstart");
}

#[test]
fn an_unknown_tool_shows_one_human_value_or_its_name_alone() {
    let titled = bar(
        "PublishReport",
        json!({"title": "Q3 numbers\nsecond line", "body": {"rows": [1, 2, 3]}}),
    );
    assert_eq!(titled, "Publish report: Q3 numbers");
    let bare = bar(
        "ConfigureThing",
        json!({"enabled": true, "level": 3, "opts": {"a": 1}}),
    );
    assert_eq!(bare, "Configure thing");
    let worded = bar("render_view", json!({"operation": "full_refresh"}));
    assert_eq!(worded, "Render view: full refresh");
    let long = bar("Note", json!({"text": "word ".repeat(60)}));
    assert_plain(&long);
    assert!(long.chars().count() <= "Note: ".len() + 60, "{long:?}");
    assert!(long.ends_with('\u{2026}'));
}

#[test]
fn mcp_calls_read_as_words() {
    let linear = bar(
        "mcp__linear__get_issue",
        json!({"id": "SPX-412", "extra": {"x": 1}}),
    );
    assert_eq!(linear, "Read SPX-412 in Linear");
    let unknown = bar("mcp__spaceo__spaceo_click", json!({"x": 10, "y": 20}));
    assert_eq!(unknown, "Use spaceo click");
    assert_plain(&unknown);
}

#[test]
fn a_tool_input_never_previews_as_json() {
    assert_eq!(
        summarize_tool_input(
            &json!({"action":"quickstart","intent":"other","design_systems":false})
        ),
        "quickstart"
    );
    assert_eq!(summarize_tool_input(&json!({"enabled": true, "n": 2})), "");
    assert_eq!(
        summarize_tool_input(&json!(["git", "status"])),
        "git status"
    );
    assert_eq!(summarize_tool_input(&json!([{"a": 1}])), "");
    assert_eq!(
        summarize_tool_input(&json!("{\"title\":\"Hello\"}")),
        "Hello"
    );
    assert_eq!(tool_input_gist(&json!({"title": "{\"nested\":1}"})), None);
}

#[test]
fn tool_names_read_as_words() {
    assert_eq!(tool_display_name("Artifact"), "Artifact");
    assert_eq!(tool_display_name("ExitPlanMode"), "Exit plan mode");
    assert_eq!(tool_display_name("apply_patch"), "Apply patch");
    assert_eq!(tool_display_name("HTTPServerStart"), "HTTP server start");
}
