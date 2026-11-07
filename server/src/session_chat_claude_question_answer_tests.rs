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

/// Real Claude Code 2.1.318 captures of an AskUserQuestion whose options all carry a `preview`
/// (GitHub #479): options in a ~30-column list on the left with wrapped labels, the boxed preview
/// on the same lines to their right, "Notes: press n to add notes" and "n to add notes" in the
/// hints, Claude's task list under it.
const PREVIEW: &str = include_str!("../tests/fixtures/claude-question-preview.txt");
/// The same picker in 24 rows with the second option highlighted: the preview is clipped.
const PREVIEW_CLIPPED: &str = include_str!("../tests/fixtures/claude-question-preview-clipped.txt");
/// The same picker at 110 columns, second option highlighted, beside Claude's "files changed"
/// side panel (the panel column is taken from claude-question-side-panel.txt).
const PREVIEW_SIDE_PANEL: &str =
    include_str!("../tests/fixtures/claude-question-preview-side-panel.txt");
/// The reporter's screen: a long question behind Claude's `│` gutter, the session-name rule under
/// the hints, seven tasks, at 200x36.
const PREVIEW_LONG: &str = include_str!("../tests/fixtures/claude-question-preview-long.txt");
/// The same question in the reporter's geometry, 24 rows beside the side panel (78 columns left
/// for Claude): the "☐ Card style" chip and the question's start are scrolled off the top.
const PREVIEW_SCROLLED: &str =
    include_str!("../tests/fixtures/claude-question-preview-scrolled-side-panel.txt");
const LONG_QUESTION: &str = "Which card style should the hero section of the landing page use for the headline, the subtext and the call to action button?";

fn preview_question(text: &str) -> Vec<SessionChatQuestion> {
    let mut question = question(
        text,
        &[
            "Light glass card (Recommended)",
            "Text on image, shadow only",
            "Dark solid band",
        ],
    );
    question.header = Some("Card style".to_string());
    question.tool_name = Some("AskUserQuestion".to_string());
    question.preview_layout = true;
    for option in &mut question.options {
        option.preview = Some("+------+\n| card |\n+------+".to_string());
    }
    vec![question]
}

fn preview_placed(rows_below_first: usize) -> ClaudeQuestionScreen {
    ClaudeQuestionScreen::Placed(ClaudeQuestionSelectorPosition {
        tab: 0,
        rows_below_first,
        text_row: None,
    })
}

#[test]
fn a_preview_question_is_placed_where_its_highlight_stands() {
    let short = preview_question("Which card style should the hero use?");
    let long = preview_question(LONG_QUESTION);
    for (questions, screen, row) in [
        (&short, PREVIEW, 0),
        (&short, PREVIEW_CLIPPED, 1),
        (&short, PREVIEW_SIDE_PANEL, 1),
        (&long, PREVIEW_LONG, 0),
        (&long, PREVIEW_SCROLLED, 0),
    ] {
        assert_eq!(
            classify_claude_question_screen(questions, screen),
            preview_placed(row),
            "{screen}"
        );
    }
}

/// GitHub #479: this screen read as "no question on screen", so the chat's answer was typed as a
/// message and waited for an input box that cannot show while the question is open.
#[test]
fn a_scrolled_preview_question_is_answered_with_its_option_keys() {
    let questions = preview_question(LONG_QUESTION);
    let ClaudeQuestionScreen::Placed(position) =
        classify_claude_question_screen(&questions, PREVIEW_SCROLLED)
    else {
        panic!("the preview question was not placed");
    };
    let selections = vec![crate::session_chat::SessionChatQuestionSelection {
        indices: vec![1],
        ..Default::default()
    }];
    let mut groups =
        crate::session_chat_question_liveness::claude_question_selector_reset_keys(position);
    groups.extend(crate::session_chat_send::build_claude_ask_answer_keys(
        &questions,
        &selections,
    ));
    assert_eq!(groups.len(), 3, "{groups:?}");
    assert!(matches!(
        groups[0],
        crate::session_chat_send::AskAnswerKeyGroup::PrepareClaudeQuestion(_)
    ));
    assert_eq!(
        groups[1..],
        [
            crate::session_chat_send::AskAnswerKeyGroup::Raw("2".to_string()),
            crate::session_chat_send::AskAnswerKeyGroup::Raw("\r".to_string()),
        ]
    );
    // The step that runs first reads the tab off the same screen and finds nothing to move.
    let tab = crate::session_chat_claude_question_prep::read_claude_question_tab(
        &questions,
        PREVIEW_SCROLLED,
    )
    .expect("tab read");
    let crate::session_chat_send::AskAnswerKeyGroup::PrepareClaudeQuestion(prep) = &groups[0]
    else {
        unreachable!()
    };
    assert_eq!(
        crate::session_chat_claude_question_prep::next_claude_question_prep_keys(prep, &tab),
        Ok(None)
    );
}

#[test]
fn a_preview_question_with_wrapped_labels_is_on_screen_and_at_the_bottom() {
    let short = preview_question("Which card style should the hero use?");
    let long = preview_question(LONG_QUESTION);
    for (questions, screen) in [
        (&short, PREVIEW),
        (&short, PREVIEW_CLIPPED),
        (&short, PREVIEW_SIDE_PANEL),
        (&long, PREVIEW_LONG),
        (&long, PREVIEW_SCROLLED),
    ] {
        assert!(
            claude_question_selector_on_screen(questions, screen),
            "{screen}"
        );
        assert!(
            claude_question_dialog_at_bottom(questions, screen),
            "{screen}"
        );
        // Caught mid-repaint without its highlight: still this question, and Escape closes it.
        let unhighlighted = screen.replace("❯ ", "  ");
        assert_eq!(
            classify_claude_question_screen(questions, &unhighlighted),
            ClaudeQuestionScreen::Unplaced { cancellable: true },
            "{unhighlighted}"
        );
    }
}

#[test]
fn a_preview_question_with_other_labels_is_gone() {
    let mut questions = preview_question(LONG_QUESTION);
    questions[0].options[0].label = "Heavy glass card".to_string();
    questions[0].question = "Which font should the footer use?".to_string();
    assert_eq!(
        classify_claude_question_screen(&questions, PREVIEW_SCROLLED),
        ClaudeQuestionScreen::Gone
    );
}

#[test]
fn an_open_question_is_told_from_an_input_box_under_it() {
    use crate::session_chat_question_liveness::claude_question_picker_on_screen;
    for screen in [
        TAB4,
        PREVIEW,
        PREVIEW_CLIPPED,
        PREVIEW_SIDE_PANEL,
        PREVIEW_LONG,
        PREVIEW_SCROLLED,
    ] {
        assert!(claude_question_picker_on_screen(screen), "{screen}");
    }
    // Answered or declined: the input box is back under the old dialog.
    let rule = "─".repeat(120);
    let answered = format!(
        "{PREVIEW_LONG}\n⏺ User answered Claude's questions\n\n{rule}\n❯ \n{rule}\n  Haiku 5.5 | medium\n"
    );
    assert!(!claude_question_picker_on_screen(&answered));
    // A permission prompt took the bottom of the screen.
    let permission = TAB4.replace(
        "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        "Enter to select · Tab/Arrow keys to navigate · Esc to cancel\n\n Bash command\n\n   rm -rf build\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel · Tab to amend",
    );
    assert!(!claude_question_picker_on_screen(&permission));
    // Another picker with the same hints but no "Chat about this" row is not a question.
    let other_picker = "  Select model\n\n❯ 1. Opus\n  2. Sonnet\n\nEnter to select · ↑/↓ to navigate · Esc to cancel\n";
    assert!(!claude_question_picker_on_screen(other_picker));
}
