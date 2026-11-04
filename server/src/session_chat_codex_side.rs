//! Codex's side conversation (`/btw`, `/side`): a temporary fork of the thread that owns Codex's
//! input box until Ctrl+C closes it.
//!
//! CDXC:SessionChat 2026-09-30 DECISION:
//! User: in Codex chats a Side chat message switches to the terminal and the user deals with the side conversation there. A later chat message leaves the side conversation before it is sent, and switching back to the chat view leaves it too, so chat reads and types into the main thread again.
//! SEE-ALSO: packages/gx-chat-core/src/composer/side_chat.rs, server/src/session_chat_composer.rs (readiness), server/src/session_chat_send/ (`DismissClaudePanel`, `handle_handoff_session_chat_draft_http`).

use crate::session_chat_options::strip_ansi_sgr;

/// Footer lines scanned: the hint sits on Codex's bottom row, under the status line.
const CODEX_SIDE_FOOTER_LINES: usize = 3;

/// Whether Codex is showing its side conversation, read from the footer hint it draws only there
/// (`Side from main thread · ctrl+/ to switch · ctrl+c to close`, Codex 0.159).
///
/// CDXC:SessionChat 2026-09-30 WHY: Ctrl+C in Codex's main thread interrupts the turn or quits Codex, so it is pressed only on this positive evidence. The main thread with a side conversation parked behind it shows `ctrl+/ for side` instead, and a narrow screen that drops the hint reads as not in a side conversation.
pub fn codex_side_conversation_on_screen(screen_text: &str) -> bool {
    screen_text
        .lines()
        .map(strip_ansi_sgr)
        .filter(|line| !line.trim().is_empty())
        .rev()
        .take(CODEX_SIDE_FOOTER_LINES)
        .any(|line| {
            // Codex 0.161 writes Control as `⌃` on macOS and `^` on Linux (`⌃/ to switch · ⌃c to close`).
            let line = line
                .to_ascii_lowercase()
                .replace('⌃', "ctrl+")
                .replace('^', "ctrl+");
            line.contains("ctrl+/ to switch") && line.contains("ctrl+c to close")
        })
}

#[cfg(test)]
mod tests {
    use super::codex_side_conversation_on_screen;

    #[test]
    fn reads_the_side_hint_in_every_modifier_spelling() {
        for hint in [
            "Side from main thread · ctrl+/ to switch · ctrl+c to close",
            "Side from main thread · ⌃/ to switch · ⌃c to close",
            "Side from main thread · ^/ to switch · ^c to close",
        ] {
            assert!(
                codex_side_conversation_on_screen(&format!(
                    "› Ask Codex to do anything\n\n  {hint}"
                )),
                "{hint}"
            );
        }
        assert!(!codex_side_conversation_on_screen(
            "› Ask Codex to do anything\n\n  ⌃/ for side"
        ));
    }
}
