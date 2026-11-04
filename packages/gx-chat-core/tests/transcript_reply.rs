//! The transcript menu's Reply row and the chat box its popover leaves behind
//! (`composer/transcript_menu.rs`, CDXC:SessionChat 2026-10-29).

use ghostex_gx_chat_core::composer::transcript_menu::{
    transcript_menu_rows, transcript_quote, transcript_reply_draft,
};
use ghostex_gx_chat_core::query::{answer_query, Query};
use ghostex_gx_chat_core::{ChatContext, ChatState};
use serde_json::{json, Value};

fn labels(rows: &[ghostex_gx_chat_core::composer::reference_menu::ReferenceMenuRow]) -> Vec<&str> {
    rows.iter().map(|row| row.label.as_str()).collect()
}

#[test]
fn reply_follows_add_to_chat_when_text_is_selected() {
    let rows = transcript_menu_rows(None, "the selected words", false);
    assert_eq!(labels(&rows), ["Copy", "Add to Chat", "Reply"]);
    let reply = serde_json::to_value(&rows[2]).unwrap();
    assert_eq!(
        reply["command"],
        json!({"quote": "> the selected words\n\n", "type": "replyToSelection"})
    );
    assert_eq!(reply["disabled"], false);
}

#[test]
fn reply_is_absent_without_a_selection_and_disabled_with_add_to_chat() {
    assert_eq!(labels(&transcript_menu_rows(None, "", false)), ["Copy"]);
    let rows = transcript_menu_rows(None, "words", true);
    let disabled: Vec<_> = rows.iter().map(|row| row.disabled).collect();
    assert_eq!(disabled, [Some(false), Some(true), Some(true)]);
}

#[test]
fn a_reply_goes_under_the_quote_at_the_end_of_the_draft() {
    let quote = transcript_quote("line one\n\nline two");
    assert_eq!(
        transcript_reply_draft("", &quote, "Why this one?"),
        "> line one\n>\n> line two\n\nWhy this one?"
    );
    // Existing text keeps the blank line Add to Chat puts between it and a quote.
    assert_eq!(
        transcript_reply_draft("Earlier draft", &quote, "  Why?\nAnd how?\n\n"),
        "Earlier draft\n\n> line one\n>\n> line two\n\nWhy?\nAnd how?"
    );
    assert_eq!(
        transcript_reply_draft("Ends with a break\n", &quote, "ok"),
        "Ends with a break\n\n> line one\n>\n> line two\n\nok"
    );
}

#[test]
fn an_empty_reply_adds_what_add_to_chat_adds() {
    let quote = transcript_quote("words");
    let add_to_chat = serde_json::to_value(&transcript_menu_rows(None, "words", false)[1]).unwrap()
        ["command"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    for current in ["", "draft", "draft\n", "draft\n\n"] {
        assert_eq!(
            transcript_reply_draft(current, &quote, " \n "),
            ghostex_gx_chat_core::composer::transcript_menu::append_draft_text(
                current,
                &add_to_chat
            )
        );
    }
}

#[test]
fn the_phone_asks_for_the_same_draft_by_query() {
    let query = Query::from_wire("transcriptReply").expect("a query");
    assert_eq!(query.as_str(), "transcriptReply");
    let answer = answer_query(
        &ChatState::default(),
        &ChatContext::default(),
        query,
        &[json!({"draft": "Earlier", "quote": "> words\n\n", "reply": "Why?"})],
    )
    .expect("an answer");
    let answer: Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(answer, "Earlier\n\n> words\n\nWhy?");
    // No quote is a refusal, not an empty draft.
    assert!(answer_query(
        &ChatState::default(),
        &ChatContext::default(),
        query,
        &[json!({"draft": "Earlier", "reply": "Why?"})],
    )
    .is_none());
}
