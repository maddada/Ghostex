use super::*;
use crate::session_chat::SessionChatQuestionOption;

/// A real Claude Code 2.1.311 AskUserQuestion with four questions, on its fourth tab
/// ("Discount", three options), with Claude's task list under the dialog (GitHub #350's card).
const TAB4: &str = include_str!("../tests/fixtures/claude-question-four-steps-tab4.txt");
const DISCOUNT: &str = "Which discount applies — the customer’s code or none?";

fn question(text: &str, options: &[&str]) -> SessionChatQuestion {
    SessionChatQuestion {
        question: text.to_string(),
        header: None,
        multi_select: false,
        allow_custom: None,
        tool_name: None,
        recommended: None,
        preview_layout: false,
        options: options
            .iter()
            .map(|label| SessionChatQuestionOption {
                label: (*label).to_string(),
                description: None,
                preview: None,
            })
            .collect(),
    }
}

fn four_questions() -> Vec<SessionChatQuestion> {
    vec![
        question(
            "Which plan should the order use?",
            &["Basic", "Pro", "Team"],
        ),
        question("How should we bill?", &["Monthly", "Yearly"]),
        question("Which region hosts the data?", &["EU", "US", "Asia"]),
        question(DISCOUNT, &["None", "WELCOME10", "LOYAL20"]),
    ]
}

fn placed(tab: usize, rows_below_first: usize) -> ClaudeQuestionScreen {
    ClaudeQuestionScreen::Placed(ClaudeQuestionSelectorPosition {
        tab,
        rows_below_first,
        text_row: Some(3),
    })
}

/// The screen as Claude drew it, with the question's heading line swapped for `heading`.
fn with_heading(heading: &str) -> String {
    TAB4.replace(DISCOUNT, heading)
}

#[test]
fn step_four_of_four_is_placed_on_its_tab() {
    assert_eq!(
        classify_claude_question_screen(&four_questions(), TAB4),
        placed(3, 0)
    );
}

#[test]
fn a_heading_drawn_with_other_punctuation_still_finds_its_tab() {
    // A console that draws the dash and the curly apostrophe as plain ASCII, or as "?".
    for heading in [
        "Which discount applies - the customer's code or none?",
        "Which discount applies ? the customer?s code or none?",
        "Which discount applies -- the customer`s code or none?",
    ] {
        assert_eq!(
            classify_claude_question_screen(&four_questions(), &with_heading(heading)),
            placed(3, 0),
            "{heading}"
        );
    }
}

#[test]
fn a_heading_that_matches_no_question_is_placed_by_its_option_rows() {
    let screen = with_heading("Wh\u{fffd}\u{fffd}h d\u{fffd}scount appl\u{fffd}es");
    assert_eq!(
        classify_claude_question_screen(&four_questions(), &screen),
        placed(3, 0)
    );
}

#[test]
fn rows_shared_by_two_questions_do_not_guess_the_tab() {
    let questions = vec![
        question("Ship it today?", &["None", "WELCOME10", "LOYAL20"]),
        question(DISCOUNT, &["None", "WELCOME10", "LOYAL20"]),
    ];
    let screen = with_heading("\u{fffd}\u{fffd}\u{fffd}");
    assert_eq!(
        classify_claude_question_screen(&questions, &screen),
        ClaudeQuestionScreen::Unplaced { cancellable: true }
    );
}

#[test]
fn a_frame_without_its_highlight_is_unplaced_and_cancellable() {
    // Caught mid-repaint: the rows are drawn, the `❯` is not yet.
    let screen = TAB4.replace("❯ 1. None", "  1. None");
    assert_eq!(
        classify_claude_question_screen(&four_questions(), &screen),
        ClaudeQuestionScreen::Unplaced { cancellable: true }
    );
}

#[test]
fn the_question_under_the_task_list_is_at_the_bottom() {
    assert!(claude_question_dialog_at_bottom(&four_questions(), TAB4));
}

#[test]
fn a_composer_under_the_question_is_not_cancelled() {
    let rule = "─".repeat(120);
    let screen = format!(
        "{TAB4}\n⏺ User declined to answer questions\n\n{rule}\n❯ \n{rule}\n  Haiku 5.5 | medium\n"
    );
    assert!(!claude_question_dialog_at_bottom(
        &four_questions(),
        &screen
    ));
}

#[test]
fn another_dialog_under_the_question_is_not_cancelled() {
    // A permission prompt took the bottom of the screen: Escape there would deny it.
    let screen = TAB4.replace(
        "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        "Enter to select · Tab/Arrow keys to navigate · Esc to cancel\n\n Bash command\n\n   rm -rf build\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel · Tab to amend",
    );
    assert!(!claude_question_dialog_at_bottom(
        &four_questions(),
        &screen
    ));
}

#[test]
fn a_screen_without_the_question_is_gone() {
    let other = vec![question("Which colour?", &["Red", "Blue"])];
    assert_eq!(
        classify_claude_question_screen(&other, TAB4),
        ClaudeQuestionScreen::Gone
    );
}

/// Seen live on Claude Code 2.1.311: the question's text starts with "🎟\u{fe0f}", and the
/// terminal draws the emoji without its variation selector.
#[test]
fn an_emoji_drawn_without_its_variation_selector_still_finds_its_tab() {
    let mut questions = four_questions();
    questions[3].question = format!("🎟\u{fe0f} {DISCOUNT}");
    let screen = with_heading(&format!("🎟 {DISCOUNT}"));
    assert_eq!(
        classify_claude_question_screen(&questions, &screen),
        placed(3, 0)
    );
}
