//! Codex's startup "Hooks need review" dialog (GitHub #458), read from real captures of one live
//! dialog (Codex 0.167 at 200x50): the VT capture the chat card is read from and the plain capture
//! an answer rechecks. The reporter's card showed this dialog's exact text and refused every pick
//! with "Codex's dialog changed".

use serde_json::{json, Map, Value};

use crate::session_chat_codex_dialog::{answer_codex_dialog, detect_codex_dialog};
use crate::session_chat_send::{
    test_terminal::{self, TestTerminal},
    SessionChatSendTarget,
};

const VT: &str = include_str!("fixtures/codex-hooks-review.vt");
const PLAIN: &str = include_str!("fixtures/codex-hooks-review.txt");

/// The same dialog as Codex renders it 40 columns wide (codex-rs/tui/src/snapshots/
/// codex_tui__startup_hooks_review__tests__startup_hooks_review_prompt.snap), with its highlight.
const NARROW: &str = "
  Hooks need review
  2 hooks are new or changed.
  Hooks can run outside the sandbox
  after you trust them.


› 1. Review hooks
  2. Trust all and continue
  3. Continue without trusting (hooks
     won't run)

  enter confirm · esc skip
";

/// Codex once the dialog is answered: its welcome and an empty composer.
const COMPOSER: &str = "
  >_ OpenAI Codex (v0.167.0)
     ~/proj

  To get started, describe a task or try one of these commands:


› Ask Codex to do anything

  gpt-5.5 high · ~/proj
";

/// The highlight on "Trust all and continue", as a `2` typed into the dialog leaves it.
fn second_row_highlighted(screen: &str) -> String {
    screen
        .replace("› 1. Review hooks", "  1. Review hooks")
        .replace("  2. Trust all", "› 2. Trust all")
}

/// Windows line endings, as a capture taken through ConPTY might carry.
fn crlf(screen: &str) -> String {
    screen.replace('\n', "\r\n")
}

#[test]
fn the_card_shows_codex_hooks_review_as_the_reporter_saw_it() {
    let notice =
        crate::session_chat_notice::classify_session_chat_terminal_notice(Some("codex"), VT)
            .expect("the card");
    assert_eq!(notice.kind, "codexInputBlocked");
    assert_eq!(notice.title, "Hooks need review");
    assert_eq!(
        notice.detail.as_deref(),
        Some("2 hooks are new or changed.\n  Hooks can run outside the sandbox after you trust them.")
    );
    let choices: Vec<(&str, bool)> = notice
        .choices
        .iter()
        .map(|choice| (choice.label.as_str(), choice.selected))
        .collect();
    assert_eq!(
        choices,
        [
            ("Review hooks", true),
            ("Trust all and continue", false),
            ("Continue without trusting (hooks won't run)", false),
        ]
    );
    assert!(notice.dialog.is_some(), "a pick answers Codex's own dialog");
}

#[test]
fn the_card_and_the_answer_see_the_same_dialog() {
    let card = detect_codex_dialog(VT).expect("card dialog");
    for (name, screen) in [
        ("plain", PLAIN.to_string()),
        ("plain, CRLF", crlf(PLAIN)),
        ("VT, CRLF", crlf(VT)),
        ("more blank rows above", format!("\n\n\n{PLAIN}")),
        ("reflowed to 40 columns", NARROW.to_string()),
        ("highlight on row 2", second_row_highlighted(PLAIN)),
    ] {
        let answer = detect_codex_dialog(&screen).unwrap_or_else(|| panic!("{name}: no dialog"));
        assert_eq!(answer.id, card.id, "{name}");
        assert_eq!(answer.title, "Hooks need review", "{name}");
        assert_eq!(answer.rows.len(), 3, "{name}");
    }
}

#[test]
fn a_dialog_that_says_something_else_is_another_dialog() {
    let card = detect_codex_dialog(PLAIN).expect("card dialog");
    let failed = PLAIN.replace(
        "after you trust them.\n",
        "after you trust them.\n  Failed to trust hooks: config/batchWrite failed in TUI\n",
    );
    let answer = detect_codex_dialog(&failed).expect("dialog with its error");
    assert_ne!(answer.id, card.id);
    let three = PLAIN.replace("2 hooks are new", "3 hooks are new");
    assert_ne!(detect_codex_dialog(&three).expect("dialog").id, card.id);
}

fn session(name: &str, screen: String) -> SessionChatSendTarget {
    let zmx_name = format!("codex-hooks-review-{name}");
    test_terminal::open(
        &zmx_name,
        TestTerminal {
            screen: Some(Some(screen)),
            screen_after_write: Some(COMPOSER.to_string()),
            ..TestTerminal::default()
        },
    );
    SessionChatSendTarget {
        project_id: "codex-hooks-review".to_string(),
        session_id: zmx_name.clone(),
        zmx_name,
        session: json!({ "agentId": "codex" }),
    }
}

fn pick(index: u64) -> Map<String, Value> {
    let card = detect_codex_dialog(VT).expect("card dialog");
    json!({ "kind": "terminalDialog", "choiceIndex": index, "dialogId": card.id })
        .as_object()
        .unwrap()
        .clone()
}

/// Every row picked on the card reaches Codex as arrows from the highlight Codex shows when the
/// answer goes, then Enter, and the dialog closing is the answer's success. The answer reads the
/// plain capture, in Codex's layout of the moment.
#[tokio::test]
async fn every_pick_on_the_card_answers_codex() {
    let temp = tempfile::tempdir().expect("temp dir");
    let state = crate::server::tests::helpers::test_app_state(crate::paths::get_gxserver_paths(
        Some(temp.path().to_path_buf()),
    ));
    let cases = [
        ("review", PLAIN.to_string(), 0, "\r"),
        ("trust-all", PLAIN.to_string(), 1, "\x1b[B\r"),
        ("skip", crlf(PLAIN), 2, "\x1b[B\x1b[B\r"),
        ("skip-narrow", NARROW.to_string(), 2, "\x1b[B\x1b[B\r"),
        ("skip-moved", second_row_highlighted(PLAIN), 2, "\x1b[B\r"),
        ("review-moved", second_row_highlighted(PLAIN), 0, "\x1b[A\r"),
    ];
    for (name, screen, index, keys) in cases {
        let target = session(name, screen);
        answer_codex_dialog(&state, &target, &pick(index))
            .await
            .unwrap_or_else(|error| panic!("{name}: refused: {}", error.message));
        let terminal = test_terminal::terminal(&target.zmx_name);
        assert_eq!(terminal.writes.concat(), keys, "{name}");
    }
}

/// A dialog that is really gone (Codex moved on before the pick arrived) still refuses the pick:
/// nothing is typed into whatever Codex shows instead.
#[tokio::test]
async fn a_pick_after_the_dialog_left_types_nothing() {
    let temp = tempfile::tempdir().expect("temp dir");
    let state = crate::server::tests::helpers::test_app_state(crate::paths::get_gxserver_paths(
        Some(temp.path().to_path_buf()),
    ));
    let target = session("gone", COMPOSER.to_string());
    let error = answer_codex_dialog(&state, &target, &pick(2))
        .await
        .expect_err("refused");
    assert_eq!(
        error.message,
        "Codex's dialog changed. Review the current choices and try again."
    );
    assert!(test_terminal::terminal(&target.zmx_name).writes.is_empty());
}
