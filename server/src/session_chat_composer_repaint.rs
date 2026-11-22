use std::time::Instant;

use crate::agents::identity::normalize_agent_id;
use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, wait_for_session_chat_composer,
    SessionChatComposerReadiness, SessionChatComposerState, SessionChatComposerWait,
    SessionChatComposerWaitPolicy, SESSION_CHAT_COMPOSER_POLL_MS,
};

use super::{
    capture_session_terminal_text, write_session_chat_payload, SessionChatSendError,
    SessionChatSendFailure,
};

/// CDXC:SessionChat 2026-09-23 WHY:
/// Claude can retain an empty prompt while its input borders and footer are partially erased, leaving every Send rejected against the same damaged screen.
/// This evidence permits a redraw inside the send worker, not permission to paste: the normal framed-composer and paste checks still have to succeed afterwards.
pub(crate) fn claude_composer_needs_redraw(
    agent: Option<&str>,
    readiness: &SessionChatComposerReadiness,
) -> bool {
    normalize_agent_id(agent).as_deref() == Some("claude")
        && readiness.state == SessionChatComposerState::NotReady
        && readiness.reason.as_deref() == Some("The claude input box is not on screen yet.")
        && readiness
            .screen_tail
            .iter()
            .rev()
            .find(|line| line.trim_start().starts_with('❯'))
            .is_some_and(|line| line.trim() == "❯")
}

/// Repaint only within a serialized message send, before clearing or pasting.
/// Claude's Ctrl+L forces a full redraw; replaying zmx's snapshot would only replay the damaged cells.
pub(super) async fn wait_for_send_composer(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    agent: Option<&str>,
    policy: SessionChatComposerWaitPolicy,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<SessionChatComposerWait, SessionChatSendError> {
    if source != "session-chat-message" || normalize_agent_id(agent).as_deref() != Some("claude") {
        return Ok(wait_for_session_chat_composer(zmx_name, agent, policy, cancelled).await);
    }
    let started = Instant::now();
    // Let an ordinary in-progress paint finish before asking Claude to redraw.
    let first = wait_for_session_chat_composer(
        zmx_name,
        agent,
        SessionChatComposerWaitPolicy {
            timeout_ms: policy.timeout_ms.min(2 * SESSION_CHAT_COMPOSER_POLL_MS),
            ..policy
        },
        cancelled,
    )
    .await;
    let SessionChatComposerWait::NotReady(ref readiness) = first else {
        return Ok(first);
    };
    if cancelled() {
        return Ok(SessionChatComposerWait::Cancelled);
    }
    if started.elapsed().as_millis() >= u128::from(policy.timeout_ms) {
        return Ok(first);
    }
    if claude_composer_needs_redraw(agent, readiness) {
        // Re-read the full screen immediately before the key, including dialog vetoes.
        if let Some(screen) = capture_session_terminal_text(zmx_name).await {
            let notice =
                crate::session_chat_notice::classify_session_chat_terminal_notice(agent, &screen);
            let current = detect_session_chat_composer_readiness(agent, &screen, notice.as_ref());
            if claude_composer_needs_redraw(agent, &current)
                && !notice.as_ref().is_some_and(|notice| notice.is_answerable())
            {
                if cancelled() {
                    return Ok(SessionChatComposerWait::Cancelled);
                }
                write_session_chat_payload(project_id, session_id, zmx_name, source, "\u{c}")
                    .await
                    .map_err(|message| {
                        SessionChatSendError::new(SessionChatSendFailure::Write, message)
                    })?;
            }
        }
    }
    let remaining = policy
        .timeout_ms
        .saturating_sub(started.elapsed().as_millis() as u64);
    let final_wait = wait_for_session_chat_composer(
        zmx_name,
        agent,
        SessionChatComposerWaitPolicy {
            settle_ms: SESSION_CHAT_COMPOSER_POLL_MS.min(remaining),
            timeout_ms: remaining,
            unknown_hold_ms: remaining,
        },
        cancelled,
    )
    .await;
    Ok(match final_wait {
        // Losing capture after a known incomplete screen is not evidence of recovery.
        SessionChatComposerWait::Unknown => first,
        other => other,
    })
}
