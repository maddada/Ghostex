//! The writes of a chat message's text and Return, each made only when a fresh read of the agent's
//! screen shows its input box ready for it and nothing waiting on an answer.

use super::*;

/// What a guarded write needs on screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputGuard {
    /// The agent's input box, ready, with no prompt waiting on an answer: before the message's text.
    InputBox,
    /// The same, with the input box holding this message: before its Return.
    Holds(String),
}

/// Why a send stopped before typing into a prompt that appeared under it. It never ends with
/// "Answer it in chat before sending." (`holds_until_agent_takes_input`), so a chat box send is
/// kept as a queued message and a queued row is put back, both sent once the prompt is answered.
pub(crate) const SESSION_CHAT_PROMPT_APPEARED: &str = "The agent asked something in its terminal just as your message was going out, so nothing was typed into it. Your message goes out once that is answered.";

/// Whether a send's error is the guard refusing to type into a prompt that appeared under it.
pub(crate) fn refused_for_prompt(error: &crate::domain::DomainStateError) -> bool {
    error.code == "composerNotReady" && error.message == SESSION_CHAT_PROMPT_APPEARED
}

/// The agents whose input box the send reads back (the paste check's `composer_agent`).
pub(crate) fn send_reads_input_box(agent: Option<&str>) -> Option<String> {
    crate::agents::identity::normalize_agent_id(agent).filter(|agent| {
        matches!(
            agent.as_str(),
            "claude" | "openclaude" | "codex" | "grok" | "empryo"
        )
    })
}

/// Whether `screen` (`None` when it could not be read whole) allows the write `guard` covers, or
/// why not.
///
/// CDXC:SessionChat 2026-11-12 WHY: GitHub #555: the queue checks that nothing waits on the user
/// before it starts a row, and the send checked the input box before the paste and the paste before
/// the Return. An approval that appeared after the last of those reads took the Return as its
/// answer (Claude Code's permission dialog highlights Yes), and one that appeared before the paste
/// took the text, where a lone digit picks an option. That hit the user's own queued rows as much
/// as agents' messages. The text and the Return are now each written only after a read of the same
/// screen shows the input box ready (`detect_session_chat_composer_readiness`, which every agent's
/// dialog, panel and blocking notice vetoes) and, for the Return, still holding the message; the
/// read and the write share one daemon connection (`send_after_screen_check`). A refused write
/// leaves the message waiting: the queue holds it while the prompt is answered, and its next try
/// clears the input box before typing it again. An unreadable screen keeps the paste check's rule:
/// the agents whose input box the send reads need it on screen, the others are written to.
pub(crate) fn guarded_write_refusal(
    agent: Option<&str>,
    guard: &InputGuard,
    screen: Option<&str>,
) -> Result<(), String> {
    let reads_box = send_reads_input_box(agent);
    let Some(screen) = screen else {
        return match reads_box {
            Some(_) => Err("The agent's screen could not be read before typing.".to_string()),
            None => Ok(()),
        };
    };
    let plain = crate::session_chat_options::strip_ansi_sgr(screen);
    // The input-box wait reads Codex from its styled capture and every other agent from plain text.
    let shown = if reads_box.as_deref() == Some("codex") {
        screen
    } else {
        plain.as_str()
    };
    let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(agent, shown);
    let readiness = crate::session_chat_composer::detect_session_chat_composer_readiness(
        agent,
        shown,
        notice.as_ref(),
    );
    if readiness.blocks_message_for(agent) {
        return Err(readiness
            .reason
            .unwrap_or_else(|| "The agent's input box is not on screen.".to_string()));
    }
    if let InputGuard::Holds(text) = guard {
        let needles = session_chat_paste_needles(text);
        if !needles.is_empty() {
            let shown = match reads_box.as_deref() {
                Some(agent) => session_chat_input_box_text(agent, screen),
                None => screen.to_string(),
            };
            if !paste_shown_in(reads_box.as_deref(), &shown, &needles) {
                return Err("The input box no longer shows the message.".to_string());
            }
        }
    }
    Ok(())
}

/// When an approval or question card last arrived by hook, per session.
fn prompt_card_arrivals() -> &'static Mutex<HashMap<String, Instant>> {
    static ARRIVALS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    ARRIVALS.get_or_init(Mutex::default)
}

/// How long an arrival is kept: far longer than any send's own run.
const PROMPT_CARD_ARRIVAL_KEEP: Duration = Duration::from_secs(15 * 60);

/// Notes that the session's hooks just stored `stored` as its card, when it is an approval or a
/// question (`ingest_agent_hook_event`).
///
/// CDXC:SessionChat 2026-11-12 WHY: GitHub #555: a screen read cannot see an approval the agent
/// has decided to show but not yet drawn, and a Return written then is still taken as its answer
/// (reproduced with a stand-in agent whose dialog and the Return landed in the same millisecond).
/// Claude Code and the other hook agents run their PermissionRequest hook, which waits for
/// gxserver to store the card, before they show the dialog, so a card that arrived since a send
/// began refuses its guarded writes even while the screen still shows the input box.
pub(crate) fn note_session_chat_prompt_card(
    project_id: &str,
    session_id: &str,
    stored: Option<&str>,
) {
    let prompt = stored.and_then(crate::session_chat_interactive::parse_stored_session_chat_prompt);
    if !matches!(
        prompt,
        Some(
            crate::session_chat_interactive::SessionChatInteractivePrompt::Approval { .. }
                | crate::session_chat_interactive::SessionChatInteractivePrompt::Question { .. }
        )
    ) {
        return;
    }
    if let Ok(mut arrivals) = prompt_card_arrivals().lock() {
        let now = Instant::now();
        arrivals.retain(|_, at| now.duration_since(*at) < PROMPT_CARD_ARRIVAL_KEEP);
        arrivals.insert(format!("{project_id}:{session_id}"), now);
    }
}

/// Whether an approval or question card arrived by hook at or after `since`.
fn prompt_card_arrived_since(project_id: &str, session_id: &str, since: Instant) -> bool {
    prompt_card_arrivals().lock().is_ok_and(|arrivals| {
        arrivals
            .get(&format!("{project_id}:{session_id}"))
            .is_some_and(|at| *at >= since)
    })
}

const PROMPT_CARD_ARRIVED: &str =
    "The agent's hooks reported an approval or a question while the message was going out.";

/// Writes `payload` for a chat send only when a fresh read of the screen passes `guard` and no
/// approval or question card arrived by hook since `since` (the send's start); a refusal is
/// `ComposerNotReady` with `SESSION_CHAT_PROMPT_APPEARED`, and nothing is written.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_session_chat_payload_guarded(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    agent: Option<&str>,
    guard: &InputGuard,
    payload: &str,
    since: Instant,
) -> Result<(), SessionChatSendError> {
    #[cfg(test)]
    if let Some(screen) = test_terminal::capture(zmx_name) {
        let verdict = guarded_write_refusal(agent, guard, screen.as_deref()).and_then(|()| {
            if prompt_card_arrived_since(project_id, session_id, since) {
                Err(PROMPT_CARD_ARRIVED.to_string())
            } else {
                Ok(())
            }
        });
        if let Err(reason) = verdict {
            return Err(prompt_appeared(
                project_id,
                session_id,
                &reason,
                screen.as_deref(),
            ));
        }
        return write_session_chat_payload(project_id, session_id, zmx_name, source, payload)
            .await
            .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message));
    }
    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        zmx_name,
        "sessionChatQueueWrite",
        source,
        payload,
    );
    let vt = send_reads_input_box(agent).is_some();
    let (name, bytes, check_agent, check_guard) = (
        zmx_name.to_string(),
        payload.to_string(),
        agent.map(str::to_string),
        guard.clone(),
    );
    let (project, session) = (project_id.to_string(), session_id.to_string());
    let sent = tokio::task::spawn_blocking(move || {
        let mut refused_screen = None;
        let sent = crate::zmx::send_after_screen_check(&name, vt, &bytes, |screen| {
            // The hook check runs after the screen read, right before the write.
            let verdict = guarded_write_refusal(check_agent.as_deref(), &check_guard, screen)
                .and_then(|()| {
                    if prompt_card_arrived_since(&project, &session, since) {
                        Err(PROMPT_CARD_ARRIVED.to_string())
                    } else {
                        Ok(())
                    }
                });
            if verdict.is_err() {
                refused_screen = screen.map(str::to_string);
            }
            verdict
        });
        (sent, refused_screen)
    })
    .await;
    match sent {
        Ok((Ok(crate::zmx::ZmxCheckedSend::Sent), _)) => {
            note_session_chat_submit(project_id, session_id, payload);
            Ok(())
        }
        Ok((Ok(crate::zmx::ZmxCheckedSend::Refused(reason)), screen)) => Err(prompt_appeared(
            project_id,
            session_id,
            &reason,
            screen.as_deref(),
        )),
        Ok((Ok(crate::zmx::ZmxCheckedSend::NotTaken(error)), _)) => {
            eprintln!("[gxserver] chat input for {project_id}/{session_id} refused ({error})");
            Err(SessionChatSendError::new(
                SessionChatSendFailure::Write,
                TERMINAL_REFUSED.to_string(),
            ))
        }
        Ok((Err(error), _)) => Err(SessionChatSendError::new(
            SessionChatSendFailure::Write,
            error,
        )),
        Err(error) => {
            eprintln!("[gxserver] chat input task for {project_id}/{session_id} failed: {error}");
            Err(SessionChatSendError::new(
                SessionChatSendFailure::Write,
                TERMINAL_REFUSED.to_string(),
            ))
        }
    }
}

/// Logs what the refused write saw and answers the error that holds the message.
fn prompt_appeared(
    project_id: &str,
    session_id: &str,
    reason: &str,
    screen: Option<&str>,
) -> SessionChatSendError {
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatSendHeldForPrompt",
        project_id,
        session_id,
        reason,
        &screen
            .map(crate::session_chat_composer::composer_terminal_preview_tail)
            .unwrap_or_default(),
    );
    SessionChatSendError::new(
        SessionChatSendFailure::ComposerNotReady,
        SESSION_CHAT_PROMPT_APPEARED.to_string(),
    )
}
