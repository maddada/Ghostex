//! Antigravity CLI's sign-in screen in a running session. A signed-out `agy` opens on its welcome
//! and login-method menu instead of its input box; the chat must say so the way it does for Claude
//! Code's own login-method menu, and a message sent meanwhile must be held the same way.

use crate::session_chat_composer::{session_composer_verdict, SessionChatComposerState};
use crate::session_chat_notice::{
    classify_session_chat_terminal_notice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeSeverity, SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
    SESSION_CHAT_NOTICE_TRUST_PROMPT,
};
use crate::session_chat_send::{guarded_write_refusal, GuardRefusal, InputGuard};

/// The screen of a user's session report (Ghostex 11.10.0, 24 x 200): a signed-out `agy` started
/// by another agent, sitting on its sign-in menu.
const AGY_SIGN_IN: &str = "\
Welcome to the Antigravity CLI. You are currently not signed in.

Select login method:

> 1. Google OAuth
  2. Use a Google Cloud project

↑/↓ Navigate · enter Select";

/// Claude Code's login-method menu, the sign-in screen the existing card is raised for.
const CLAUDE_SIGN_IN: &str = "\
 Select login method:

 ❯ 1. Claude account with subscription · Pro, Max, Team, or Enterprise

   2. Anthropic Console account · API usage billing

   3. 3rd-party platform · Amazon Bedrock, Microsoft Foundry, or Vertex AI";

/// Antigravity's idle input box (the shape of issue #349's capture, Antigravity CLI 1.4.9).
fn agy_input_box(above: &str) -> String {
    let rule = "─".repeat(55);
    format!("{above}\n  Antigravity CLI 1.4.9\n  Gemini 3.8 Flash (Medium)\n\n{rule}\n> \n{rule}\n? for shortcuts")
}

/// Antigravity's folder-trust prompt, in the shape its detector reads
/// (session_chat_workspace_trust.rs).
const AGY_TRUST: &str = "\
 Accessing workspace:

 /Users/me/project

 Antigravity CLI requires permission to read, edit, and execute files here.

 > Yes, I trust this folder
   No, exit

 Enter to confirm · Esc to cancel";

const HELD_REASON: &str = "Finish signing in. Clear it in the terminal before sending.";

#[test]
fn antigravitys_sign_in_menu_raises_the_finish_signing_in_card() {
    let notice =
        classify_session_chat_terminal_notice(Some("antigravity"), AGY_SIGN_IN).expect("notice");
    assert_eq!(notice.kind, SESSION_CHAT_NOTICE_LOGIN_EXPIRED);
    assert_eq!(notice.severity, SessionChatTerminalNoticeSeverity::Warning);
    assert_eq!(notice.title, "Finish signing in");
    assert_eq!(
        notice.detail.as_deref(),
        Some("Antigravity CLI is waiting for you to sign in. Finish or cancel it in the terminal, then send your message.")
    );
    assert!(notice.blocks_input());
    assert_eq!(
        notice.actions,
        vec![SessionChatTerminalNoticeAction::switch_to_terminal(
            "Terminal view"
        )]
    );
    // The same card, kind and button Claude Code's login-method menu gets.
    let claude =
        classify_session_chat_terminal_notice(Some("claude"), CLAUDE_SIGN_IN).expect("claude");
    assert_eq!(
        (
            &claude.kind,
            claude.severity,
            &claude.title,
            claude.blocks_input(),
            &claude.actions
        ),
        (
            &notice.kind,
            notice.severity,
            &notice.title,
            notice.blocks_input(),
            &notice.actions
        )
    );
    // Every launch name of Antigravity reads its screen the same way.
    for agent in ["antigravity-cli", "agy"] {
        assert_eq!(
            classify_session_chat_terminal_notice(Some(agent), AGY_SIGN_IN)
                .map(|notice| notice.title),
            Some("Finish signing in".to_string()),
            "{agent}"
        );
    }
}

/// The input-box verdict the session report and the not-ready card state, and what a send to the
/// session (the chat box, `ghostex agents send`) is held with: the same as for Claude Code.
#[test]
fn a_message_to_antigravitys_sign_in_menu_is_held_as_claudes_is() {
    for (agent, screen) in [("antigravity", AGY_SIGN_IN), ("claude", CLAUDE_SIGN_IN)] {
        let verdict = session_composer_verdict(Some(agent), screen);
        assert_eq!(verdict.state, SessionChatComposerState::NotReady, "{agent}");
        assert_eq!(verdict.reason.as_deref(), Some(HELD_REASON), "{agent}");
        assert_eq!(
            guarded_write_refusal(Some(agent), &InputGuard::InputBox, Some(screen)),
            Err(GuardRefusal::Prompt(HELD_REASON.to_string())),
            "{agent}"
        );
    }
}

#[test]
fn antigravitys_input_box_is_not_taken_for_its_sign_in_menu() {
    let idle = agy_input_box("");
    assert!(classify_session_chat_terminal_notice(Some("antigravity"), &idle).is_none());
    assert_eq!(
        session_composer_verdict(Some("antigravity"), &idle).state,
        SessionChatComposerState::Ready
    );
    // A reply that quotes the menu, with Antigravity's own input box drawn below it, is text on
    // the screen, not this session's sign-in.
    let quoted = agy_input_box(AGY_SIGN_IN);
    assert!(
        classify_session_chat_terminal_notice(Some("antigravity"), &quoted)
            .is_none_or(|notice| notice.kind != SESSION_CHAT_NOTICE_LOGIN_EXPIRED)
    );
    assert_eq!(
        guarded_write_refusal(Some("antigravity"), &InputGuard::InputBox, Some(&quoted)),
        Ok(())
    );
}

/// A signed-in user's draft that spells the menu's words sits inside the input box itself, so no
/// ready box is drawn below the matched row; the box around that row still proves it is a draft.
#[test]
fn a_draft_in_the_input_box_that_spells_the_menu_is_not_a_sign_in() {
    let draft = "Can you explain Select login method: Google OAuth?";
    let agy = agy_input_box("").replace("\n> \n", &format!("\n> {draft}\n"));
    let rule = "─".repeat(55);
    let claude = format!("{rule}\n❯ {draft}\n{rule}\n  Sonnet 5.5 | medium | $0.00");
    for (agent, screen) in [("antigravity", agy), ("claude", claude)] {
        assert_eq!(
            crate::session_chat_composer::detect_session_chat_composer_ready(Some(agent), &screen)
                .state,
            SessionChatComposerState::Ready,
            "{agent}: the fixture draws a ready input box"
        );
        assert!(
            classify_session_chat_terminal_notice(Some(agent), &screen)
                .is_none_or(|notice| notice.kind != SESSION_CHAT_NOTICE_LOGIN_EXPIRED),
            "{agent}"
        );
        assert_eq!(
            guarded_write_refusal(Some(agent), &InputGuard::InputBox, Some(&screen)),
            Ok(()),
            "{agent}"
        );
    }
}

#[test]
fn antigravitys_folder_trust_prompt_stays_the_trust_card() {
    let notice =
        classify_session_chat_terminal_notice(Some("antigravity"), AGY_TRUST).expect("notice");
    assert_eq!(notice.kind, SESSION_CHAT_NOTICE_TRUST_PROMPT);
    assert_eq!(notice.title, "Trust this folder?");
}

/// Claude Code's own login menu, read as Antigravity's screen, lacks Antigravity's login methods.
#[test]
fn another_agents_login_menu_is_not_antigravitys() {
    assert!(
        classify_session_chat_terminal_notice(Some("antigravity"), CLAUDE_SIGN_IN)
            .is_none_or(|notice| notice.kind != SESSION_CHAT_NOTICE_LOGIN_EXPIRED)
    );
}
