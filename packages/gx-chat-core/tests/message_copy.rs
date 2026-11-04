//! What a message's Copy copies: the whole message as Markdown, on every client.
//!
//! The phone's long-press menu and the desktop's Copy button both read the projected message's
//! `copyText` (`apps/mobile/app/src/chat/native/transcript/MessageRow.tsx`, desktop
//! `message_actions.rs`), so these tests pin that field for the three kinds of reply a user asked
//! to copy in full: a long reply, a reply with code blocks, and a reply that asks questions.

use ghostex_gx_chat_core::composer::transcript_menu::transcript_menu_rows;
use ghostex_gx_chat_core::protocol::ChatMessage;
use ghostex_gx_chat_core::transcript::line_breaks::AgentLineBreaks;
use ghostex_gx_chat_core::transcript::presentation::project_message;
use ghostex_gx_chat_core::ChatContext;
use serde_json::{json, Value};
use std::cell::OnceCell;
use std::collections::HashMap;

fn reply(markdown: &str) -> ChatMessage {
    serde_json::from_value(json!({
        "id": "m1",
        "role": "assistant",
        "blocks": [{ "type": "text", "text": markdown }],
        "timestamp": 1_700_000_000_000_i64,
        "source": "claude",
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

/// The Copy row the transcript menu offers for a message held on the phone: its selection is the
/// message's `copyText`, and the row copies exactly that selection.
fn menu_copy(projected: &Value) -> String {
    let selection = projected["copyText"].as_str().unwrap().trim();
    let rows = transcript_menu_rows(None, selection, false);
    let copy = rows
        .iter()
        .find(|row| row.label == "Copy")
        .expect("the menu has a Copy row");
    assert_eq!(copy.disabled, Some(false));
    serde_json::to_value(&copy.command).unwrap()["text"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_long_reply_copies_in_full() {
    let paragraphs: Vec<String> = (1..=60)
        .map(|n| {
            format!("Paragraph {n}: the quick brown fox jumps over the lazy dog, again and again.")
        })
        .collect();
    let markdown = paragraphs.join("\n\n");
    let model = projected(&reply(&markdown));
    assert_eq!(model["copyText"], markdown);
    assert_eq!(menu_copy(&model), markdown);
}

#[test]
fn a_reply_with_code_blocks_copies_the_blocks_too() {
    let markdown = "Run this:\n\n```bash\ncargo test --all\n```\n\nThen edit:\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\nDone.";
    let model = projected(&reply(markdown));
    assert_eq!(model["copyText"], markdown);
    assert_eq!(menu_copy(&model), markdown);
}

#[test]
fn a_reply_with_a_question_form_copies_every_question_and_choice() {
    let markdown = "Before I start:\n\n```questions\n[{\"question\":\"Which database?\",\"options\":[{\"label\":\"Postgres\"},{\"label\":\"SQLite\"}]},{\"question\":\"Add tests?\",\"options\":[{\"label\":\"Yes\"},{\"label\":\"No\"}]}]\n```\n\nTell me and I will go on.";
    let model = projected(&reply(markdown));
    let copied = menu_copy(&model);
    assert_eq!(model["copyText"], markdown);
    assert_eq!(copied, markdown);
    for part in [
        "Which database?",
        "Postgres",
        "SQLite",
        "Add tests?",
        "Yes",
        "No",
        "Tell me and I will go on.",
    ] {
        assert!(copied.contains(part), "{part} missing from the copy");
    }
}
