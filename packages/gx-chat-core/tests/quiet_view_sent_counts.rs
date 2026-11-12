//! A Quiet View line counts a call drawn as a "Message to …" card as a message sent, never also as
//! a tool call (CDXC:SessionChat 2026-11-12 in `transcript/quiet_view.rs` on `Counts`).

use ghostex_gx_chat_core::transcript::quiet_view::{apply_settings, QuietViewSettings};
use ghostex_gx_chat_core::{assemble_with, AssembleCache, ChatContext, ChatState};
use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};

const T0: i64 = 1_794_358_800_000;
const THREAD: &str = "S90:P3lv0:G2ab";

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat message")
}

fn text(id: &str, role: &str, at: i64, text: &str) -> ChatMessage {
    message(json!({
        "id": id, "role": role, "timestamp": at, "source": "transcript",
        "blocks": [{ "type": "text", "text": text }],
    }))
}

/// One call and its result, as two rows.
fn call(id: &str, at: i64, name: &str, input: Value, output: &str) -> [ChatMessage; 2] {
    [
        message(json!({
            "id": id, "role": "assistant", "timestamp": at, "source": "transcript",
            "blocks": [{ "type": "tool-call", "name": name, "callId": id, "input": input }],
        })),
        message(json!({
            "id": format!("{id}-result"), "role": "tool", "timestamp": at + 1_000,
            "source": "transcript",
            "blocks": [{ "type": "tool-result", "callId": id, "output": output }],
        })),
    ]
}

fn bash(id: &str, at: i64, command: &str, output: &str) -> [ChatMessage; 2] {
    call(id, at, "Bash", json!({ "command": command }), output)
}

fn start_thread(id: &str, at: i64) -> [ChatMessage; 2] {
    bash(
        id,
        at,
        "ghostex coordinator start-thread --title \"Verify English lesson layout\" --agent claude --body-file /tmp/brief.md",
        &format!("Started thread \"Verify English lesson layout\" ({THREAD})"),
    )
}

/// The coordinator's turn of the report: it writes the brief, starts the thread, and keeps its
/// books (`link add`, `item add`), then answers.
fn bookkeeping_turn(sends: &[[ChatMessage; 2]]) -> Vec<ChatMessage> {
    let mut rows = vec![text("u1", "user", T0, "Check the English lesson layout.")];
    rows.extend(call(
        "w1",
        T0 + 5_000,
        "Write",
        json!({ "file_path": "/tmp/brief.md", "content": "Verify the layout." }),
        "File created successfully at: /tmp/brief.md",
    ));
    for send in sends {
        rows.extend(send.iter().cloned());
    }
    rows.extend(bash(
        "b2",
        T0 + 20_000,
        &format!("ghostex link add {THREAD} --url https://example.com"),
        "Linked.",
    ));
    rows.extend(bash(
        "b3",
        T0 + 25_000,
        "ghostex coordinator item add --title \"English lesson layout\"",
        "Added W1.",
    ));
    rows.push(text(
        "a1",
        "assistant",
        T0 + 30_000,
        "Started a thread on it.",
    ));
    rows
}

fn quiet_chat(mut rows: Vec<ChatMessage>) -> ChatState {
    for (at, row) in rows.iter_mut().enumerate() {
        row.byte_offset.get_or_insert(at as u64 * 100);
    }
    let rows: Vec<_> = rows.into_iter().map(std::sync::Arc::new).collect();
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state.core.controller_started = true;
    state.messages.composed = rows.clone();
    state.messages.list = rows;
    apply_settings(
        &mut state,
        QuietViewSettings {
            coordinators: true,
            chats: true,
            expand_last: false,
        },
    );
    state
}

/// Each quiet line's text.
fn lines(state: &mut ChatState) -> Vec<String> {
    let context = ChatContext {
        now_ms: (T0 + 600_000) as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::transcript::rows::refresh(state, &context);
    let quiet = assemble_with(state, &context, &mut AssembleCache::default())
        .quiet_view
        .expect("the chat is in Quiet View");
    quiet
        .stretches
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

#[test]
fn a_started_thread_alone_is_one_message_sent() {
    let mut rows = vec![text("u1", "user", T0, "Check the English lesson layout.")];
    rows.extend(start_thread("s1", T0 + 10_000));
    rows.push(text(
        "a1",
        "assistant",
        T0 + 30_000,
        "Started a thread on it.",
    ));
    assert_eq!(
        lines(&mut quiet_chat(rows)),
        ["Worked for 30s · 1 message sent"]
    );
}

#[test]
fn bookkeeping_around_a_started_thread_counts_only_its_own_calls() {
    let rows = bookkeeping_turn(&[start_thread("s1", T0 + 10_000)]);
    assert_eq!(
        lines(&mut quiet_chat(rows)),
        ["Worked for 30s · 1 message sent · 3 tool calls · 1 file edited"]
    );
}

#[test]
fn agents_send_is_one_message_sent() {
    let mut rows = vec![text("u1", "user", T0, "Tell the thread to go on.")];
    rows.extend(bash(
        "s1",
        T0 + 10_000,
        &format!("ghostex agents send {THREAD} \"Go on.\" --json"),
        &format!(
            r#"{{"ok":true,"status":"delivered","recipient":{{"globalRef":"{THREAD}","title":"Verify English lesson layout"}}}}"#
        ),
    ));
    rows.push(text("a1", "assistant", T0 + 30_000, "Told it."));
    assert_eq!(
        lines(&mut quiet_chat(rows)),
        ["Worked for 30s · 1 message sent"]
    );
}

#[test]
fn claude_send_message_is_one_message_sent() {
    let mut rows = vec![text("u1", "user", T0, "Ask the reviewer.")];
    rows.extend(call(
        "s1",
        T0 + 10_000,
        "SendMessage",
        json!({ "to": "reviewer", "message": "Look at the layout." }),
        "Message sent to reviewer.",
    ));
    rows.push(text("a1", "assistant", T0 + 30_000, "Asked."));
    assert_eq!(
        lines(&mut quiet_chat(rows)),
        ["Worked for 30s · 1 message sent"]
    );
}

/// While the turn still runs its rows are counted one by one, not from a finished turn's tally.
#[test]
fn a_running_turn_counts_a_started_thread_once() {
    let mut rows = vec![text("u1", "user", T0, "Check the English lesson layout.")];
    rows.extend(start_thread("s1", T0 + 10_000));
    rows.extend(bash(
        "b2",
        T0 + 20_000,
        "ghostex coordinator item add --title x",
        "Added W1.",
    ));
    let mut state = quiet_chat(rows);
    state.session.server_working = true;
    assert_eq!(
        lines(&mut state),
        ["Working · 1 message sent · 1 tool call so far"]
    );
}
