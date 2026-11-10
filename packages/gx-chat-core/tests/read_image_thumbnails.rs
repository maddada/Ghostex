//! A picture an agent opened from disk shows on its tool row, and its turn's fold keeps a strip of
//! them (CDXC:SessionChat 2026-11-02 in `transcript/read_images.rs`).

use ghostex_gx_chat_core::{ChatContext, ChatState, TranscriptItem};
use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};

const T0: i64 = 1_793_420_000_000;

/// The rows as the core holds them (`ChatRow`).
fn shared(rows: &[ChatMessage]) -> Vec<ghostex_gx_chat_core::state::ChatRow> {
    rows.iter().cloned().map(std::sync::Arc::new).collect()
}

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat message")
}

fn user(id: &str, at: i64, text: &str) -> ChatMessage {
    message(json!({
        "id": id, "role": "user", "timestamp": at, "source": "transcript",
        "blocks": [{ "type": "text", "text": text }],
    }))
}

fn assistant(id: &str, at: i64, text: &str) -> ChatMessage {
    message(json!({
        "id": id, "role": "assistant", "timestamp": at, "source": "transcript",
        "blocks": [{ "type": "text", "text": text }],
    }))
}

/// One call and its answer.
fn call(id: &str, at: i64, name: &str, input: Value, failed: bool) -> [ChatMessage; 2] {
    let mut result = json!({ "type": "tool-result", "callId": id, "output": "ok" });
    if failed {
        result["isError"] = json!(true);
        result["output"] = json!("File does not exist.");
    }
    [
        message(json!({
            "id": id, "role": "assistant", "timestamp": at, "source": "transcript",
            "blocks": [{ "type": "tool-call", "name": name, "callId": id, "input": input }],
        })),
        message(json!({
            "id": format!("{id}-result"), "role": "tool", "timestamp": at + 500,
            "source": "transcript", "blocks": [result],
        })),
    ]
}

fn state_with(agent: &str, mut rows: Vec<ChatMessage>) -> ChatState {
    for (at, row) in rows.iter_mut().enumerate() {
        row.byte_offset.get_or_insert(at as u64 * 100);
    }
    let mut state = ChatState::default();
    state.session.agent = Some(agent.into());
    state.messages.composed = shared(&rows);
    state.messages.list = shared(&rows);
    state
}

fn items(state: &ChatState) -> Vec<TranscriptItem> {
    let context = ChatContext {
        now_ms: (T0 + 600_000) as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::transcript::rows::rows(state, &context)
}

fn paths(images: &[Value]) -> Vec<&str> {
    images
        .iter()
        .map(|image| image["path"].as_str().unwrap_or_default())
        .collect()
}

#[test]
fn a_finished_claude_turn_keeps_the_pictures_it_read_under_its_fold() {
    let mut rows = vec![user("u1", T0, "look at the screenshots")];
    rows.extend(call(
        "c1",
        T0 + 1_000,
        "Read",
        json!({"file_path": "/shots/toast-before.png"}),
        false,
    ));
    rows.extend(call(
        "c2",
        T0 + 2_000,
        "Bash",
        json!({"command": "cargo xtask web-build"}),
        false,
    ));
    rows.extend(call(
        "c3",
        T0 + 3_000,
        "Read",
        json!({"file_path": "/shots/toast-after.png"}),
        false,
    ));
    rows.extend(call(
        "c4",
        T0 + 4_000,
        "Read",
        json!({"file_path": "/shots/gone.png"}),
        true,
    ));
    rows.push(assistant(
        "a1",
        T0 + 5_000,
        "The toast overlaps the composer.",
    ));
    let items = items(&state_with("claude", rows));
    let (work, read_images, stats) = items
        .iter()
        .find_map(|item| match item {
            TranscriptItem::CompletedWork {
                work,
                read_images,
                stats,
                ..
            } => Some((work, read_images, stats)),
            _ => None,
        })
        .expect("the finished turn folds");
    // The failed read showed the agent nothing, so it bubbles nothing up.
    assert_eq!(
        paths(read_images),
        ["/shots/toast-before.png", "/shots/toast-after.png"]
    );
    // The card's counts start with what its strip holds.
    assert_eq!(stats[0], json!({"kind": "images", "label": "2 images"}));
    // Each Read row carries its own thumbnail.
    let row_images: Vec<Vec<String>> = work
        .iter()
        .flat_map(|message| message["tools"].as_array().cloned().unwrap_or_default())
        .filter_map(|row| {
            row["images"]
                .as_array()
                .map(|images| paths(images).into_iter().map(str::to_string).collect())
        })
        .collect();
    assert_eq!(
        row_images,
        [
            vec!["/shots/toast-before.png"],
            vec!["/shots/toast-after.png"]
        ]
    );
    assert!(read_images.iter().all(|image| image["transport"] == "read"));
}

#[test]
fn a_codex_script_that_viewed_pictures_bubbles_them_up_in_order() {
    let mut rows = vec![user("u1", T0, "check the screenshots")];
    rows.extend(call(
        "c1",
        T0 + 1_000,
        "exec",
        json!("const a=await tools.view_image({path:\"/s/one.png\"}); image(a.image_url);\nconst b=await tools.view_image({path:\"/s/two.jpg\",detail:\"high\"}); image(b.image_url);\n"),
        false,
    ));
    rows.extend(call(
        "c2",
        T0 + 2_000,
        "view_image",
        json!({"path": "/s/three.webp"}),
        false,
    ));
    rows.push(assistant("a1", T0 + 3_000, "Done."));
    let items = items(&state_with("codex", rows));
    let read_images = items
        .iter()
        .find_map(|item| match item {
            TranscriptItem::CompletedWork { read_images, .. } => Some(read_images),
            _ => None,
        })
        .expect("the finished turn folds");
    assert_eq!(
        paths(read_images),
        ["/s/one.png", "/s/two.jpg", "/s/three.webp"]
    );
}

#[test]
fn a_turn_that_read_no_pictures_has_no_strip() {
    let mut rows = vec![user("u1", T0, "read the code")];
    rows.extend(call(
        "c1",
        T0 + 1_000,
        "Read",
        json!({"file_path": "/repo/src/main.rs"}),
        false,
    ));
    rows.push(assistant("a1", T0 + 2_000, "Read it."));
    let items = items(&state_with("claude", rows));
    for item in &items {
        if let TranscriptItem::CompletedWork { read_images, .. } = item {
            assert!(read_images.is_empty());
        }
    }
    // An empty strip is left off the wire.
    let wire = serde_json::to_value(&items).unwrap();
    assert!(!wire.to_string().contains("readImages\":[{"));
}

fn read_turn(prompt: &str, read_path: &str) -> Vec<ChatMessage> {
    let mut rows = vec![user("u1", T0, prompt)];
    rows.extend(call(
        "c1",
        T0 + 1_000,
        "Read",
        json!({"file_path": read_path}),
        false,
    ));
    rows.extend(call(
        "c2",
        T0 + 2_000,
        "Read",
        json!({"file_path": "/shots/other.png"}),
        false,
    ));
    rows.push(assistant("a1", T0 + 3_000, "Looked."));
    rows
}

/// What the finished turn shows: its strip, and every thumbnail on its rows.
fn shown(state: &ChatState) -> (Vec<String>, Vec<String>) {
    let items = items(state);
    let (work, read_images) = items
        .iter()
        .find_map(|item| match item {
            TranscriptItem::CompletedWork {
                work, read_images, ..
            } => Some((work.clone(), read_images.clone())),
            _ => None,
        })
        .expect("the finished turn folds");
    let on_rows = work
        .iter()
        .flat_map(|message| message["tools"].as_array().cloned().unwrap_or_default())
        .flat_map(|row| row["images"].as_array().cloned().unwrap_or_default())
        .map(|image| image["path"].as_str().unwrap_or_default().to_string())
        .collect();
    let strip = paths(&read_images)
        .into_iter()
        .map(str::to_string)
        .collect();
    (strip, on_rows)
}

#[test]
fn a_read_of_the_picture_the_user_pasted_keeps_its_row_tile_but_leaves_the_strip() {
    // The composer saved the paste under /var/folders; the agent read it through /private. The
    // open row still shows it (it only says "Read ghostex-paste-1.png"); the closed fold's strip
    // and its "N images" count leave it out (2026-11-06, superseding the same day's "every
    // strip" rule).
    let state = state_with(
        "claude",
        read_turn(
            "[Image #1](/var/folders/ab/T/ghostex-paste-1.png) what is wrong here?",
            "/private/var/folders/ab/T/ghostex-paste-1.png",
        ),
    );
    let (strip, on_rows) = shown(&state);
    assert_eq!(strip, ["/shots/other.png"]);
    assert_eq!(
        on_rows,
        [
            "/private/var/folders/ab/T/ghostex-paste-1.png",
            "/shots/other.png"
        ]
    );
    assert_eq!(
        marked(&state),
        ["/private/var/folders/ab/T/ghostex-paste-1.png"]
    );
    assert_eq!(image_stat(&state).as_deref(), Some("1 image"));
}

#[test]
fn a_turn_that_only_read_the_users_picture_has_no_strip_and_no_count() {
    let mut rows = vec![user(
        "u1",
        T0,
        "![shot](/Users/me/Desktop/shot.png) check this",
    )];
    rows.extend(call(
        "c1",
        T0 + 1_000,
        "Read",
        json!({"file_path": "/Users/me/Desktop/shot.png"}),
        false,
    ));
    rows.push(assistant("a1", T0 + 3_000, "Looked."));
    let state = state_with("claude", rows);
    let (strip, on_rows) = shown(&state);
    assert!(strip.is_empty());
    assert_eq!(on_rows, ["/Users/me/Desktop/shot.png"]);
    assert_eq!(image_stat(&state), None);
    // Each message's own strip (a closed text row, a closed "N tool calls") is empty too.
    let work = completed_work(&state);
    assert!(work
        .iter()
        .all(|message| message["readImages"].as_array().is_none_or(Vec::is_empty)));
}

#[test]
fn a_read_of_the_same_picture_at_another_path_leaves_the_strip_once_both_are_read() {
    let mut state = state_with(
        "claude",
        read_turn(
            "![shot](/Users/me/Desktop/shot.png) check this",
            "/repo/assets/shot-copy.png",
        ),
    );
    // Until the bytes are known the two paths are two pictures.
    assert_eq!(
        shown(&state).0,
        ["/repo/assets/shot-copy.png", "/shots/other.png"]
    );
    ghostex_gx_chat_core::transcript::read_images::note_picture_bytes(
        &mut state,
        "/repo/assets/shot-copy.png",
        "iVBORw0KGgoSAME",
    );
    ghostex_gx_chat_core::transcript::read_images::note_picture_bytes(
        &mut state,
        "/shots/other.png",
        "iVBORw0KGgoDIFFERENT",
    );
    ghostex_gx_chat_core::transcript::read_images::note_picture_bytes(
        &mut state,
        "/Users/me/Desktop/shot.png",
        "iVBORw0KGgoSAME",
    );
    let (strip, on_rows) = shown(&state);
    assert_eq!(strip, ["/shots/other.png"]);
    assert_eq!(on_rows, ["/repo/assets/shot-copy.png", "/shots/other.png"]);
    assert_eq!(marked(&state), ["/repo/assets/shot-copy.png"]);
}

fn completed_work(state: &ChatState) -> Vec<Value> {
    items(state)
        .iter()
        .find_map(|item| match item {
            TranscriptItem::CompletedWork { work, .. } => {
                Some(work.iter().map(|row| row.as_ref().clone()).collect())
            }
            _ => None,
        })
        .expect("the finished turn folds")
}

/// The row pictures core marked as the user's own, which a strip built from rows leaves out.
fn marked(state: &ChatState) -> Vec<String> {
    completed_work(state)
        .iter()
        .flat_map(|message| message["tools"].as_array().cloned().unwrap_or_default())
        .flat_map(|row| row["images"].as_array().cloned().unwrap_or_default())
        .filter(ghostex_gx_chat_core::transcript::read_images::is_user_shown)
        .map(|image| image["path"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The finished turn card's "N images" count.
fn image_stat(state: &ChatState) -> Option<String> {
    items(state).iter().find_map(|item| match item {
        TranscriptItem::CompletedWork { stats, .. } => stats
            .iter()
            .find(|stat| stat["kind"] == "images")
            .and_then(|stat| stat["label"].as_str().map(str::to_string)),
        _ => None,
    })
}
