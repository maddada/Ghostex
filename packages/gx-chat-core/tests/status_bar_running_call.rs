//! The status bar above the chat box names the running call only where the transcript leaves its
//! words out (`extras/working_card.rs`): it follows the folds the reader opened or closed under
//! every Agent work look, and keeps naming the call when a background task ends mid-call.

use ghostex_gx_chat_core::transcript::chat_look::ChatLookSettings;
use ghostex_gx_chat_core::{ChatContext, ChatState};
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

fn row(id: &str, role: &str, at: i64, blocks: Value) -> ChatMessage {
    message(
        json!({ "id": id, "role": role, "timestamp": at, "source": "transcript", "blocks": blocks }),
    )
}

fn call(id: &str, at: i64, name: &str, input: Value) -> ChatMessage {
    row(
        id,
        "assistant",
        at,
        json!([{ "type": "tool-call", "name": name, "callId": id, "input": input }]),
    )
}

fn result(id: &str, at: i64, call: &str, output: &str) -> ChatMessage {
    row(
        id,
        "tool",
        at,
        json!([{ "type": "tool-result", "callId": call, "output": output }]),
    )
}

fn words(id: &str, at: i64, text: &str) -> ChatMessage {
    row(
        id,
        "assistant",
        at,
        json!([{ "type": "text", "text": text }]),
    )
}

/// Claude's own `<task-notification>` for a background command that finished.
fn task_notification(id: &str, at: i64) -> ChatMessage {
    row(
        id,
        "user",
        at,
        json!([{ "type": "text", "text": "<task-notification>\n<task-id>bg1</task-id>\n<tool-use-id>toolu_bg1</tool-use-id>\n<output-file>/tmp/bg1.output</output-file>\n<status>completed</status>\n<summary>Background command \"Run the watcher\" completed (exit code 0)</summary>\n</task-notification>" }]),
    )
}

fn look(state: &mut ChatState, agent_work: &str) {
    state.transcript_view.chat_look =
        state
            .transcript_view
            .chat_look
            .with_settings(&ChatLookSettings {
                agent_work: Some(agent_work.into()),
                ..Default::default()
            });
}

fn set_rows(state: &mut ChatState, mut rows: Vec<ChatMessage>) {
    for (at, row) in rows.iter_mut().enumerate() {
        row.byte_offset.get_or_insert(at as u64 * 100);
    }
    state.messages.composed = shared(&rows);
    state.messages.list = shared(&rows);
    state.messages.compose_generation += 1;
}

fn chat(agent_work: &str, rows: Vec<ChatMessage>) -> ChatState {
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    look(&mut state, agent_work);
    set_rows(&mut state, rows);
    state.session.server_working = true;
    state.session.working_started_at_ms = Some(T0 as f64);
    state.session.lifecycle = Some(TurnLifecycle {
        state: TurnLifecycleState::Working,
        turn_id: "u1".into(),
        timestamp: Some(T0),
    });
    state
}

fn items(state: &mut ChatState, now: i64) -> Vec<Value> {
    let context = ChatContext {
        now_ms: now as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::session::before_compose(state, &context);
    ghostex_gx_chat_core::transcript::rows::refresh(state, &context);
    ghostex_gx_chat_core::transcript::rows::rows(state, &context)
        .iter()
        .map(|item| serde_json::to_value(item).expect("an item"))
        .collect()
}

fn status_bar(state: &ChatState) -> Option<String> {
    ghostex_gx_chat_core::extras::working_card::working_card(
        state,
        &ghostex_gx_chat_core::WorkingStrip {
            label: Some("Sprouting…".into()),
            ..Default::default()
        },
        &[],
    )
    .label
}

fn report_folds(state: &mut ChatState, expanded: &[String], collapsed: &[String]) {
    let folds = &mut state.extras.search.folds;
    folds.expanded = expanded.iter().cloned().collect();
    folds.collapsed = collapsed.iter().cloned().collect();
}

/// The id of the row the list draws the running call in.
fn running_row(items: &[Value]) -> String {
    let runs = |row: &Value| {
        row["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["status"] == "running"))
    };
    for item in items.iter().rev() {
        let rows: Vec<&Value> = match item["kind"].as_str() {
            Some("message") => vec![&item["message"]],
            Some("summary") => ["cards", "work"]
                .iter()
                .flat_map(|key| item[*key].as_array().into_iter().flatten())
                .collect(),
            _ => Vec::new(),
        };
        if let Some(row) = rows.into_iter().find(|row| runs(row)) {
            return row["id"].as_str().unwrap_or_default().to_string();
        }
    }
    panic!("no running row: {items:#?}")
}

/// A turn running a described Bash call after the agent's words.
fn running_turn() -> Vec<ChatMessage> {
    vec![
        row("u1", "user", T0, json!([{ "type": "text", "text": "go" }])),
        words("w1", T0 + 500, "Looking around first."),
        call("c1", T0 + 1_000, "Bash", json!({ "command": "ls" })),
        result("r1", T0 + 2_000, "c1", "a\nb"),
        words("w2", T0 + 5_000, "Building now."),
        call(
            "c3",
            T0 + 6_000,
            "Bash",
            json!({ "command": "cargo build", "description": "Build the project" }),
        ),
    ]
}

/// Agent work Default: the bar names the running call while its sentence's fold is shut and says
/// something else once the reader opens it, in Normal view; in Detailed the fold starts open and a
/// reader who closes it gets the call's name back.
#[test]
fn default_follows_the_folds_the_reader_opened_or_closed() {
    let mut state = chat("default", running_turn());
    let list = items(&mut state, T0 + 6_500);
    let row = running_row(&list);
    assert_eq!(status_bar(&state).as_deref(), Some("Build the project"));
    report_folds(&mut state, &[format!("tools:{row}")], &[]);
    assert_eq!(
        status_bar(&state).as_deref(),
        Some("Sprouting…"),
        "the opened fold shows the call's words, so the bar shows them nowhere else"
    );
    let mut state = chat("default", running_turn());
    state.transcript_view.simple_override = Some(false);
    let list = items(&mut state, T0 + 6_500);
    let row = running_row(&list);
    assert_eq!(status_bar(&state).as_deref(), Some("Sprouting…"));
    report_folds(&mut state, &[], &[format!("tools:{row}")]);
    assert_eq!(
        status_bar(&state).as_deref(),
        Some("Build the project"),
        "a closed fold leaves the words to the bar"
    );
    // Summary view: the running turn's Active work and its newest row start open.
    let mut state = chat("default", running_turn());
    state.transcript_view.summary_mode = true;
    let list = items(&mut state, T0 + 6_500);
    assert_eq!(status_bar(&state).as_deref(), Some("Sprouting…"));
    let summary = list
        .iter()
        .rev()
        .find(|item| item["kind"] == "summary")
        .and_then(|item| item["id"].as_str())
        .expect("a summary turn")
        .to_string();
    report_folds(&mut state, &[], &[format!("active-work:{summary}")]);
    assert_eq!(
        status_bar(&state).as_deref(),
        Some("Build the project"),
        "a closed Active work leaves the words to the bar"
    );
}

/// Agent work Quiet: a background command that ends while a foreground call still runs reaches the
/// chat as a user row (`<task-notification>`). It is the agent's work, not a prompt, so the bar
/// keeps naming the call that runs.
#[test]
fn a_background_task_ending_mid_call_keeps_the_running_call_named() {
    for agent_work in ["quiet", "reduced", "default"] {
        let mut rows = running_turn();
        rows.push(task_notification("n1", T0 + 7_000));
        let mut state = chat(agent_work, rows);
        items(&mut state, T0 + 7_500);
        assert_eq!(
            status_bar(&state).as_deref(),
            Some("Build the project"),
            "{agent_work}"
        );
    }
}
