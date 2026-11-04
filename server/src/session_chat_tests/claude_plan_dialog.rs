//! Claude's "Ready to code?" plan approval, read from real captures of one live dialog: the VT
//! capture the chat probe reads and the plain capture an answer rechecks (GitHub #321, #323).

use crate::session_chat_claude_dialog::detect_claude_dialog;
use crate::session_chat_claude_plan::ready_to_code_question;
use crate::session_chat_options::strip_ansi_sgr;
use crate::session_chat_screen_styles::plain_screen_text;

const VT: &str = include_str!("fixtures/claude-ready-to-code.vt");
const PLAIN: &str = include_str!("fixtures/claude-ready-to-code.txt");

#[test]
fn cursor_forward_gaps_read_as_spaces() {
    assert_eq!(
        strip_ansi_sgr("\x1b[0mYes,\x1b[1Cmanually\x1b[Capprove\x1b[3Cedits"),
        "Yes, manually approve   edits"
    );
    assert_eq!(strip_ansi_sgr("a\x1b[?25lb\x1b[45;4H"), "ab");
}

#[test]
fn every_ready_to_code_row_is_a_choice_from_the_vt_capture() {
    let dialog = detect_claude_dialog(&plain_screen_text(VT)).expect("dialog");
    assert_eq!(dialog.title, "Ready to code?");
    let labels: Vec<&str> = dialog.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Yes, and switch to BYPASS PERMISSIONS (no further prompts) for this session",
            "Yes, manually approve edits",
            "Tell Claude what to change",
        ]
    );
    assert!(dialog.body.contains("Here is Claude's plan:"));
}

#[test]
fn the_card_and_the_answer_see_the_same_dialog() {
    let card = detect_claude_dialog(&plain_screen_text(VT)).expect("card dialog");
    let answer = detect_claude_dialog(PLAIN).expect("answer dialog");
    assert_eq!(card.id, answer.id);
    assert_eq!(card.rows, answer.rows);
}

#[test]
fn the_question_under_the_plan_pane() {
    let expected =
        "Claude has written up a plan and is ready to execute. Would you like to proceed?";
    assert_eq!(ready_to_code_question(PLAIN).as_deref(), Some(expected));
    assert_eq!(ready_to_code_question(VT).as_deref(), Some(expected));
}
