//! A finished turn's files fold shows only while the turn's card is open (CDXC:SessionChat
//! 2026-11-08 in `transcript/file_change_rows.rs`): the closed "Previous turn" / "Worked for" card
//! leaves the files to its "N files" pill instead of drawing "Edited N files" under it too.

use ghostex_gx_chat_core::transcript::file_change_rows::{turn_files_fold, TurnFilesFold};
use ghostex_gx_chat_core::{ChatContext, ChatState, TranscriptItem};
use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};

const T0: i64 = 1_793_420_000_000;

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat message")
}

fn text(id: &str, role: &str, at: i64, text: &str) -> ChatMessage {
    message(json!({
        "id": id, "role": role, "timestamp": at, "source": "transcript",
        "blocks": [{ "type": "text", "text": text }],
    }))
}

fn edit(id: &str, at: i64, path: &str) -> [ChatMessage; 2] {
    [
        message(json!({
            "id": id, "role": "assistant", "timestamp": at, "source": "transcript",
            "blocks": [{ "type": "tool-call", "name": "Edit", "callId": id,
                "input": {"file_path": path, "old_string": "a", "new_string": "b"} }],
        })),
        message(json!({
            "id": format!("{id}-result"), "role": "tool", "timestamp": at + 500,
            "source": "transcript",
            "blocks": [{ "type": "tool-result", "callId": id, "output": "ok" }],
        })),
    ]
}

fn finished_turn(rows: Vec<ChatMessage>) -> TranscriptItem {
    let mut rows = rows;
    for (at, row) in rows.iter_mut().enumerate() {
        row.byte_offset.get_or_insert(at as u64 * 100);
    }
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state.messages.composed = rows.clone();
    state.messages.list = rows;
    let context = ChatContext {
        now_ms: (T0 + 600_000) as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::transcript::rows::rows(&state, &context)
        .into_iter()
        .find(|item| matches!(item, TranscriptItem::CompletedWork { .. }))
        .expect("the finished turn folds")
}

#[test]
fn the_files_fold_shows_under_an_open_turn_only() {
    let mut rows = vec![text("u1", "user", T0, "fix both files")];
    rows.extend(edit("c1", T0 + 1_000, "/repo/src/a.rs"));
    rows.extend(edit("c2", T0 + 2_000, "/repo/src/b.rs"));
    rows.push(text("a1", "assistant", T0 + 3_000, "Done."));
    let turn = finished_turn(rows);

    let TranscriptItem::CompletedWork {
        files,
        simple_files_label,
        stats,
        ..
    } = &turn
    else {
        unreachable!()
    };
    assert_eq!(simple_files_label, "Edited 2 files");
    assert_eq!(files.len(), 2);
    // The closed card still says how many files changed, in its pill.
    assert!(
        stats.iter().any(|stat| stat["kind"] == "files"),
        "the card's counts carry the files: {stats:?}"
    );

    assert_eq!(turn_files_fold(&turn, false), TurnFilesFold::Hidden);
    assert_eq!(turn_files_fold(&turn, true), TurnFilesFold::Files);
}

#[test]
fn a_turn_that_changed_no_files_has_no_fold_even_open() {
    let turn = TranscriptItem::CompletedWork {
        id: "u1".into(),
        label: "Previous turn".into(),
        stats: Vec::new(),
        files_card: Value::Null,
        files: Vec::new(),
        files_label: "0 files changed".into(),
        simple_files_label: String::new(),
        expandable: true,
        deferred: None,
        work: Vec::new(),
        questions: Vec::new(),
        sent_messages: Vec::new(),
        workflows: Vec::new(),
        artifacts: Vec::new(),
        final_message: None,
        read_images: Vec::new(),
    };
    assert_eq!(turn_files_fold(&turn, true), TurnFilesFold::Hidden);
}

#[test]
fn an_unread_turn_folds_its_known_paths_once_open() {
    let turn = TranscriptItem::CompletedWork {
        id: "u1".into(),
        label: "Worked for 12s".into(),
        stats: Vec::new(),
        files_card: Value::Null,
        files: Vec::new(),
        files_label: "1 file changed".into(),
        simple_files_label: "Edited 1 file".into(),
        expandable: true,
        deferred: Some(json!({"filePaths": ["/repo/src/a.rs"]})),
        work: Vec::new(),
        questions: Vec::new(),
        sent_messages: Vec::new(),
        workflows: Vec::new(),
        artifacts: Vec::new(),
        final_message: None,
        read_images: Vec::new(),
    };
    assert_eq!(turn_files_fold(&turn, false), TurnFilesFold::Hidden);
    assert_eq!(turn_files_fold(&turn, true), TurnFilesFold::Unread);
}
