//! OMP 18.13.3's inline tool approval, captured live with the Unicode symbol preset and the `pi` and
//! `box` composer shapes. OMP keeps its composer on screen right below the approval frame and removes
//! the frame once the call is approved or denied. A card's keys must reach OMP as OMP, though its
//! transcript reads as Pi's, only while a screen that takes them is up, and never its tool approval.

use serde_json::{json, Value};

use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, SessionChatComposerState,
};
use crate::session_chat_follower::{
    omp_agent_for_pi_transcript, session_chat_agent_for_session,
    session_chat_display_agent_for_session,
};
use crate::session_chat_notice::{
    classify_session_chat_terminal_notice, SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED,
    SESSION_CHAT_NOTICE_PERMISSION_PROMPT,
};
use crate::session_chat_omp_blocking::{detect_omp_blocking_screen, detect_omp_permission_picker};
use crate::session_chat_resume_picker::answer_terminal_key;
use crate::session_chat_send::{
    test_terminal::{self, TestTerminal},
    SessionChatSendTarget, SESSION_CHAT_INTERRUPT,
};

const PI: &str = include_str!("fixtures/omp-tool-approval-pi.txt");
const BOX: &str = include_str!("fixtures/omp-tool-approval-box.txt");

/// The permission frame from OMP 18.15.0; unrelated transcript and composer rows are omitted.
const OMP_APPROVAL: &str = "\
╭─ Allow tool: bash ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Command: echo cap-b                                                                                                                                                                                  │
│ ❯ Approve                                                                                                                                                                                            │
│   Deny                                                                                                                                                                                               │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ↑/↓ choose · ⏎ select · ⌃⏎/⌃Q approve · ⎋ deny · ⌃O details · type to deny and steer                                                                                                                 │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯";

const APPROVAL: &str = "OMP is waiting for tool approval";

#[test]
fn an_approval_right_above_the_composer_is_live() {
    for screen in [PI, BOX] {
        assert_eq!(
            detect_omp_blocking_screen(screen).map(|blocking| blocking.title),
            Some(APPROVAL)
        );
    }
}

/// `screen`'s approval redrawn as an MCP tool's, whose dialog shows its whole request as JSON under
/// `Arguments:`. A bash dialog's `Command:` shows no unique request (OMP pads the frame, so a
/// command's trailing spaces never show), so OMP's rows and the chat's answers are an MCP tool's.
pub(crate) fn as_mcp(screen: &str, command: &str) -> String {
    screen
        .replace("Allow tool: bash", "Allow tool: mcp__files__read")
        .replace(
            &format!("Command: {command}"),
            &format!("Origin: MCP server tool │\n│ Arguments: │\n│ {{\"path\": \"{command}\"}}"),
        )
}

/// The command the live `pi` and `box` captures ask approval for.
pub(crate) const CAPTURED: &str = "echo ghostex-approval-test";

/// An open approval that shows its request is a permission card with OMP's own rows, as Claude
/// Code's, Antigravity's, Copilot's and Grok's are, and it holds the chat's sends.
#[test]
fn the_open_approval_is_a_permission_card_with_omps_rows() {
    for screen in [as_mcp(PI, CAPTURED), as_mcp(BOX, CAPTURED)] {
        let notice = classify_session_chat_terminal_notice(Some("omp"), &screen).expect("notice");
        assert_eq!(notice.kind, SESSION_CHAT_NOTICE_PERMISSION_PROMPT);
        assert_eq!(notice.title, "Allow this?");
        let rows: Vec<(&str, bool)> = notice
            .choices
            .iter()
            .map(|choice| (choice.label.as_str(), choice.selected))
            .collect();
        assert_eq!(rows, [("Approve", true), ("Deny", false)]);
        assert!(notice.blocks_input());
        // A send meets the chat's "Answer it in chat before sending" refusal (send.rs).
        assert!(notice.is_answerable());
        assert!(!notice.passes_keys_through());
        let readiness = detect_session_chat_composer_readiness(Some("omp"), &screen, Some(&notice));
        assert_ne!(readiness.state, SessionChatComposerState::Ready);
    }
}

/// A bash approval, as captured live, is the terminal card: no rows and no answers from the chat,
/// because the frame cannot show whether its command ends in spaces (CDXC:SessionChat 2026-11-26
/// in session_chat_omp_blocking.rs). It still holds the chat's sends.
#[test]
fn a_bash_approval_is_the_terminal_card_and_holds_sends() {
    for screen in [PI, BOX, OMP_APPROVAL] {
        let notice = classify_session_chat_terminal_notice(Some("omp"), screen).expect("notice");
        assert_eq!(notice.kind, SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED);
        assert_eq!(notice.title, APPROVAL);
        assert!(notice.choices.is_empty());
        assert!(!notice.is_answerable());
        assert!(notice.blocks_input());
        assert!(detect_omp_permission_picker(screen).is_none());
        let readiness = detect_session_chat_composer_readiness(Some("omp"), screen, Some(&notice));
        assert_ne!(readiness.state, SessionChatComposerState::Ready);
    }
}

#[test]
fn omp_approvals_for_different_requests_are_different_notices() {
    for screen in [
        as_mcp(PI, CAPTURED),
        as_mcp(BOX, CAPTURED),
        as_mcp(OMP_APPROVAL, "echo cap-b"),
    ] {
        let mut first =
            classify_session_chat_terminal_notice(Some("omp"), &screen).expect("notice");
        first.detected_at = "first".to_string();
        let next_screen = screen
            .replace(CAPTURED, "echo another-command")
            .replace("echo cap-b", "echo another-command");
        let mut next =
            classify_session_chat_terminal_notice(Some("omp"), &next_screen).expect("next notice");
        assert!(!next.same_notice(Some(&first)));
        next.detected_at = "next".to_string();
        next.carry_forward_detected_at(Some(&first));
        assert_eq!(next.detected_at, "next");
    }
}

#[test]
fn omp_approval_with_cut_arguments_is_a_terminal_card() {
    // OMP 18.15.0 cuts a long line at the box edge instead of wrapping it. Cut arguments can stand
    // for several requests, so the dialog has no rows on the chat's card: it is the terminal card
    // (CDXC:SessionChat 2026-11-25 in session_chat_omp_blocking.rs `omp_shown_request`).
    let shown = "dies-ist-ein-sehr-langer-pfad-der-im-dialog-umbrechen-soll-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa…";
    let truncated = as_mcp(OMP_APPROVAL, "echo cap-b").replace("echo cap-b", shown);
    let notice =
        classify_session_chat_terminal_notice(Some("omp"), &truncated).expect("truncated notice");
    assert_eq!(notice.kind, SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED);
    assert!(notice.choices.is_empty());
}

#[test]
fn omp_approval_with_a_command_over_several_lines_is_a_terminal_card() {
    // A multi-line request in the captured frame shape, not a claimed OMP wrap capture: which
    // spaces the lines dropped cannot be read back, so the dialog has no rows on the chat's card.
    let screen = OMP_APPROVAL
        .replace("echo cap-b", "printf 'one  two'")
        .replace(
            "\n│ ❯ Approve",
            "\n│   printf 'three' │\n│ │               │\n│ ❯ Approve",
        );
    let notice = classify_session_chat_terminal_notice(Some("omp"), &screen).expect("notice");
    assert_eq!(notice.kind, SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED);
    assert!(notice.choices.is_empty());
}

/// The grant's folder and OMP's provider safety checks below the arguments are not part of the
/// request: the dialog still shows its request and keeps its rows.
#[test]
fn omp_approval_excludes_sections_after_the_arguments() {
    for appendix in [
        "Provider safety checks: │\n│ Network destination needs review",
        "Folder: workspace — 2 entries │\n│   - example.txt",
    ] {
        let screen = as_mcp(OMP_APPROVAL, "echo cap-b")
            .replace("\n│ ❯ Approve", &format!("\n│ {appendix} │\n│ ❯ Approve"));
        let notice = classify_session_chat_terminal_notice(Some("omp"), &screen).expect("notice");
        assert_eq!(notice.kind, SESSION_CHAT_NOTICE_PERMISSION_PROMPT);
        let dialog =
            crate::session_chat_omp_blocking::read_omp_permission_dialog(&screen).expect("dialog");
        assert_eq!(
            dialog.shown,
            Some(
                crate::session_chat_omp_blocking::OmpShownRequest::Arguments(
                    json!({ "path": "echo cap-b" })
                )
            )
        );
    }
}

#[test]
fn omp_approval_without_a_command_does_not_invent_one() {
    // A tool with another visible argument, using the captured permission frame shape.
    let screen = OMP_APPROVAL.replace("Command: echo cap-b", "Path: example.txt");
    let notice = classify_session_chat_terminal_notice(Some("omp"), &screen).expect("notice");
    assert_eq!(notice.approval_command, None);
    assert!(notice.to_value().get("approvalCommand").is_none());
}

#[test]
fn the_post_answer_agent_reads_omps_next_approval_not_its_transcript_format() {
    let next_approval = as_mcp(OMP_APPROVAL, "echo cap-b");
    let session = json!({ "agentId": "omp" });
    let transcript = session_chat_agent_for_session(&session).expect("transcript agent");
    assert_eq!(transcript, "pi");
    assert!(classify_session_chat_terminal_notice(Some(&transcript), &next_approval).is_none());
    let terminal = session_chat_display_agent_for_session(&session).expect("terminal agent");
    assert_eq!(terminal, "omp");
    let notice = classify_session_chat_terminal_notice(Some(&terminal), &next_approval)
        .expect("next approval");
    assert_eq!(notice.kind, SESSION_CHAT_NOTICE_PERMISSION_PROMPT);
}

/// A row is picked by walking OMP's highlight to it and pressing Enter (its rows have no numbers);
/// the chat's Allow on the stored approval card picks the Approve row the same way.
#[test]
fn a_row_is_answered_by_walking_omps_highlight() {
    for screen in [as_mcp(PI, CAPTURED), as_mcp(BOX, CAPTURED)] {
        let picker = detect_omp_permission_picker(&screen).expect("picker");
        assert_eq!(picker.answer_key(0).as_deref(), Some("\r"));
        assert_eq!(picker.answer_key(1).as_deref(), Some("\u{1b}[1;1B\r"));
        assert_eq!(picker.answer_key(2), None);
    }
    assert!(detect_omp_permission_picker(&as_mcp(&retired_approval(), CAPTURED)).is_none());
}

/// The live `pi` capture with one line between the approval frame and the composer, the way a
/// frame left in scrollback is followed by whatever OMP printed after it.
fn retired_approval() -> String {
    let mut lines: Vec<&str> = PI.lines().collect();
    let bottom = lines
        .iter()
        .position(|line| line.trim_start().starts_with('\u{2570}'))
        .expect("approval frame bottom");
    lines.insert(bottom + 1, " Denied: echo ghostex-approval-test");
    lines.join("\n")
}

#[test]
fn an_approval_with_any_line_below_it_stays_retired() {
    assert_eq!(detect_omp_blocking_screen(&retired_approval()), None);
}

#[test]
fn the_card_keys_of_an_omp_session_go_to_omp() {
    // The agent the `terminalKey` answer path resolves: the transcript agent, folded back to OMP.
    let terminal_key_agent = |agent_id: &str| {
        let session = serde_json::json!({ "agentId": agent_id });
        let transcript = session_chat_agent_for_session(&session).expect("a transcript agent");
        (
            transcript.clone(),
            omp_agent_for_pi_transcript(&session, transcript),
        )
    };
    assert_eq!(
        terminal_key_agent("omp"),
        ("pi".to_string(), "omp".to_string())
    );
    assert_eq!(
        terminal_key_agent("pi"),
        ("pi".to_string(), "pi".to_string())
    );
}

/// The same approvals after the user typed "N" on the card: OMP's `approval-dialog.ts` hands the key
/// to the composer below, moves the highlight to "Deny and send message" and swaps the footer for
/// "⏎ deny and send · ⌃⏎/⌃Q approve · ⎋ deny · ⌃O details".
const DRAFT_PI: &str = include_str!("fixtures/omp-tool-approval-draft-pi.txt");
const DRAFT_BOX: &str = include_str!("fixtures/omp-tool-approval-draft-box.txt");

/// After a key typed in OMP's terminal the dialog is "Deny and send message", whose Enter sends that
/// message: the chat shows it as the terminal card, not as rows. The card is still marked as one
/// that passes keys on, so chat cores that hide a hook card's Allow and Deny only beside such a card
/// keep hiding them; its keys are refused (`no_card_key_reaches_omps_tool_approval`).
#[test]
fn an_approval_with_a_typed_message_becomes_the_terminal_card() {
    for drafting in [DRAFT_PI, DRAFT_BOX] {
        assert!(detect_omp_permission_picker(drafting).is_none());
        assert_eq!(
            detect_omp_blocking_screen(drafting).map(|blocking| blocking.title),
            Some(APPROVAL)
        );
        let notice = classify_session_chat_terminal_notice(Some("omp"), drafting).expect("notice");
        assert_eq!(notice.kind, SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED);
        assert_eq!(notice.title, APPROVAL);
        assert!(notice.blocks_input());
        assert!(notice.passes_keys_through());
        let readiness =
            detect_session_chat_composer_readiness(Some("omp"), drafting, Some(&notice));
        assert_ne!(readiness.state, SessionChatComposerState::Ready);
    }
}

/// A session whose terminal is a test stand-in, answered through the real `terminalKey` route:
/// the answer function, the session's send queue and its worker, which reads the screen and writes.
struct CardSession {
    target: SessionChatSendTarget,
    agent: String,
}

impl CardSession {
    fn new(name: &str, agent: &str, screen: Option<&str>) -> Self {
        let zmx_name = format!("omp-tool-approval-{name}");
        test_terminal::open(
            &zmx_name,
            TestTerminal {
                screen: Some(screen.map(str::to_string)),
                ..TestTerminal::default()
            },
        );
        Self {
            target: SessionChatSendTarget {
                project_id: "omp-tool-approval".to_string(),
                session_id: zmx_name.clone(),
                zmx_name,
                session: json!({ "agentId": agent }),
            },
            agent: agent.to_string(),
        }
    }

    async fn answer(&self, answer: Value) -> Result<Value, crate::domain::DomainStateError> {
        answer_terminal_key(&self.target, &self.agent, answer.as_object().unwrap()).await
    }
}

fn typed(text: &str) -> Value {
    json!({ "kind": "terminalKey", "text": text })
}

fn key(key: &str) -> Value {
    json!({ "kind": "terminalKey", "key": key })
}

/// OMP's tool approval is answered in the terminal, with or without a message typed into it, so no
/// key from its card reaches OMP: Enter would approve, or deny and send the message, and a letter
/// would start or continue that message (CDXC:SessionChat 2026-11-26 DECISION above
/// `answer_terminal_key`). Every key is refused as answered in the terminal, after the screen was
/// read once, and nothing is written. An OMP session whose transcript agent is Pi's answers here as
/// OMP too (`the_card_keys_of_an_omp_session_go_to_omp`).
#[tokio::test]
async fn no_card_key_reaches_omps_tool_approval() {
    let answers: Vec<Value> = [
        "up", "down", "left", "right", "enter", "escape", "tab", "pageUp", "pageDown",
    ]
    .into_iter()
    .map(key)
    .chain([typed("N")])
    .collect();
    for (shape, screen) in [
        ("pi", PI),
        ("box", BOX),
        ("draft-pi", DRAFT_PI),
        ("draft-box", DRAFT_BOX),
    ] {
        let session = CardSession::new(&format!("approval-{shape}"), "omp", Some(screen));
        for answer in &answers {
            let refused = session
                .answer(answer.clone())
                .await
                .expect_err(&format!("{shape}: {answer} was accepted"));
            assert_eq!(refused.code, "answerInTerminal", "{shape}: {answer}");
        }
        let terminal = test_terminal::terminal(&session.target.zmx_name);
        assert_eq!(terminal.writes, Vec::<String>::new(), "{shape}");
        assert_eq!(terminal.captures, answers.len(), "{shape}");
    }
}

/// A select dialog OMP draws in place of its composer, which the chat cannot read row by row: its
/// card passes keys on.
const SELECTOR: &str = "╭─ Deploy target ──────────────────────╮\n\
│ Where should the build go?           │\n\
├──────────────────────────────────────┤\n\
│ ❯ staging                            │\n\
│   production                         │\n\
├──────────────────────────────────────┤\n\
│ ⏎ select · ⎋ cancel                  │\n\
╰──────────────────────────────────────╯\n";

#[tokio::test]
async fn every_card_key_reaches_omp_as_that_key() {
    // The bytes a terminal sends for each key; Escape in the kitty form zmx passes on
    // (`SESSION_CHAT_INTERRUPT`, a bare ESC is dropped).
    let keys = [
        ("up", "\x1b[A"),
        ("down", "\x1b[B"),
        ("left", "\x1b[D"),
        ("right", "\x1b[C"),
        ("enter", "\r"),
        ("escape", SESSION_CHAT_INTERRUPT),
        ("tab", "\t"),
        ("pageUp", "\x1b[5~"),
        ("pageDown", "\x1b[6~"),
    ];
    // Any of OMP's screens whose card passes keys on, but its tool approval (above).
    for (shape, screen) in [("selector", SELECTOR)] {
        let session = CardSession::new(&format!("keys-{shape}"), "omp", Some(screen));
        for (name, _) in keys {
            session.answer(key(name)).await.expect("a key on the card");
        }
        session
            .answer(typed("y"))
            .await
            .expect("a letter on the card");
        let mut expected: Vec<String> = keys.iter().map(|(_, bytes)| bytes.to_string()).collect();
        expected.push("y".to_string());
        assert_eq!(
            test_terminal::terminal(&session.target.zmx_name).writes,
            expected,
            "{shape}"
        );
    }
}

#[tokio::test]
async fn no_card_key_reaches_a_screen_that_no_longer_shows_the_card() {
    // A card still on a client, or a key already sent, after the screen it was for went away:
    // the approval answered or denied, another prompt, a shell, another program, or a screen that
    // could not be read. Every agent whose cards pass keys through answers on this route.
    let retired = retired_approval();
    let screens = [
        Some(retired.as_str()),
        Some("\u{276f} an unrelated prompt with an unsent draft"),
        Some("user@host % echo must-not-run"),
        Some("VIM - editing an unrelated buffer"),
        None,
    ];
    let answers: Vec<Value> = [
        "up", "down", "left", "right", "enter", "escape", "tab", "pageUp", "pageDown",
    ]
    .into_iter()
    .map(key)
    .chain([typed("x")])
    .collect();
    let mut written = Vec::new();
    for agent in ["omp", "pi", "codex", "cursor", "grok", "hermes", "empryo"] {
        for (screen_index, screen) in screens.iter().enumerate() {
            for (answer_index, answer) in answers.iter().enumerate() {
                let session = CardSession::new(
                    &format!("stale-{agent}-{screen_index}-{answer_index}"),
                    agent,
                    *screen,
                );
                let result = session.answer(answer.clone()).await;
                let writes = test_terminal::terminal(&session.target.zmx_name).writes;
                if result.is_ok() || !writes.is_empty() {
                    written.push(format!(
                        "{agent} screen {screen_index} {answer}: accepted {} wrote {writes:?}",
                        result.is_ok()
                    ));
                }
            }
        }
    }
    assert!(written.is_empty(), "{written:#?}");
}
