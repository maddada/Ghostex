//! Proof that the Return a chat send wrote was taken as a submission.

use std::time::{Duration, Instant};

use crate::session_chat_send::{
    build_agent_tui_clear_input_for_text, capture_session_terminal_text_vt,
    normalize_session_chat_screen_text, record_busy_agent_wait, session_chat_paste_needles,
    wait_out_busy_agent, write_session_chat_payload, AgentMidTurnProbe, BusyAgentWait,
    SessionChatSendError, SessionChatSendFailure, SESSION_CHAT_EMPRYO_SUBMIT,
};

/// Codex and Claude Code clear their input box within a frame of taking a submission; the window
/// only has to outlast a slow repaint.
const SUBMIT_CHECK_WINDOW: Duration = Duration::from_millis(1_500);
const SUBMIT_CHECK_POLL: Duration = Duration::from_millis(150);
/// Screen lines kept in the diagnostics entry.
const SUBMIT_CHECK_TAIL_LINES: usize = 30;
/// Claude Code's collapsed large-paste placeholder, normalized and lowercased.
const CLAUDE_PASTED_PLACEHOLDER: &str = "[pastedtext";

const CLAUDE_KEPT_MESSAGE: &str = "Claude Code kept the message in its input box instead of sending it, so nothing was sent and Ghostex cleared that input box.";

/// Whether a send failed because the agent kept the message in its input box after the Return.
pub(crate) fn is_not_submitted_failure(message: &str) -> bool {
    message == CLAUDE_KEPT_MESSAGE
        || message.contains("kept the message in its input box instead of sending it. Press")
}

/// The agents whose input box a send reads back after its Return.
pub(crate) fn verifies_submission(agent: &str) -> bool {
    matches!(agent, "codex" | "claude" | "openclaude" | "empryo")
}

/// CDXC:SessionChat 2026-09-26 DECISION:
/// User: a Codex message must never again show as sent in the chat while it sits unsent in the CLI's input box ("my message was in the input box of the cli just needed to hit enter"). After the Return, the send reads Codex's input box; if the message is still there, it presses Return once more, as the user had to, logs the screen so the cause can be fixed at its source, and reports a failure if Codex still keeps it.
/// WHY: the input box is read from its own region (Codex's styled cells, Claude Code's rule-bounded box), so a history row that repeats the message is never mistaken for it. A screen that cannot be read, or an input box that is gone (a working Codex, a dialog), counts as taken, so this never blocks a message it cannot see.
/// CDXC:SessionChat 2026-10-07 WHY: Empryo takes the same check, and the second press repeats the key the send used. A slash command goes with Enter, which only arms while Empryo 3.9.1-beta's repo map is still building ("Enter again sends before the map is ready"), so a second Enter is Empryo's own way to run it; a message goes with Alt+Q, which skips that gate, and its second press stays Alt+Q because Enter would steer a running turn instead of queueing the message.
/// CDXC:SessionChat 2026-10-04 WHY: Claude Code takes the same check. A `ghostex agents send` whose paste Claude would not submit (a form feed in it; see `picture_terminal_control_characters`) answered "accepted" while the message sat in Claude's input box, until the delivery watchdog handed it back to the chat composer as a draft. Claude's input box is cleared when it still holds the message after the second Return, because pressing Enter there would not send it either and the failure already says nothing was sent.
pub(crate) async fn confirm_submitted(
    agent: &str,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    text: &str,
    submit: &str,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
    mid_turn: Option<&AgentMidTurnProbe>,
) -> Result<(), SessionChatSendError> {
    let needles = session_chat_paste_needles(text);
    if needles.is_empty() {
        return Ok(());
    }
    let Some(screen) = message_still_held(agent, zmx_name, &needles, cancelled).await else {
        return Ok(());
    };
    // Claude's prompt hook refused it: another Return would only run the hook again.
    if crate::session_chat_hook_blocked::claude_screen_shows_hook_block(agent, &screen, text) {
        crate::session_chat_hook_blocked::finish_hook_blocked_send(
            project_id,
            session_id,
            zmx_name,
            source,
            text,
            &screen_tail(&screen),
        )
        .await;
        return Ok(());
    }
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatSendPressedEnterAgain",
        project_id,
        session_id,
        &format!(
            "{} kept the message in its input box after {}.",
            agent_name(agent),
            submit_key_name(submit)
        ),
        &screen_tail(&screen),
    );
    write_session_chat_payload(project_id, session_id, zmx_name, source, submit)
        .await
        .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message))?;
    let Some(screen) = message_still_held(agent, zmx_name, &needles, cancelled).await else {
        return Ok(());
    };
    // A mid-turn agent takes the Return when its screen catches up; clearing the box now would
    // throw away a message it is about to queue (busy_input_wait.rs).
    if let Some(probe) = mid_turn {
        let needles = needles.as_slice();
        match wait_out_busy_agent(probe, cancelled, move || async move {
            !message_held_now(agent, zmx_name, needles).await
        })
        .await
        {
            BusyAgentWait::Settled => {
                record_busy_agent_wait(project_id, session_id, "it took the Return");
                return Ok(());
            }
            BusyAgentWait::Cancelled => return Ok(()),
            BusyAgentWait::NotMidTurn | BusyAgentWait::TurnEnded | BusyAgentWait::Stalled => {}
        }
    }
    let kept = if matches!(agent, "codex" | "empryo") {
        format!(
            "{} kept the message in its input box instead of sending it. Press {} in the terminal to send it.",
            agent_name(agent),
            submit_key_name(submit)
        )
    } else {
        // The returned-prompt detector would otherwise find this text in the box later and hand it
        // to the chat composer as a draft the sender never sees.
        let _ = write_session_chat_payload(
            project_id,
            session_id,
            zmx_name,
            source,
            &build_agent_tui_clear_input_for_text(text),
        )
        .await;
        CLAUDE_KEPT_MESSAGE.to_string()
    };
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatSendNotSubmitted",
        project_id,
        session_id,
        &kept,
        &screen_tail(&screen),
    );
    Err(SessionChatSendError::new(
        SessionChatSendFailure::Write,
        kept,
    ))
}

fn submit_key_name(submit: &str) -> &'static str {
    if submit == SESSION_CHAT_EMPRYO_SUBMIT {
        "Alt+Q"
    } else {
        "Enter"
    }
}

fn agent_name(agent: &str) -> &'static str {
    match agent {
        "codex" => "Codex",
        "empryo" => "Empryo",
        _ => "Claude Code",
    }
}

/// The screen that still shows the message in the agent's input box once the window has passed,
/// or `None` as soon as the box no longer holds it (or cannot be read, or the send was cancelled).
async fn message_still_held(
    agent: &str,
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
        if !input_holds_message(agent, &screen, needles) {
            return None;
        }
        if started.elapsed() >= SUBMIT_CHECK_WINDOW {
            return Some(screen);
        }
    }
}

/// One capture of `message_still_held`'s question; an unreadable screen counts as taken.
async fn message_held_now(agent: &str, zmx_name: &str, needles: &[String]) -> bool {
    capture_session_terminal_text_vt(zmx_name)
        .await
        .is_some_and(|screen| input_holds_message(agent, &screen, needles))
}

fn input_holds_message(agent: &str, screen: &str, needles: &[String]) -> bool {
    crate::session_chat_composer::session_chat_composer_input(agent, screen).is_some_and(|input| {
        !input.is_empty() && {
            let typed = normalize_session_chat_screen_text(&input.text);
            needles.iter().any(|needle| typed.contains(needle.as_str()))
                || (agent != "codex" && typed.to_lowercase().contains(CLAUDE_PASTED_PLACEHOLDER))
        }
    })
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
