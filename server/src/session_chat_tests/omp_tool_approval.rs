//! OMP 18.13.3's inline tool approval, captured live with the Unicode symbol preset and the `pi` and
//! `box` composer shapes. OMP keeps its composer on screen right below the approval frame and removes
//! the frame once the call is approved or denied. The card's keys must reach OMP as OMP, though its
//! transcript reads as Pi's, and only while a screen that takes them is up.

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

/// The open approval is a permission card with OMP's own rows, as Claude Code's, Antigravity's,
/// Copilot's and Grok's are, and it holds the chat's sends.
#[test]
fn the_open_approval_is_a_permission_card_with_omps_rows() {
    for screen in [PI, BOX] {
        let notice = classify_session_chat_terminal_notice(Some("omp"), screen).expect("notice");
        assert_eq!(notice.kind, SESSION_CHAT_NOTICE_PERMISSION_PROMPT);
        assert_eq!(notice.title, "Allow this?");
        assert_eq!(
            notice.detail.as_deref(),
            Some("Allow tool: bash Command: echo ghostex-approval-test")
        );
        assert_eq!(
            notice.approval_command.as_deref(),
            Some("echo ghostex-approval-test")
        );
        assert_eq!(
            notice.to_value()["approvalCommand"],
            "echo ghostex-approval-test"
        );
        let rows: Vec<(&str, bool)> = notice
            .choices
            .iter()
            .map(|choice| (choice.label.as_str(), choice.selected))
            .collect();
        assert_eq!(rows, [("Approve", true), ("Deny", false)]);
        assert!(notice.blocks_input());
        assert!(!notice.passes_keys_through());
        let readiness = detect_session_chat_composer_readiness(Some("omp"), screen, Some(&notice));
        assert_ne!(readiness.state, SessionChatComposerState::Ready);
    }
}

#[test]
fn omp_approvals_for_different_commands_are_different_notices() {
    for (screen, command) in [
        (PI, "echo ghostex-approval-test"),
        (BOX, "echo ghostex-approval-test"),
        (OMP_APPROVAL, "echo cap-b"),
    ] {
        let mut first = classify_session_chat_terminal_notice(Some("omp"), screen).expect("notice");
        first.detected_at = "first".to_string();
        let next_screen = screen.replace(
            &format!("Command: {command}"),
            "Command: echo another-command",
        );
        let mut next =
            classify_session_chat_terminal_notice(Some("omp"), &next_screen).expect("next notice");
        assert!(!next.same_notice(Some(&first)));
        next.detected_at = "next".to_string();
        next.carry_forward_detected_at(Some(&first));
        assert_eq!(next.detected_at, "next");
    }
}

#[test]
fn omp_approval_names_only_the_command_the_dialog_shows() {
    let notice = classify_session_chat_terminal_notice(Some("omp"), OMP_APPROVAL).expect("notice");
    assert_eq!(notice.approval_command.as_deref(), Some("echo cap-b"));
    assert_eq!(notice.to_value()["approvalCommand"], "echo cap-b");

    // OMP 18.15.0 cuts a long command at the box edge instead of wrapping it.
    let shown = "echo dies-ist-ein-sehr-langer-befehl-der-im-dialog-umbrechen-soll-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa…";
    let truncated = OMP_APPROVAL.replace("echo cap-b", shown);
    let notice =
        classify_session_chat_terminal_notice(Some("omp"), &truncated).expect("truncated notice");
    assert_eq!(notice.approval_command.as_deref(), Some(shown));
}

#[test]
fn omp_approval_keeps_visible_command_lines_and_spacing() {
    // A multi-line request in the captured frame shape, not a claimed OMP wrap capture.
    let screen = OMP_APPROVAL
        .replace("echo cap-b", "printf 'one  two'")
        .replace(
            "\n│ ❯ Approve",
            "\n│   printf 'three' │\n│ │               │\n│ ❯ Approve",
        );
    let notice = classify_session_chat_terminal_notice(Some("omp"), &screen).expect("notice");
    assert_eq!(
        notice.approval_command.as_deref(),
        Some("printf 'one  two'\n  printf 'three'\n│")
    );
}

#[test]
fn omp_approval_excludes_sections_after_the_command() {
    for appendix in [
        "Provider safety checks: │\n│ Network destination needs review",
        "Folder: workspace — 2 entries │\n│   - example.txt",
    ] {
        let screen =
            OMP_APPROVAL.replace("\n│ ❯ Approve", &format!("\n│ {appendix} │\n│ ❯ Approve"));
        let notice = classify_session_chat_terminal_notice(Some("omp"), &screen).expect("notice");
        assert_eq!(notice.approval_command.as_deref(), Some("echo cap-b"));
        assert_eq!(notice.to_value()["approvalCommand"], "echo cap-b");
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
    let session = json!({ "agentId": "omp" });
    let transcript = session_chat_agent_for_session(&session).expect("transcript agent");
    assert_eq!(transcript, "pi");
    assert!(classify_session_chat_terminal_notice(Some(&transcript), OMP_APPROVAL).is_none());
    let terminal = session_chat_display_agent_for_session(&session).expect("terminal agent");
    assert_eq!(terminal, "omp");
    let notice = classify_session_chat_terminal_notice(Some(&terminal), OMP_APPROVAL)
        .expect("next approval");
    assert_eq!(notice.kind, SESSION_CHAT_NOTICE_PERMISSION_PROMPT);
    assert_eq!(notice.approval_command.as_deref(), Some("echo cap-b"));
}

/// A row is picked by walking OMP's highlight to it and pressing Enter (its rows have no numbers);
/// the chat's Allow on the stored approval card picks the Approve row the same way.
#[test]
fn a_row_is_answered_by_walking_omps_highlight() {
    for screen in [PI, BOX] {
        let picker = detect_omp_permission_picker(screen).expect("picker");
        assert_eq!(picker.answer_key(0).as_deref(), Some("\r"));
        assert_eq!(picker.answer_key(1).as_deref(), Some("\u{1b}[1;1B\r"));
        assert_eq!(picker.answer_key(2), None);
    }
    assert!(detect_omp_permission_picker(&retired_approval()).is_none());
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
/// message: the chat shows it as the terminal card that passes keys to OMP, not as rows.
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

#[tokio::test]
async fn a_message_typed_on_the_card_reaches_omp_and_enter_denies_and_sends() {
    let message = "Use the staging folder instead, then run it again.";
    // The open approval is answered from its rows; its terminal card exists once a key typed in
    // OMP's terminal started the message, so the keys here continue that message.
    for (shape, drafting) in [("pi", DRAFT_PI), ("box", DRAFT_BOX)] {
        let session = CardSession::new(&format!("message-{shape}"), "omp", Some(drafting));
        for (index, letter) in message.chars().enumerate() {
            session
                .answer(typed(&letter.to_string()))
                .await
                .unwrap_or_else(|error| panic!("{shape}: letter {index} refused: {error:?}"));
        }
        // Enter on a draft is OMP's "deny and send"; it has to reach OMP as Enter.
        session
            .answer(key("enter"))
            .await
            .unwrap_or_else(|error| panic!("{shape}: Enter refused: {error:?}"));
        let terminal = test_terminal::terminal(&session.target.zmx_name);
        assert_eq!(terminal.writes.concat(), format!("{message}\r"));
        // Every key was checked against the screen before it was written.
        assert_eq!(terminal.captures, message.chars().count() + 1);
    }
}

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
    for (shape, screen) in [("drafting", DRAFT_BOX)] {
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
