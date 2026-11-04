// ---------------------------------------------------------------------------
// Inline tests: byte builders and keystroke builders are pure and locked here
// ---------------------------------------------------------------------------

use super::*;
use crate::session_chat::SessionChatQuestionOption;

fn question(text: &str, multi_select: bool, options: &[&str]) -> SessionChatQuestion {
    SessionChatQuestion {
        question: text.to_string(),
        header: None,
        multi_select,
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

fn selection(indices: &[usize], other: Option<&str>) -> SessionChatQuestionSelection {
    SessionChatQuestionSelection {
        indices: indices.to_vec(),
        other: other.map(str::to_string),
    }
}

fn raw(value: &str) -> AskAnswerKeyGroup {
    AskAnswerKeyGroup::Raw(value.to_string())
}

fn text(value: &str) -> AskAnswerKeyGroup {
    AskAnswerKeyGroup::Text(value.to_string())
}

#[test]
fn clear_burst_follows_the_2n_minus_1_law() {
    // 1 line → 1 repetition.
    assert_eq!(build_agent_tui_clear_input(1), "\u{15}\u{b}");
    // 3 lines → 5 repetitions of each.
    assert_eq!(
        build_agent_tui_clear_input(3),
        format!("{}{}", "\u{15}".repeat(5), "\u{b}".repeat(5))
    );
    // Cap at 40 lines → 79 repetitions; 0 clamps up to 1.
    assert_eq!(
        build_agent_tui_clear_input(1000),
        format!("{}{}", "\u{15}".repeat(79), "\u{b}".repeat(79))
    );
    assert_eq!(build_agent_tui_clear_input(0), "\u{15}\u{b}");
    // For-text adds the 8-line slack: 1 line + 8 → 17 repetitions.
    assert_eq!(
        build_agent_tui_clear_input_for_text("hello"),
        format!("{}{}", "\u{15}".repeat(17), "\u{b}".repeat(17))
    );
    assert_eq!(count_agent_tui_input_lines("a\r\nb\rc\nd"), 4);
    assert_eq!(count_agent_tui_input_lines("plain"), 1);
    assert_eq!(count_agent_tui_input_lines("trailing\n"), 2);
}

#[test]
fn paste_sanitize_and_normalize_match_spec() {
    assert_eq!(
        sanitize_bracketed_paste_text("a\u{1b}[201~b"),
        "a\u{241b}[201~b"
    );
    assert_eq!(
        normalize_terminal_paste_line_endings("a\r\nb\nc"),
        "a\rb\rc"
    );
    // Lone CR is untouched.
    assert_eq!(normalize_terminal_paste_line_endings("a\rb"), "a\rb");
    // Multiline → framed; single line → sanitized unframed.
    assert_eq!(
        build_session_chat_paste_bytes("one\ntwo"),
        "\u{1b}[200~one\rtwo\u{1b}[201~"
    );
    assert_eq!(build_session_chat_paste_bytes("solo"), "solo");
    // Long single line → framed, so it arrives as one paste; long commands stay typed.
    let long_line = "word ".repeat(200);
    assert_eq!(
        build_session_chat_paste_bytes(&long_line),
        format!("\u{1b}[200~{long_line}\u{1b}[201~")
    );
    let long_command = format!("/rename {long_line}");
    assert_eq!(build_session_chat_paste_bytes(&long_command), long_command);
    // 300 form feeds sanitize to 900 bytes, so the line is framed.
    let control_heavy = "\u{c}".repeat(300);
    assert!(build_session_chat_paste_bytes(&control_heavy).starts_with("\u{1b}[200~"));
    assert!(is_multiline_draft("text\n"));
    assert!(!is_multiline_draft("text"));
    assert_eq!(
        build_session_chat_image_paste_bytes("/tmp/a.png"),
        "\u{1b}[200~/tmp/a.png\u{1b}[201~"
    );
}

#[test]
fn message_steps_verify_the_paste_before_the_separate_enter() {
    let steps = build_session_chat_message_steps(Some("claude"), "hi", &[], false);
    assert_eq!(
        steps,
        vec![
            SessionChatSendStep::WaitForComposer {
                agent: Some("claude".to_string()),
                settle_ms: 0,
                timeout_ms: 6_000,
            },
            SessionChatSendStep::ClearComposer {
                agent: "claude".to_string()
            },
            SessionChatSendStep::Write("hi".to_string()),
            SessionChatSendStep::VerifyPasteLanded {
                text: "hi".to_string(),
                settle_ms: 500,
                timeout_ms: 2_000,
            },
            SessionChatSendStep::Write("\r".to_string()),
            SessionChatSendStep::VerifySubmitted {
                agent: "claude".to_string(),
                text: "hi".to_string(),
                submit: "\r".to_string(),
            },
        ]
    );
    let with_images = build_session_chat_message_steps(
        Some("claude"),
        "what is this",
        &["/tmp/ghostex-paste-1.png".to_string()],
        false,
    );
    assert_eq!(
        with_images,
        vec![
            SessionChatSendStep::WaitForComposer {
                agent: Some("claude".to_string()),
                settle_ms: 0,
                timeout_ms: 6_000,
            },
            SessionChatSendStep::ClearComposer {
                agent: "claude".to_string()
            },
            SessionChatSendStep::Write(
                "\u{1b}[200~/tmp/ghostex-paste-1.png\u{1b}[201~".to_string()
            ),
            SessionChatSendStep::SleepMs(300),
            SessionChatSendStep::Write("what is this".to_string()),
            SessionChatSendStep::VerifyPasteLanded {
                text: "what is this".to_string(),
                settle_ms: 500,
                timeout_ms: 2_000,
            },
            SessionChatSendStep::Write("\r".to_string()),
            SessionChatSendStep::VerifySubmitted {
                agent: "claude".to_string(),
                text: "what is this".to_string(),
                submit: "\r".to_string(),
            },
        ]
    );
    // Images without text: no body write, nothing on screen to verify, so
    // the Enter keeps the original blind settle.
    let images_only =
        build_session_chat_message_steps(Some("claude"), "", &["/tmp/a.png".to_string()], false);
    assert_eq!(
        images_only,
        vec![
            SessionChatSendStep::WaitForComposer {
                agent: Some("claude".to_string()),
                settle_ms: 0,
                timeout_ms: 6_000,
            },
            SessionChatSendStep::ClearComposer {
                agent: "claude".to_string()
            },
            SessionChatSendStep::Write("\u{1b}[200~/tmp/a.png\u{1b}[201~".to_string()),
            SessionChatSendStep::SleepMs(500),
            SessionChatSendStep::Write("\r".to_string()),
        ]
    );
    // The window scales with the payload and is capped.
    assert_eq!(session_chat_verify_timeout_ms(0), 2_000);
    assert_eq!(
        session_chat_verify_timeout_ms(4_600),
        if cfg!(windows) { 9_700 } else { 2_800 }
    );
    assert_eq!(
        session_chat_verify_timeout_ms(1_000_000),
        SESSION_CHAT_VERIFY_MAX_TIMEOUT_MS
    );
}

#[test]
fn paste_needles_survive_composer_rewrapping() {
    // Both ends are sampled, whitespace and box drawing are dropped.
    let needles = session_chat_paste_needles("first line here\n\nmiddle\nlast line here");
    assert_eq!(needles, vec!["firstlinehere", "lastlinehere"]);
    // One logical line yields one needle, capped at 40 characters.
    let long = "a".repeat(80);
    assert_eq!(session_chat_paste_needles(&long), vec!["a".repeat(40)]);
    // A composer that framed, indented and wrapped the text still matches.
    let screen = "╭──────────────╮\n│ > first line │\n│   here       │\n╰──────────────╯";
    let normalized = normalize_session_chat_screen_text(screen);
    assert!(normalized.contains("firstlinehere"));
}

fn claude_prep(
    questions: &[SessionChatQuestion],
    question: usize,
    ticked: Option<&[usize]>,
) -> AskAnswerKeyGroup {
    AskAnswerKeyGroup::PrepareClaudeQuestion(
        crate::session_chat_claude_question_prep::ClaudeQuestionPrep {
            questions: questions.to_vec(),
            question,
            ticked: ticked.map(<[usize]>::to_vec),
        },
    )
}

#[test]
fn claude_single_question_single_select_commits_by_digit() {
    let questions = vec![question("Pick one", false, &["A", "B", "C"])];
    let selections = vec![selection(&[1], None)];
    // Digit selects AND commits; single single-select never ends on the
    // Submit tab, so no trailing Enter.
    assert_eq!(
        build_claude_ask_answer_keys(&questions, &selections),
        vec![claude_prep(&questions, 0, None), raw("2")]
    );
}

#[test]
fn claude_free_text_routes_through_type_something() {
    let questions = vec![question("Pick", false, &["A", "B"])];
    let selections = vec![selection(&[0], Some("also this"))];
    // "Type something" is row options.len()+1 = 3; label + other joined.
    assert_eq!(
        build_claude_ask_answer_keys(&questions, &selections),
        vec![
            claude_prep(&questions, 0, None),
            raw("3"),
            text("A, also this"),
            raw("\r")
        ]
    );
}

#[test]
fn claude_multi_select_toggles_then_advances_then_submits() {
    let questions = vec![question("Pick many", true, &["A", "B", "C"])];
    let selections = vec![selection(&[0, 2], None)];
    // The prep step ticks exactly 1 and 3, then Right to the Submit tab
    // and the final confirmation (single multiSelect question ends on
    // the Submit tab).
    assert_eq!(
        build_claude_ask_answer_keys(&questions, &selections),
        vec![
            claude_prep(&questions, 0, Some(&[0, 2])),
            raw("\u{1b}[C"),
            raw("\r")
        ]
    );
}

#[test]
fn claude_multi_question_steps_past_unanswered_and_confirms() {
    let questions = vec![
        question("First", false, &["A", "B"]),
        question("Second", false, &["X", "Y"]),
    ];
    let selections = vec![
        selection(&[0], None),
        SessionChatQuestionSelection::default(),
    ];
    // Q1 digit auto-advances; Q2 unanswered → Right past it; multi-question
    // ends on the Submit tab → final Enter.
    assert_eq!(
        build_claude_ask_answer_keys(&questions, &selections),
        vec![
            claude_prep(&questions, 0, None),
            raw("1"),
            claude_prep(&questions, 1, None),
            raw("\u{1b}[C"),
            raw("\r")
        ]
    );
}

#[test]
fn codex_digit_commits_and_notes_align_to_their_row() {
    let align = |question: usize, row: Option<usize>| AskAnswerKeyGroup::AlignCodexQuestion {
        question,
        row,
    };
    let questions = vec![question("Pick", false, &["A", "B", "C"])];
    assert_eq!(
        build_codex_ask_answer_keys(&questions, &[selection(&[2], None)]),
        vec![align(0, None), raw("3")]
    );
    // A note without an option goes on the notes row (index 3 of 4 rows).
    assert_eq!(
        build_codex_ask_answer_keys(&questions, &[selection(&[], Some("my note"))]),
        vec![align(0, Some(3)), raw("\t"), text("my note"), raw("\r")]
    );
    assert_eq!(
        build_codex_ask_answer_keys(&questions, &[selection(&[0], Some("why"))]),
        vec![align(0, Some(0)), raw("\t"), text("why"), raw("\r")]
    );
}

#[test]
fn codex_unanswered_rows_are_skipped_and_confirmed() {
    let align = |question: usize| AskAnswerKeyGroup::AlignCodexQuestion {
        question,
        row: None,
    };
    let questions = vec![
        question("First", false, &["A", "B"]),
        question("Second", false, &["X", "Y"]),
    ];
    let selections = vec![
        SessionChatQuestionSelection::default(),
        selection(&[1], None),
    ];
    // Q1 unanswered → DEL + Right (not last); Q2 digit commits; unanswered
    // remains → trailing Enter for the confirmation dialog.
    assert_eq!(
        build_codex_ask_answer_keys(&questions, &selections),
        vec![
            align(0),
            raw("\u{7f}"),
            raw("\u{1b}[C"),
            align(1),
            raw("2"),
            raw("\r")
        ]
    );
    // Unanswered LAST question ends its row with Enter instead of Right.
    let tail_unanswered = vec![
        selection(&[0], None),
        SessionChatQuestionSelection::default(),
    ];
    assert_eq!(
        build_codex_ask_answer_keys(&questions, &tail_unanswered),
        vec![
            align(0),
            raw("1"),
            align(1),
            raw("\u{7f}"),
            raw("\r"),
            raw("\r")
        ]
    );
}

#[test]
fn format_ask_answer_keeps_one_line_per_question() {
    let questions = vec![
        question("First", false, &["A", "B"]),
        question("Second", false, &["X", "Y"]),
        question("Third", false, &["M"]),
    ];
    let selections = vec![
        selection(&[1], None),
        SessionChatQuestionSelection::default(),
        selection(&[0], Some("extra")),
    ];
    assert_eq!(format_ask_answer(&questions, &selections), "B\n\nM, extra");
    assert!(has_ask_answer(&selections));
    assert!(!has_ask_answer(&[SessionChatQuestionSelection::default()]));
}

#[test]
fn key_steps_are_a_single_verbatim_write() {
    assert_eq!(
        build_session_chat_key_steps("shift-tab"),
        Some(vec![SessionChatSendStep::Write("\u{1b}[9;2u".to_string())])
    );
    // No bracketed paste framing, no trailing Enter, no clear burst.
    assert_eq!(build_session_chat_key_steps("shift-tab").unwrap().len(), 1);
    assert_eq!(
        build_session_chat_key_steps("shift-up"),
        Some(vec![SessionChatSendStep::Write("\u{1b}[1;2A".to_string())])
    );
    assert_eq!(
        build_session_chat_key_steps("shift-down"),
        Some(vec![SessionChatSendStep::Write("\u{1b}[1;2B".to_string())])
    );
    assert_eq!(build_session_chat_key_steps("tab"), None);
    assert_eq!(build_session_chat_key_steps(""), None);
}

#[test]
fn ask_answer_steps_space_groups_one_second_apart() {
    let steps = build_ask_answer_steps(&[raw("1"), text("note\nline"), raw("\r")]);
    assert_eq!(
        steps,
        vec![
            SessionChatSendStep::Write("1".to_string()),
            SessionChatSendStep::SleepMs(1_000),
            SessionChatSendStep::Write("\u{1b}[200~note\rline\u{1b}[201~".to_string()),
            SessionChatSendStep::SleepMs(1_000),
            SessionChatSendStep::Write("\r".to_string()),
        ]
    );
    assert!(build_ask_answer_steps(&[]).is_empty());
}

/// Run this test binary from the packaged Web directory so bundled zmx resolves.
/// Set `GHOSTEX_CLAUDE_MODEL_TEST_ZMX` to a dedicated idle Claude TUI and pass `live_zmx_claude_model_changes --ignored --nocapture`.
#[tokio::test]
#[ignore = "requires an explicitly supplied disposable Claude zmx session"]
async fn live_zmx_claude_model_changes() {
    use crate::session_chat_options::{detect_session_chat_selection, SessionChatOptionAgent};
    let name = std::env::var("GHOSTEX_CLAUDE_MODEL_TEST_ZMX")
        .expect("set the dedicated test session name");
    assert!(name.starts_with("ghostex-claude-check-"));
    for model in ["opus", "fable", "haiku", "sonnet", "opus", "sonnet"] {
        let started = std::time::Instant::now();
        let command = format!("/model {model}");
        execute_session_chat_send(
            "claude-model-check",
            &name,
            &name,
            "claude-model-check",
            build_session_chat_message_steps(Some("claude"), &command, &[], false),
        )
        .await
        .expect("deliver model command");
        let delivered_ms = started.elapsed().as_millis();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        loop {
            let screen = capture_session_terminal_text(&name).await.unwrap();
            let selected = detect_session_chat_selection(SessionChatOptionAgent::Claude, &screen);
            if selected
                .and_then(|value| value.model)
                .is_some_and(|value| value.value == model)
            {
                println!(
                    "Claude {model}: delivered {delivered_ms}ms, footer confirmed {}ms",
                    started.elapsed().as_millis()
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Claude did not confirm {model}:\n{screen}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
}

/// A real Claude Code 2.1.311 AskUserQuestion at 200 columns, with the "files changed" panel down
/// the right: the highlight is read and the answer is not refused.
#[test]
fn a_claude_question_beside_the_side_panel_places_its_highlight() {
    let screen = include_str!("../../tests/fixtures/claude-question-side-panel.txt");
    let questions = vec![SessionChatQuestion {
        header: Some("Colour".to_string()),
        ..question("Which colour?", false, &["Red", "Blue"])
    }];
    assert!(
        crate::session_chat_question_liveness::claude_question_selector_on_screen(
            &questions, screen
        )
    );
    let position = crate::session_chat_question_liveness::claude_question_selector_position(
        &questions, screen,
    )
    .expect("highlight placed");
    assert_eq!(position.tab, 0);
    assert_eq!(position.rows_below_first, 0);
}
