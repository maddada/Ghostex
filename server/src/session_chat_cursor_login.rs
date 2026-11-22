//! Cursor CLI's sign-in screens, read so the chat can ask the user to sign in.
//!
//! cursor-agent 2026.10.01 (its bundled `index.js`) shows a signed-out user its logo, or "Cursor Agent" in a
//! narrow terminal, above "Press any key to log in...". Any key starts the login: it opens the sign-in page
//! (unless `NO_OPEN_BROWSER` is set) and shows "Signing in with the browser..." (or "Signing in"), "If your
//! browser didn't open, click this link to log in:", the link on one unwrapped line, and "Press q to show a QR
//! code…". Success shows "Signing in..." then "✓ Login successful!" and starts the agent; a failure shows
//! "✗ Login failed" and exits with "Authentication required to use Cursor Agent." (rules.rs answers that exit).
//!
//! CDXC:AgentProviders 2026-10-07 DECISION:
//! User: "we need to support asking the user to login in the chat view" for Cursor. The welcome screen becomes a Sign in card whose button presses the key, the browser step shows Cursor's sign-in link (clickable, and openable on another device, so a phone or remote client can sign in), and signing out to the shell offers Sign in through Restart.
//! SEE-ALSO: packages/gx-chat-core/src/questions/terminal_dialog_copy.rs (`cursor_sign_in_copy` writes the browser step's card copy), server/src/session_chat_notice/rules.rs (the signed-out exit rule).

use crate::session_chat_notice::{
    session_chat_terminal_screen_tail, SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
    SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
};
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_terminal_dialog::TerminalDialog;

const SCAN_LINES: usize = 160;
const WELCOME_PROMPT: &str = "Press any key to log in...";
const LINK_INSTRUCTION: &str = "If your browser didn't open, click this link to log in:";
/// The two headings above the link: the browser was opened, or opening it is turned off.
const BROWSER_HEADINGS: [&str; 2] = ["Signing in with the browser...", "Signing in"];
const DIALOG_ID_PREFIX: &str = "cursor-sign-in:";

/// The screen's last lines, SGR-stripped and trimmed; blank rows stay as empty strings because they end the
/// link.
fn scan_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .rev()
        .take(SCAN_LINES)
        .map(|raw| normalize_spaces(&strip_ansi_sgr(raw)).trim().to_string())
        .collect();
    lines.reverse();
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

fn sign_in_finished_after(lines: &[String]) -> bool {
    lines.iter().any(|line| {
        line.contains("Login successful!")
            || line.contains("Login failed")
            || line.starts_with("Authentication required to use Cursor Agent")
    })
}

/// The link, rejoined when a terminal narrower than it wrapped it over several rows.
fn sign_in_link(after_instruction: &[String]) -> Option<String> {
    let link: String = after_instruction
        .iter()
        .skip_while(|line| line.is_empty())
        .take_while(|line| !line.is_empty() && !line.contains(' '))
        .map(String::as_str)
        .collect();
    (link.starts_with("https://") && link.len() > "https://".len()).then_some(link)
}

fn dialog_id(link: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    link.hash(&mut hasher);
    format!("{DIALOG_ID_PREFIX}{:016x}", hasher.finish())
}

/// The live Cursor sign-in step as a chat notice, or `None` when no sign-in screen owns the terminal.
pub fn detect_cursor_sign_in(text: &str) -> Option<SessionChatTerminalNotice> {
    let lines = scan_lines(text);
    let last = lines.last()?;
    if last == WELCOME_PROMPT {
        return Some(
            SessionChatTerminalNotice::new(
                SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
                SessionChatTerminalNoticeSeverity::Info,
                SessionChatTerminalNoticeSource::Screen,
                "Sign in to Cursor",
            )
            .with_input_blocking(true)
            .with_detail("Cursor isn't signed in on this computer. Sign in to start chatting; your browser opens Cursor's sign-in page.")
            .with_screen_tail(session_chat_terminal_screen_tail(text))
            .with_actions(vec![
                SessionChatTerminalNoticeAction::send_keys("signIn", "Sign in", "\r"),
                SessionChatTerminalNoticeAction::switch_to_terminal("Terminal view"),
            ]),
        );
    }
    let browser_step = lines
        .iter()
        .rposition(|line| line == LINK_INSTRUCTION)
        .filter(|instruction| !sign_in_finished_after(&lines[instruction + 1..]))
        .and_then(|instruction| {
            let link = sign_in_link(&lines[instruction + 1..])?;
            let heading = lines[..instruction]
                .iter()
                .rev()
                .find(|line| !line.is_empty())
                .filter(|line| BROWSER_HEADINGS.contains(&line.as_str()))?;
            Some((heading.clone(), link))
        });
    if let Some((heading, link)) = browser_step {
        let dialog = TerminalDialog {
            id: dialog_id(&link),
            title: heading,
            body: format!("{LINK_INSTRUCTION}\n{link}"),
            footer: String::new(),
            rows: Vec::new(),
            input: None,
            input_value: String::new(),
            actions: Vec::new(),
            side_question: None,
            blocks: None,
            picker: None,
        };
        let mut notice = dialog
            .into_notice(SESSION_CHAT_NOTICE_LOGIN_EXPIRED)
            .with_input_blocking(true);
        notice.severity = SessionChatTerminalNoticeSeverity::Warning;
        return Some(notice);
    }
    // "Signing in..." while the tokens are saved, then "✓ Login successful!" for a moment before the agent starts.
    let saving = last == "Signing in..."
        || (last == "Authentication tokens stored securely."
            && lines
                .iter()
                .rev()
                .take(4)
                .any(|line| line.ends_with("Login successful!")));
    if saving {
        return Some(
            SessionChatTerminalNotice::new(
                SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
                SessionChatTerminalNoticeSeverity::Info,
                SessionChatTerminalNoticeSource::Screen,
                "Signing in to Cursor…",
            )
            .with_input_blocking(true)
            .with_detail("Cursor is finishing your sign-in. Chat is ready as soon as it starts."),
        );
    }
    let failed = lines
        .iter()
        .rev()
        .take(4)
        .any(|line| line.ends_with("Login failed"));
    failed.then(|| {
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Screen,
            "Cursor sign-in failed",
        )
        .with_input_blocking(true)
        .with_detail(format!(
            "Cursor says: \"{last}\" It closes in a moment; then choose Sign in to try again."
        ))
        .with_screen_tail(session_chat_terminal_screen_tail(text))
    })
}
