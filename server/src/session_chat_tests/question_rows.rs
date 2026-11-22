//! Question lists answered from the chat (`session_chat_question_row_align`). OMP's symbol presets
//! are read on OMP's lists only, so a Cursor or pi label that looks like OMP's ASCII radio or frame
//! bar is still found. The OMP screens follow OMP 18.13.3's ask dialog and custom-answer prompt
//! with the glyphs of its symbol presets (`packages/tui/src/theme/symbols.ts`); they are written
//! out here, not captured.

use crate::session_chat_question_row_align::{
    read_omp_custom_prompt, read_question_list_screen, QuestionListUi, QuestionRowAction,
    QuestionRowTarget,
};

const QUESTION: &str = "Which colour should the test use?";

const OMP_ASCII_LIST: &str = "  Esc Asking test colour question

+- Ask --------------------------------------------+
| Which colour should the test use?                |
+--------------------------------------------------+
| > ( ) Red                                        |
|   ( ) Blue                                       |
|   ( ) Other (type your own)                      |
|                                                  |
+--------------------------------------------------+
| Enter select · n note · Up/Down move · Esc cancel · Option+H hide |
+--------------------------------------------------+
";

const OMP_NERD_LIST: &str = concat!(
    "  \u{f12b7} Asking test colour question\n",
    "\n",
    "╭─ Ask ────────────────────────────────────────────╮\n",
    "│ Which colour should the test use?                │\n",
    "├──────────────────────────────────────────────────┤\n",
    "│ \u{f054} \u{f10c} Red                                       │\n",
    "│   \u{f10c} Blue                                       │\n",
    "│   \u{f10c} Other (type your own)                      │\n",
    "│                                                  │\n",
    "├──────────────────────────────────────────────────┤\n",
    "│ \u{f0311} select · n note · ↑/↓ move · \u{f12b7} cancel │\n",
    "╰──────────────────────────────────────────────────╯\n",
);

const OMP_ASCII_CUSTOM_ANSWER: &str = "+- Custom answer: Which colour should the test use? ------+
|                                                          |
| > Green, typed in the chat                               |
|                                                          |
| Enter or Ctrl+Q submit  Esc cancel  Ctrl+G external editor |
|                                                          |
+----------------------------------------------------------+
";

fn move_to(ui: QuestionListUi, question: &str, labels: &[&str], row: usize) -> QuestionRowTarget {
    QuestionRowTarget {
        ui,
        questions: vec![question.to_string()],
        question: 0,
        labels: labels.iter().map(|label| label.to_string()).collect(),
        row,
        action: QuestionRowAction::Move,
    }
}

#[test]
fn a_pi_label_that_starts_like_an_omp_radio_is_found() {
    let target = move_to(
        QuestionListUi::Pi,
        "Which badge?",
        &["(o) ring", "solid"],
        1,
    );
    let screen = read_question_list_screen(&target, "Which badge?\n→ (o) ring\n  solid\n")
        .expect("the list");
    assert_eq!(screen.highlighted, Some(0));
    assert_eq!(screen.ticked, vec![None; 3]);
}

#[test]
fn an_omp_ascii_label_that_starts_like_its_radio_keeps_it() {
    let target = move_to(
        QuestionListUi::Omp,
        "Which badge?",
        &["(o) ring", "solid"],
        1,
    );
    let screen = read_question_list_screen(
        &target,
        "| Which badge? |\n| > ( ) (o) ring |\n|   (o) solid |\n",
    )
    .expect("the list");
    assert_eq!(screen.highlighted, Some(0));
    assert_eq!(screen.ticked, vec![Some(false), Some(true), None]);
}

#[test]
fn a_cursor_label_that_ends_in_bars_is_found() {
    let target = move_to(
        QuestionListUi::Cursor,
        "Which symbol?",
        &["|bar|", "solid"],
        1,
    );
    let screen = read_question_list_screen(&target, "Which symbol?\n› [ ] |bar|\n  [ ] solid\n")
        .expect("the list");
    assert_eq!(screen.highlighted, Some(0));
    assert_eq!(screen.ticked, vec![Some(false), Some(false), None]);
}

#[test]
fn omp_lists_are_read_with_the_ascii_and_nerd_font_presets() {
    let target = move_to(QuestionListUi::Omp, QUESTION, &["Red", "Blue"], 1);
    for list in [OMP_ASCII_LIST, OMP_NERD_LIST] {
        let screen = read_question_list_screen(&target, list).expect("the list");
        assert_eq!(screen.highlighted, Some(0));
        assert_eq!(screen.ticked, vec![Some(false); 3]);
    }
}

#[test]
fn an_omp_custom_answer_is_read_inside_an_ascii_frame() {
    assert_eq!(read_omp_custom_prompt(OMP_ASCII_CUSTOM_ANSWER), Some(true));
}
