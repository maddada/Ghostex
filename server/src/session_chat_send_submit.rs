//! Proof that the Return a chat send wrote was taken as a submission.

use std::time::{Duration, Instant};

use crate::session_chat_send::{
    capture_session_terminal_text_vt, normalize_session_chat_screen_text,
    session_chat_paste_needles, write_session_chat_payload, SessionChatSendError,
    SessionChatSendFailure, SESSION_CHAT_SUBMIT,
};

/// Codex clears its input box within a frame of taking a submission; the window only has to
/// outlast a slow repaint.
const SUBMIT_CHECK_WINDOW: Duration = Duration::from_millis(1_500);
const SUBMIT_CHECK_POLL: Duration = Duration::from_millis(150);
/// Screen lines kept in the diagnostics entry.
const SUBMIT_CHECK_TAIL_LINES: usize = 30;

const CODEX_KEPT_MESSAGE: &str = "Codex kept the message in its input box instead of sending it. Press Enter in the terminal to send it.";

/// CDXC:SessionChat 2026-09-26 DECISION:
/// User: a Codex message must never again show as sent in the chat while it sits unsent in the CLI's input box ("my message was in the input box of the cli just needed to hit enter"). After the Return, the send reads Codex's input box; if the message is still there, it presses Return once more, as the user had to, logs the screen so the cause can be fixed at its source, and reports a failure if Codex still keeps it.
/// WHY: Codex only: its input box is read from styled cells, so a history row that repeats the message is never mistaken for it. A screen that cannot be read, or an input box that is gone (a working Codex, a dialog), counts as taken, so this never blocks a message it cannot see.
pub(crate) async fn confirm_codex_submitted(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    text: &str,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), SessionChatSendError> {
    let needles = session_chat_paste_needles(text);
    if needles.is_empty() {
        return Ok(());
    }
    let Some(screen) = message_still_held(zmx_name, &needles, cancelled).await else {
        return Ok(());
    };
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatSendPressedEnterAgain",
        project_id,
        session_id,
        "Codex kept the message in its input box after Return.",
        &screen_tail(&screen),
    );
    write_session_chat_payload(
        project_id,
        session_id,
        zmx_name,
        source,
        SESSION_CHAT_SUBMIT,
    )
    .await
    .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message))?;
    let Some(screen) = message_still_held(zmx_name, &needles, cancelled).await else {
        return Ok(());
    };
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatSendNotSubmitted",
        project_id,
        session_id,
        CODEX_KEPT_MESSAGE,
        &screen_tail(&screen),
    );
    Err(SessionChatSendError::new(
        SessionChatSendFailure::Write,
        CODEX_KEPT_MESSAGE.to_string(),
    ))
}

/// The screen that still shows the message in Codex's input box once the window has passed, or
/// `None` as soon as the box no longer holds it (or cannot be read, or the send was cancelled).
async fn message_still_held(
    zmx_name: &str,
    needles: &[String],
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Option<String> {
    let started = Instant::now();
    loop {
        tokio::time::sleep(SUBMIT_CHECK_POLL).await;
        if cancelled() {
            return None;
        }
        let screen = capture_session_terminal_text_vt(zmx_name).await?;
        let input = crate::session_chat_composer::session_chat_composer_input("codex", &screen)?;
        let held = !input.is_empty() && {
            let typed = normalize_session_chat_screen_text(&input.text);
            needles.iter().any(|needle| typed.contains(needle.as_str()))
        };
        if !held {
            return None;
        }
        if started.elapsed() >= SUBMIT_CHECK_WINDOW {
            return Some(screen);
        }
    }
}

fn screen_tail(screen: &str) -> Vec<String> {
    let lines: Vec<String> = screen
        .lines()
        .map(|line| {
            crate::session_chat_options::strip_ansi_sgr(line)
                .trim_end()
                .to_string()
        })
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(SUBMIT_CHECK_TAIL_LINES)..].to_vec()
}
