//! Where Claude's AskUserQuestion selector stands when the chat answers it, read again while a
//! repaint settles, and the Escape that closes it when its highlight still cannot be placed.

use std::time::Duration;

use crate::session_chat::SessionChatQuestion;
use crate::session_chat_question_liveness::{
    claude_question_dialog_at_bottom, claude_question_selector_on_screen,
    claude_question_selector_position, ClaudeQuestionSelectorPosition,
};
use crate::session_chat_question_row_align::write_keys_and_await_repaint;
use crate::session_chat_send::{capture_session_terminal_text, SESSION_CHAT_INTERRUPT};

/// Reads of the screen before an answer gives up placing the highlight.
const READS: usize = 6;
const READ_GAP_MS: u64 = 300;
/// Escapes before a selector that stays open fails the answer.
const ESCAPES: usize = 3;

/// The selector as one answer finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeQuestionScreen {
    /// No read of the terminal came back.
    Unreadable,
    /// No selector for these questions is on screen; the answer goes as a message.
    Gone,
    /// The selector and where its highlight stands.
    Placed(ClaudeQuestionSelectorPosition),
    /// The selector shows, but no read placed its highlight. `cancellable` when the bottom of the
    /// last read is this question's own selector, which Escape closes.
    Unplaced { cancellable: bool },
}

/// One capture, classified.
pub fn classify_claude_question_screen(
    questions: &[SessionChatQuestion],
    screen_text: &str,
) -> ClaudeQuestionScreen {
    if let Some(position) = claude_question_selector_position(questions, screen_text) {
        return ClaudeQuestionScreen::Placed(position);
    }
    if !claude_question_selector_on_screen(questions, screen_text) {
        return ClaudeQuestionScreen::Gone;
    }
    ClaudeQuestionScreen::Unplaced {
        cancellable: claude_question_dialog_at_bottom(questions, screen_text),
    }
}

/// CDXC:SessionChat 2026-10-29 WHY: GitHub #350: one capture decided every Claude answer, so a capture taken while Claude (or the Windows console under it) was still repainting the selector refused the answer with "where its highlight sits could not be read", which a second read a moment later would have placed. A highlight that cannot be placed is read again for about two seconds before the answer falls back to Escape and a message (`claude_question_dialog_at_bottom`).
pub(crate) async fn read_claude_question_screen(
    zmx_name: &str,
    questions: &[SessionChatQuestion],
) -> ClaudeQuestionScreen {
    let mut last = ClaudeQuestionScreen::Unreadable;
    for read in 0..READS {
        if read > 0 {
            tokio::time::sleep(Duration::from_millis(READ_GAP_MS)).await;
        }
        let Some(screen) = capture_session_terminal_text(zmx_name).await else {
            continue;
        };
        last = classify_claude_question_screen(questions, &screen);
        if !matches!(last, ClaudeQuestionScreen::Unplaced { .. }) {
            return last;
        }
    }
    last
}

/// Closes this question's selector with Escape, so the message with the answers that follows
/// reaches Claude's input box. See CDXC:SessionChat on `claude_question_dialog_at_bottom`.
pub(crate) async fn cancel_claude_question(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    questions: &[SessionChatQuestion],
    superseded: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), String> {
    for _ in 0..=ESCAPES {
        if superseded() {
            return Err("The answer was superseded before it reached Claude.".to_string());
        }
        let Some(capture) = capture_session_terminal_text(zmx_name).await else {
            return Err(
                "The session's terminal could not be read, so the answer was not sent.".to_string(),
            );
        };
        if !claude_question_dialog_at_bottom(questions, &capture) {
            // Closed, or never this question's dialog: the message's own send checks the input box.
            return Ok(());
        }
        write_keys_and_await_repaint(
            project_id,
            session_id,
            zmx_name,
            source,
            &[SESSION_CHAT_INTERRUPT.to_string()],
            &capture,
        )
        .await?;
    }
    Err("Claude's question stayed open after Escape, so the answer was not sent. Answer it in the terminal.".to_string())
}

#[cfg(test)]
#[path = "session_chat_claude_question_answer_tests.rs"]
mod tests;
