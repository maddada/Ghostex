//! Claude's "We recommend compacting earlier" offer, rebuilt from Claude Code 2.1.309's own text and
//! layout (its prompt needs 200k tokens of context on a 1M model and a server-side experiment): the
//! VT capture the chat card reads, with the cursor-forward gaps Ink leaves when it repaints, and the
//! plain capture of the same screen.

use serde_json::{json, Map, Value};

use crate::session_chat_claude_dialog::{
    claude_dialog_steps, detect_claude_dialog, CLAUDE_ENTER_AFTER_ARROW_MS,
};
use crate::session_chat_screen_styles::plain_screen_text;
use crate::session_chat_send::SessionChatSendStep;

const VT: &str = include_str!("fixtures/claude-compact-offer.vt");
const PLAIN: &str = include_str!("fixtures/claude-compact-offer.txt");

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap()
}

#[test]
fn both_offer_rows_are_choices_and_the_text_keeps_its_spaces() {
    let dialog = detect_claude_dialog(&plain_screen_text(VT)).expect("dialog");
    assert_eq!(dialog.title, "We recommend compacting earlier");
    let labels: Vec<&str> = dialog.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Keep auto (1m tokens)", "Autocompact at ‹ 400k › tokens"]
    );
    assert!(dialog.rows[0].selected);
    assert!(dialog
        .body
        .starts_with("Each message re-sends the whole conversation, so long conversations"));
    for action in ["left", "right", "confirm", "cancel"] {
        assert!(dialog.actions.iter().any(|a| a == action), "{action}");
    }
}

#[test]
fn the_card_and_the_answer_see_the_same_offer() {
    let card = detect_claude_dialog(&plain_screen_text(VT)).expect("card dialog");
    let answer = detect_claude_dialog(PLAIN).expect("answer dialog");
    assert_eq!(card.id, answer.id);
    assert_eq!(card.rows, answer.rows);
}

#[test]
fn enter_waits_out_claudes_arrow_guard() {
    let dialog = detect_claude_dialog(PLAIN).expect("dialog");
    let keep = claude_dialog_steps(
        &dialog,
        &params(json!({"dialogId": dialog.id, "choiceIndex": 0})),
    )
    .unwrap();
    assert!(matches!(
        keep.as_slice(),
        [
            SessionChatSendStep::VerifyTerminalDialog { .. },
            SessionChatSendStep::Write(enter),
            SessionChatSendStep::SleepMs(_),
        ] if enter == "\r"
    ));
    let lower = claude_dialog_steps(
        &dialog,
        &params(json!({"dialogId": dialog.id, "choiceIndex": 1})),
    )
    .unwrap();
    assert!(matches!(
        lower.as_slice(),
        [
            SessionChatSendStep::VerifyTerminalDialog { .. },
            SessionChatSendStep::Write(down),
            SessionChatSendStep::SleepMs(wait),
            SessionChatSendStep::Write(enter),
            SessionChatSendStep::SleepMs(_),
        ] if down == "\x1b[1;1B" && *wait == CLAUDE_ENTER_AFTER_ARROW_MS && enter == "\r"
    ));
    assert!(CLAUDE_ENTER_AFTER_ARROW_MS > 250);
}

#[test]
fn the_size_steps_with_the_arrow_keys() {
    let dialog = detect_claude_dialog(PLAIN).expect("dialog");
    let right = claude_dialog_steps(
        &dialog,
        &params(json!({"dialogId": dialog.id, "dialogAction": "right"})),
    )
    .unwrap();
    assert!(matches!(
        right.as_slice(),
        [SessionChatSendStep::VerifyTerminalDialog { .. }, SessionChatSendStep::Write(key)]
            if key == "\x1b[1;1C"
    ));
}
