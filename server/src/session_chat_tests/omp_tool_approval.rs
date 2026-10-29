//! OMP 18.13.3's inline tool approval, captured live with the Unicode symbol preset and the `pi` and
//! `box` composer shapes. OMP keeps its composer on screen right below the approval frame and removes
//! the frame once the call is approved or denied. The card's keys must reach OMP as OMP, though its
//! transcript reads as Pi's.

use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, SessionChatComposerState,
};
use crate::session_chat_follower::{omp_agent_for_pi_transcript, session_chat_agent_for_session};
use crate::session_chat_notice::{
    classify_session_chat_terminal_notice, SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED,
};
use crate::session_chat_omp_blocking::detect_omp_blocking_screen;

const PI: &str = include_str!("fixtures/omp-tool-approval-pi.txt");
const BOX: &str = include_str!("fixtures/omp-tool-approval-box.txt");

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

#[test]
fn the_chat_holds_sends_while_the_approval_is_open() {
    for screen in [PI, BOX] {
        let notice = classify_session_chat_terminal_notice(Some("omp"), screen).expect("notice");
        assert_eq!(notice.kind, SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED);
        assert_eq!(notice.title, APPROVAL);
        assert!(notice.blocks_input());
        assert!(notice.passes_keys_through());
        let readiness = detect_session_chat_composer_readiness(Some("omp"), screen, Some(&notice));
        assert_ne!(readiness.state, SessionChatComposerState::Ready);
    }
}

#[test]
fn an_approval_with_any_line_below_it_stays_retired() {
    // The live `pi` capture with one line between the approval frame and the composer, the way a
    // frame left in scrollback is followed by whatever OMP printed after it.
    let mut lines: Vec<&str> = PI.lines().collect();
    let bottom = lines
        .iter()
        .position(|line| line.trim_start().starts_with('\u{2570}'))
        .expect("approval frame bottom");
    lines.insert(bottom + 1, " Denied: echo ghostex-approval-test");
    assert_eq!(detect_omp_blocking_screen(&lines.join("\n")), None);
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
