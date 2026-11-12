//! Proof that the Return a chat send wrote was taken as a submission.

use std::time::{Duration, Instant};

use crate::session_chat_send::{
    build_agent_tui_clear_input_for_text, capture_session_terminal_text_vt,
    normalize_session_chat_screen_text, record_busy_agent_wait, session_chat_paste_needles,
    wait_out_busy_agent, write_session_chat_payload_guarded, AgentMidTurnProbe, BusyAgentWait,
    InputGuard, SessionChatSendError, SessionChatSendFailure, SESSION_CHAT_EMPRYO_SUBMIT,
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

/// How long a starting Codex is given to take a message it holds as a submission.
const STARTUP_WAIT_MAX: Duration = Duration::from_secs(3 * 60);
const STARTUP_WAIT_POLL: Duration = Duration::from_millis(250);
/// Once its startup line has gone, how long the input box gets to empty before the message
/// counts as still unsent.
const STARTUP_CATCH_UP: Duration = Duration::from_secs(4);

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
    since: Instant,
) -> Result<(), SessionChatSendError> {
    let needles = session_chat_paste_needles(text);
    if needles.is_empty() {
        return Ok(());
    }
    let Some(mut screen) = message_still_held(agent, zmx_name, &needles, cancelled).await else {
        return Ok(());
    };
    // CDXC:SessionChat 2026-11-11 DECISION:
    // User (through the coordinator), of a message to a Codex session that had just been woken:
    // the chat said "Message not sent: Codex kept the message in its input box" with Retry about
    // 15 seconds after it was sent, although Codex ran it. "A notice must never claim a
    // delivered message was not sent, and Retry there would double it." Codex 0.167 takes the
    // Return while it is still starting and keeps the message in its input box under "Waiting
    // for startup · esc cancel" until it has started, then submits it by itself. That is a
    // message Codex has, not one it kept: no second Return is pressed (it would be a second
    // submission waiting) and nothing is reported. The send waits for the box to empty; if Codex
    // is still starting after `STARTUP_WAIT_MAX` the send still answers that it was taken, and
    // the delivery watchdog reads the transcript as it does for every send.
    if holds_submission_until_started(agent, &screen) {
        match wait_out_startup(agent, zmx_name, &needles, cancelled).await {
            StartupWait::Taken(what) => {
                crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                    "sessionChatSendWaitedForStartup",
                    project_id,
                    session_id,
                    what,
                    &screen_tail(&screen),
                );
                return Ok(());
            }
            // Started, and the message is still in the box: the Return was not taken after all.
            StartupWait::StillHeld(now) => screen = now,
        }
    }
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
    // Only while the input box still holds the message (GitHub #555, guarded_write.rs).
    let holds = InputGuard::Holds(text.to_string());
    match write_session_chat_payload_guarded(
        project_id,
        session_id,
        zmx_name,
        source,
        Some(agent),
        &holds,
        submit,
        since,
    )
    .await
    {
        // CDXC:SessionChat 2026-11-12 WHY: GitHub #555 review: the first Return was written, and an
        // agent whose screen lags still shows the message after taking it. A refused second Return
        // reported "nothing was sent", so the queue typed the message again and the agent got it
        // twice (seen live with a stand-in that kept the text on screen 2.5 s after its Return).
        // The message may well be sent: it counts as submitted, the delivery watchdog reads the
        // transcript as it does for every send, and it is never typed again from here.
        Err(error) if error.composer_not_ready() => {
            crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                "sessionChatSendSecondReturnHeld",
                project_id,
                session_id,
                "A prompt appeared before the second Return; the first Return was written, so the message counts as sent.",
                &screen_tail(&screen),
            );
            return Ok(());
        }
        result => result?,
    }
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
        let _ = write_session_chat_payload_guarded(
            project_id,
            session_id,
            zmx_name,
            source,
            Some(agent),
            &holds,
            &build_agent_tui_clear_input_for_text(text),
            since,
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

/// How waiting out a starting agent ended.
enum StartupWait {
    /// The message left the input box, the screen could not be read, the send was cancelled, or
    /// the agent was still starting when the wait ran out: all count as taken, with what to log.
    Taken(&'static str),
    /// The agent has started and its input box still holds the message: the screen it shows.
    StillHeld(String),
}

/// Whether the agent shows that it holds what was submitted until it has started: Codex's status
/// line under its input box, "Waiting for startup · esc cancel". Read from the screen's last
/// lines only, so a message that quotes those words is not mistaken for the line.
fn holds_submission_until_started(agent: &str, screen: &str) -> bool {
    agent == "codex"
        && screen
            .lines()
            .rev()
            .map(|line| crate::session_chat_options::strip_ansi_sgr(line))
            .filter(|line| !line.trim().is_empty())
            .take(3)
            .any(|line| {
                let line = line.trim().to_lowercase();
                line.starts_with("waiting for startup") && line.contains("esc")
            })
}

async fn wait_out_startup(
    agent: &str,
    zmx_name: &str,
    needles: &[String],
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> StartupWait {
    let started = Instant::now();
    let mut started_at: Option<Instant> = None;
    loop {
        tokio::time::sleep(STARTUP_WAIT_POLL).await;
        if cancelled() {
            return StartupWait::Taken("The send was cancelled while the agent was starting.");
        }
        let Some(screen) = capture_session_terminal_text_vt(zmx_name).await else {
            return StartupWait::Taken(
                "The agent's screen could not be read while it was starting.",
            );
        };
        if !input_holds_message(agent, &screen, needles) {
            return StartupWait::Taken(
                "The agent held the message while it was starting and took it once it had started.",
            );
        }
        let now = Instant::now();
        if holds_submission_until_started(agent, &screen) {
            started_at = None;
            if now >= started + STARTUP_WAIT_MAX {
                return StartupWait::Taken(
                    "The agent was still starting after three minutes with the message submitted in its input box.",
                );
            }
            continue;
        }
        // Its startup line is gone: it submits what it held within a moment of that.
        match started_at {
            None => started_at = Some(now),
            Some(at) if now >= at + STARTUP_CATCH_UP => return StartupWait::StillHeld(screen),
            Some(_) => {}
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Codex 0.167's screen 4 seconds after a message was sent to a session that had just been
    /// woken (a test server, 2026-11-11): the Return is taken and waits for startup.
    const CODEX_STARTING: &str = "  >_ OpenAI Codex (v0.167.0)\n\
     ~/proj\n\
  Resuming session\u{2026}\n\
\u{203a} Test only, from a thread: the release notes draft is done.\n\
  nothing else.\n\
\n\
  Waiting for startup  \u{b7} esc cancel\n";

    #[test]
    fn a_starting_codex_holds_the_message_as_a_submission() {
        assert!(holds_submission_until_started("codex", CODEX_STARTING));
        // Its ordinary footer is not that line, and no other agent has it.
        let ready = CODEX_STARTING.replace(
            "Waiting for startup  \u{b7} esc cancel",
            "gpt-5.5 high \u{b7} 100% left \u{b7} ~/proj",
        );
        assert!(!holds_submission_until_started("codex", &ready));
        assert!(!holds_submission_until_started("claude", CODEX_STARTING));
        // A message that quotes the words sits in the input box, above the footer.
        let quoted = ready.replace(
            "  nothing else.",
            "  Waiting for startup  \u{b7} esc cancel\n  more\n  more\n  more",
        );
        assert!(!holds_submission_until_started("codex", &quoted));
    }
}
