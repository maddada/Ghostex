//! CDXC:SessionChat 2026-09-16 DECISION:
//! User: when Codex says the conversation is open elsewhere, Continue here closes other Ghostex sessions with that conversation ID, then sends `r`. Command+Enter invokes it; recovery keeps the chat draft unsent.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{read_runtime_text, schedule_presentation_session_delta, AppState};
use crate::session_chat_notice::*;
use crate::session_chat_send::{
    capture_session_terminal_text, capture_session_terminal_text_vt, execute_session_chat_send,
    resolve_session_chat_send_target, write_session_chat_payload, SessionChatSendStep,
};
use crate::storage::open_gxserver_database;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockSession {
    pub project_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationLock {
    pub conversation_id: Option<String>,
    pub sessions: Vec<LockSession>,
}

/// CDXC:AgentScreenDetection 2026-09-16 WHY:
/// The lock text can occur in a pasted transcript. Require its dedicated footer at the live tail, with no composer after the heading, before offering a destructive recovery action.
pub(crate) fn is_locked(text: &str) -> bool {
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            crate::session_chat_options::normalize_spaces(
                &crate::session_chat_options::strip_ansi_sgr(line),
            )
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
        })
        .filter(|line| !line.is_empty())
        .collect();
    let Some(footer) = lines.iter().rposition(|line| is_lock_footer(line)) else {
        return false;
    };
    if footer + 1 != lines.len() {
        return false;
    }
    let start = footer.saturating_sub(5);
    let Some(heading) = (start..footer).rev().find(|&i| {
        let line = lines[i].trim_start_matches('🔒').trim();
        line.starts_with("This conversation is open in another app")
    }) else {
        return false;
    };
    lines[heading + 1..footer]
        .iter()
        .any(|line| line == "Close it there and press R to continue here.")
        && !lines[heading + 1..]
            .iter()
            .any(|line| line.starts_with(['›', '»']))
}

/// CDXC:AgentScreenDetection 2026-10-27 WHY: Codex builds this footer from parts. Since 0.157 it offers `f fork`, since 0.158 it replaces Escape's exit with `←/esc command center` when the command center is available, it drops the transcript hint when it has none, and since 0.161 it writes Control as `⌃` on macOS and `^` on Linux (openai/codex#49804). Codex 0.164 draws `r retry   f fork   esc/⌃c/q exit   ⌃t transcript`, so matching the older line `r retry esc/ctrl+c/q exit ctrl+t transcript` exactly never recognized the lock: chat offered no Continue here and the send waited on a composer Codex had replaced. The footer is recognized by its fixed parts, `r retry` first and a Ctrl+C exit, in any modifier spelling.
fn is_lock_footer(line: &str) -> bool {
    let line = line
        .to_lowercase()
        .replace('⌃', "ctrl+")
        .replace('^', "ctrl+");
    let words: Vec<_> = line.split_whitespace().collect();
    words.starts_with(&["r", "retry"])
        && words
            .windows(2)
            .any(|pair| pair[1] == "exit" && pair[0].split('/').any(|key| key == "ctrl+c"))
}

pub(crate) fn notice() -> SessionChatTerminalNotice {
    let mut notice = SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_CODEX_INPUT_BLOCKED,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        "Conversation open elsewhere",
    )
    .with_detail("Close this conversation in the other app, then retry. Your draft stays here.");
    notice.conversation_lock = Some(ConversationLock {
        conversation_id: None,
        sessions: Vec::new(),
    });
    notice.actions = vec![
        SessionChatTerminalNoticeAction {
            id: "recoverCodexConversation".into(),
            label: "Retry".into(),
            kind: SessionChatTerminalNoticeActionKind::RecoverCodexConversation,
            send: None,
        },
        SessionChatTerminalNoticeAction::switch_to_terminal("Open terminal"),
    ];
    notice
}

fn conversation_id(session: &Value) -> Option<String> {
    (crate::session_chat_composer::session_chat_composer_agent_id(session).as_deref()
        == Some("codex"))
    .then(|| read_runtime_text(session, "agentSessionId"))
    .flatten()
    .filter(|id| !id.trim().is_empty())
}

fn matching_sessions(
    repository: &DomainRepository<'_>,
    current: &Value,
) -> Result<Vec<LockSession>, DomainStateError> {
    let Some(id) = conversation_id(current) else {
        return Ok(Vec::new());
    };
    let current_provider = crate::zmx::provider_zmx_session_name(current).ok();
    Ok(repository
        .list_sessions(None)?
        .into_iter()
        .filter(|session| {
            conversation_id(session).as_ref() == Some(&id)
                && crate::presentation::effective_lifecycle_state(session) == "running"
                && session.get("sessionId") != current.get("sessionId")
                && crate::zmx::provider_zmx_session_name(session)
                    .ok()
                    .is_some_and(|name| Some(name) != current_provider)
        })
        .filter_map(|session| {
            Some(LockSession {
                project_id: session.get("projectId")?.as_str()?.to_string(),
                session_id: session.get("sessionId")?.as_str()?.to_string(),
            })
        })
        .collect())
}

pub(crate) fn enrich_notice(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    notice: &mut SessionChatTerminalNotice,
) {
    let Some(lock) = notice.conversation_lock.as_mut() else {
        return;
    };
    let Ok(Some(current)) = repository.get_session(project_id, session_id) else {
        return;
    };
    lock.conversation_id = conversation_id(&current);
    let Ok(sessions) = matching_sessions(repository, &current) else {
        return;
    };
    lock.sessions = sessions;
    if !lock.sessions.is_empty() {
        let count = lock.sessions.len();
        let noun = if count == 1 { "session" } else { "sessions" };
        let reference = if count == 1 {
            "that session"
        } else {
            "those sessions"
        };
        notice.detail = Some(format!("Close {count} other Ghostex {noun} using this conversation and continue here. This stops any work running in {reference}. Your draft stays here."));
        notice.actions[0].label = "Continue here".into();
    }
}

fn invalid(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "invalidState",
        message: message.into(),
    }
}

// One takeover per conversation, even when two blocked clients click together.
static RECOVERING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
struct RecoveryGuard(String);
impl Drop for RecoveryGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = RECOVERING.get_or_init(Default::default).lock() {
            active.remove(&self.0);
        }
    }
}

pub(crate) async fn recover(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let requested: ConversationLock = serde_json::from_value(
        params
            .get("conversationLock")
            .cloned()
            .unwrap_or(Value::Null),
    )
    .map_err(|_| {
        invalid("The conversation lock details are missing. Refresh the notice and retry.")
    })?;
    let target = resolve_session_chat_send_target(state, params, "recoverCodexConversation")?;
    let key = requested
        .conversation_id
        .clone()
        .unwrap_or_else(|| target.zmx_name.clone());
    {
        let mut active = RECOVERING
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| invalid("Recovery is unavailable."))?;
        if !active.insert(key.clone()) {
            return Err(invalid(
                "This conversation is already being recovered. Wait for it to finish.",
            ));
        }
    }
    let _guard = RecoveryGuard(key);
    let blocking_state = state.clone();
    let blocking_params = params.clone();
    tokio::task::spawn_blocking(move || {
        let target = resolve_session_chat_send_target(
            &blocking_state,
            &blocking_params,
            "recoverCodexConversation",
        )?;
        if crate::session_chat_composer::session_chat_composer_agent_id(&target.session).as_deref()
            != Some("codex")
            || conversation_id(&target.session) != requested.conversation_id
        {
            return Err(invalid(
                "The Codex conversation changed. Refresh the notice and retry.",
            ));
        }
        let db = open_gxserver_database(&blocking_state.paths)
            .map_err(|error| invalid(error.to_string()))?;
        let repository = DomainRepository::new(&db, &blocking_state.metadata.server_id);
        let screen = crate::zmx::read_zmx_session_history_capture(
            &repository,
            &target.project_id,
            &target.session_id,
        )
        .map_err(|_| invalid("Could not read the Codex terminal. No sessions were closed."))?;
        if screen.truncated || !is_locked(&screen.text) {
            return Err(invalid("The conversation lock is no longer on screen."));
        }
        let matches = matching_sessions(&repository, &target.session)?;
        // The Retry variant carries no close targets. A newly discovered instance needs a fresh notice and explicit Continue here.
        if matches
            .iter()
            .any(|session| !requested.sessions.contains(session))
        {
            return Err(invalid(
                "Another matching session appeared. Review the updated notice before continuing.",
            ));
        }
        for session in matches {
            let current = repository
                .get_session(&session.project_id, &session.session_id)?
                .ok_or_else(|| invalid("The other session changed. Retry recovery."))?;
            if conversation_id(&current) != requested.conversation_id {
                return Err(invalid(
                    "The other session changed conversations. Retry recovery.",
                ));
            }
            crate::session_chat_send::cancel_session_chat_sends(
                &session.project_id,
                &session.session_id,
            );
            let lifecycle = crate::zmx::LifecycleParams {
                project_id: session.project_id.clone(),
                session_id: session.session_id.clone(),
            };
            let (kill, _) =
                crate::zmx::kill_and_cache_session_provider(&repository, &lifecycle, "stopped")
                    .map_err(|error| {
                        invalid(format!("Could not close the other session: {error:?}"))
                    })?;
            schedule_presentation_session_delta(
                &blocking_state,
                &db,
                &repository,
                &session.project_id,
                &session.session_id,
            )?;
            if !kill.killed {
                return Err(invalid(
                    kill.error
                        .unwrap_or_else(|| "Could not close the other session.".into()),
                ));
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let (probe, _, _, _) =
                    crate::zmx::probe_and_cache_session_provider(&repository, &lifecycle).map_err(
                        |error| {
                            invalid(format!(
                                "Could not verify the other session closed: {error:?}"
                            ))
                        },
                    )?;
                if probe.lifecycle_state == "missing" {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(invalid(
                        "The other session has not exited yet. Retry when it closes.",
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Ok(())
    })
    .await
    .map_err(|error| invalid(error.to_string()))??;
    let result = execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "codex-conversation-recovery",
        vec![SessionChatSendStep::RetryCodexConversation],
    )
    .await;
    result.map_err(|error| invalid(error.message))
}

pub(crate) async fn retry(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    cancelled: &(impl Fn() -> bool + Sync),
) -> Result<(), String> {
    let screen = capture_session_terminal_text(zmx_name)
        .await
        .ok_or("Could not read the Codex terminal.")?;
    if cancelled() || !is_locked(&screen) {
        return Err("The conversation lock is no longer on screen. Retry was not sent.".into());
    }
    write_session_chat_payload(project_id, session_id, zmx_name, source, "r").await?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if cancelled() {
            return Err("Conversation recovery was cancelled.".into());
        }
        if let Some(screen) = capture_session_terminal_text_vt(zmx_name).await {
            if crate::session_chat_composer::detect_session_chat_composer_ready(
                Some("codex"),
                &screen,
            )
            .state
                == crate::session_chat_composer::SessionChatComposerState::Ready
            {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err("Codex is not ready yet. If the conversation is still open in another app, close it there and retry. Your draft has not been sent.".into());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::is_locked;

    /// Codex 0.164.0 on macOS, captured live from a second `codex resume` of a running conversation.
    const LOCKED_0164: &str = "• Hi!\n\n  Worked for 4s • 1:16 AM\n\n\n  🔒  This conversation is open in another app                                                                    r to retry\n      Close it there and press R to continue here.\n\n   r retry   f fork   esc/⌃c/q exit   ⌃t transcript\n";

    #[test]
    fn recognizes_codex_0164_lock_screen() {
        assert!(is_locked(LOCKED_0164));
    }

    #[test]
    fn recognizes_every_footer_variant() {
        for footer in [
            // Before 0.157.
            "r retry   esc/ctrl+c/q exit   ctrl+t transcript",
            // Command center available (0.158), Linux and Windows spellings, no transcript hint.
            "r retry   f fork   ←/esc command center   ⌃c/q exit   ⌃t transcript",
            "r retry   f fork   esc/^c/q exit   ^t transcript",
            "r retry   f fork   esc/ctrl+c/q exit   ctrl+t transcript",
            "r retry   f fork   esc/⌃c/q exit",
        ] {
            let screen =
                LOCKED_0164.replace("r retry   f fork   esc/⌃c/q exit   ⌃t transcript", footer);
            assert!(is_locked(&screen), "{footer}");
        }
    }

    #[test]
    fn ignores_the_lock_text_once_the_composer_is_back_or_quoted() {
        // The footer must be the last line: the composer returning below it ends the lock.
        assert!(!is_locked(&format!(
            "{LOCKED_0164}\n› Ask Codex to do anything\n"
        )));
        // Without its footer the heading is only transcript text.
        assert!(!is_locked(
            "  🔒  This conversation is open in another app\n      Close it there and press R to continue here.\n"
        ));
        // A footer that is not the lock's.
        assert!(!is_locked(&LOCKED_0164.replace(
            "r retry   f fork   esc/⌃c/q exit   ⌃t transcript",
            "enter submit   esc cancel"
        )));
    }
}
