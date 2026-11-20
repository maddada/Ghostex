//! `/api/clearSessionConversation` (`ghostex clear-command`): starts a session's conversation over
//! with its agent's own clear command, in the same session (same name, place and ref).
//!
//! CDXC:Security 2026-11-20 DECISION (GitHub #685, Q4272 A):
//! 11.11.1 stopped agents from writing into another session's chat as the user (sender_mark.rs):
//! an agent's text there always carries its sender block, first for a `/` or `!` body, so it
//! never runs as a command. That also ended the one way a lead session reset a session it reuses
//! (`send-session-chat-message <ref> "/clear"` with its identity removed). This route gives that
//! reset back without reopening what 11.11.1 closed:
//! - The caller sends no text at all. gxserver picks the command from the session's agent (`/clear`
//!   for Claude Code and Codex; every other agent is refused), so this is never a way to
//!   run an arbitrary command or to put words in the user's mouth, and nothing is recorded as the
//!   user's own send (no `note_user_send`, so it settles no coordinator ask).
//! - It refuses while the session works, waits on the user (a permission prompt, a question, a
//!   Needs you to-do, a stored prompt, a connector card, any dialog in front of its input box),
//!   holds queued messages, or has unsent text in its input box. A typed command ends with Enter,
//!   and Enter on a permission prompt answers it, so a waiting session is never typed into.
//! - Who may call it, decided from the caller's token, never from the text or the CLI's
//!   environment: you (the desktop app, a paired phone, and the token file: the `ghostex` CLI in
//!   your own terminal outside Ghostex, Grok Bot, the phone over SSH) and any agent's session
//!   token, for any session on this computer. User (Q4280, after the Fable review of 6d5889f7f
//!   found the first rule, "only a coordinator thread it leads", was one an agent could grant
//!   itself by promoting itself and linking any session): "let agents clear sessions, why block
//!   this". It is the reach `send-text` already gives agents, with the checks above on top. A
//!   session can't clear itself (it is mid-turn whenever it runs a command). Connected computers'
//!   agents (peers), Collab guests and extensions never reach the route (`full_local` in
//!   protocol.rs; peers have their own allow-list), and a GBot API chat stays the owner's
//!   (`SESSION_INPUT_ROUTES` in security_gates.rs). Every clear is in the security log with its
//!   caller (`chat.clearConversation`).
//! - The CLI refuses to fall back to the token file inside a Ghostex session pane whose variables
//!   are missing, the same check 11.11.1 added for the raw message verbs, so an agent can't pass
//!   itself off as your terminal by dropping its variables.
//! What this does not change: raw terminal input into another session (`send-text`, `send-key`)
//! stays open to agents by the 2026-11-08 decision in security/policy.rs; this route is the
//! supported, checked path, not the only gate.
//! SEE-ALSO: server/src/session_chat_send/sender_mark.rs, server/src/server/security_gates.rs,
//! server/src/ghostex_cli/clear_command.rs.

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::security::Principal;

use super::AppState;

pub(crate) const CLEAR_SESSION_ENDPOINT: &str = "/api/clearSessionConversation";

/// The refusal for a screen that can't take the command yet (no input box drawn, a screen that
/// can't be read): the only one `ghostex clear-command` asks again on after waking a session, so a
/// woken session that turns out to be working or holding messages is refused, not cleared once it
/// goes idle (Fable review of 6d5889f7f, finding 6).
pub(crate) const INPUT_BOX_NOT_READY: &str = "inputBoxNotReady";

const WRITES_PENDING: &str = "Another write to its terminal (typed keys or a message) is still waiting or running. Try again once it is done.";

fn refused(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// The agent's own command that starts a new conversation in the same process.
fn clear_command_for(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("/clear"),
        // Not `/new`: Codex 6 answers `/new` in a Git folder with a "Where should the new
        // conversation run?" picker (current folder or a new worktree), which stopped the clear
        // there; its `/clear` starts the new conversation at once (seen 2026-11-20).
        "codex" => Some("/clear"),
        _ => None,
    }
}

/// Why `caller` may not clear session `session_id` of `project_id`, or `None` when it may.
fn caller_refusal(caller: &Principal, project_id: &str, session_id: &str) -> Option<&'static str> {
    match caller {
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent => None,
        Principal::Session {
            project_id: own_project,
            session_id: own_session,
            ..
        } => (own_project == project_id && own_session == session_id).then_some(
            "A session can't clear its own conversation: it is in the middle of a turn whenever it runs a command. Ask the user, or another session, to clear it once this turn ends.",
        ),
        _ => Some("Only you and the agents on this computer can clear a session's conversation."),
    }
}

/// Why the session can't be cleared right now, or `None`.
fn busy_refusal(
    state: &AppState,
    db: &rusqlite::Connection,
    session: &Value,
    project_id: &str,
    session_id: &str,
    running: bool,
) -> Option<String> {
    let now = crate::presentation::now_iso();
    if crate::session_chat_queue::session_has_pending_session_chat_queue(db, project_id, session_id)
    {
        return Some("It has queued messages; they would go into the new conversation. Send or remove them first.".into());
    }
    if crate::presentation::turn_end_waits_on_user(session, &now) {
        return Some("It is waiting on the user (a permission prompt, a question, a Needs you to-do or a card). Answer or dismiss it first.".into());
    }
    if !running {
        return None;
    }
    // A `send-text`, `send-key` or message still waiting or running in its terminal's queue would
    // go in first; the clear's own job checks this again as it is queued (queue.rs
    // `execute_session_chat_send_if_idle`).
    if crate::session_chat_send::session_chat_send_writes_pending(project_id, session_id) {
        return Some(WRITES_PENDING.into());
    }
    let activity = crate::presentation::presentation_activity(session, &now);
    if activity == "working" && !crate::presentation::working_only_in_background(session, &now) {
        return Some("It is working. Try again once its turn ends.".into());
    }
    if activity == "attention"
        && crate::presentation::attention_state(session, &now)
            .get("kind")
            .and_then(Value::as_str)
            != Some("finished")
    {
        return Some("It is waiting on the user. Answer it first.".into());
    }
    if crate::session_chat_options::cached_session_chat_terminal_notice(
        state, project_id, session_id,
    )
    .is_some_and(|notice| notice.blocks_queued_delivery())
    {
        return Some(
            "A dialog or notice is open in its terminal. Answer or close it first.".into(),
        );
    }
    None
}

/// Why the session's screen, read now, can't take the command: its input box is not drawn and
/// ready (a session that just woke and is still resuming, a dialog), or it holds text someone typed
/// and has not sent. Read fresh rather than from the observers' cache, which still said "ready"
/// for a session woken a moment ago, so the typed command was lost while Codex resumed (seen
/// 2026-11-20).
fn screen_refusal(
    state: &AppState,
    repository: &DomainRepository<'_>,
    agent: &str,
    project_id: &str,
    session_id: &str,
) -> Option<DomainStateError> {
    let not_ready = |message: String| {
        Some(refused(
            INPUT_BOX_NOT_READY,
            format!("The conversation was not cleared. {message}"),
        ))
    };
    let capture =
        match crate::zmx::read_zmx_session_history_capture_vt(repository, project_id, session_id) {
            Ok(capture) if !capture.truncated => capture,
            _ => return not_ready("Its screen could not be read. Try again in a moment.".into()),
        };
    let notice = crate::session_chat_options::cached_session_chat_terminal_notice(
        state, project_id, session_id,
    );
    let composer = crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some(agent),
        &capture.text,
        notice.as_ref(),
    );
    if composer.state != crate::session_chat_composer::SessionChatComposerState::Ready {
        return not_ready(match composer.reason {
            Some(reason) => format!("Its input box is not ready: {reason}"),
            None => "Its input box is not ready (it may still be starting).".into(),
        });
    }
    // A box that can't be read counts as holding a draft, not as empty: the job's clear burst
    // would erase whatever is there (Fable review of 6d5889f7f, finding 7; the coordinator
    // runtime's Empryo role check reads its box the same way).
    let input = crate::session_chat_composer::session_chat_composer_input(agent, &capture.text);
    match input {
        None => {
            return not_ready("Its input box could not be read. Try again in a moment.".into())
        }
        Some(input) if input.text_unreadable() => {
            return not_ready("The text in its input box could not be read, so it may hold a draft. Try again in a moment.".into())
        }
        Some(input) if !input.is_empty() => {
            return Some(refused(
                "agentBusy",
                "The conversation was not cleared. Its input box holds unsent text. Send or remove it first.",
            ))
        }
        Some(_) => {}
    }
    None
}

/// What the route does once its checks pass: answer that the session is waking, or type the
/// command and answer with `result`.
enum Prepared {
    Waking(Value),
    Type {
        job: crate::zmx::ZmxQueuedSessionMessage,
        result: Value,
    },
}

fn prepare_clear(
    state: &AppState,
    caller: &Principal,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Prepared, DomainStateError> {
    let project_id = text(params, "projectId")
        .ok_or_else(|| DomainStateError::bad_request("clear-command needs a projectId."))?;
    let session_id = text(params, "sessionId")
        .ok_or_else(|| DomainStateError::bad_request("clear-command needs a sessionId."))?;
    if let Some(message) = caller_refusal(caller, project_id, session_id) {
        return Err(refused("forbidden", message));
    }
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Session {session_id} was not found."))
        })?;
    let lifecycle = crate::presentation::effective_lifecycle_state(&session);
    if !matches!(lifecycle.as_str(), "running" | "sleeping") {
        return Err(refused(
            "conflict",
            "The session is closed, so there is no conversation to clear.",
        ));
    }
    let agent =
        crate::session_chat_follower::session_chat_agent_for_session(&session).unwrap_or_default();
    let Some(command) = clear_command_for(&agent) else {
        return Err(refused(
            "unsupportedAgent",
            "clear-command works for Claude Code and Codex sessions only.",
        ));
    };
    let lifecycle_params = crate::zmx::LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let daemon_missing =
        crate::zmx::probe_and_cache_session_provider(repository, &lifecycle_params)
            .is_ok_and(|(probe, ..)| probe.lifecycle_state == "missing");
    if let Some(reason) = busy_refusal(state, db, &session, project_id, session_id, !daemon_missing)
    {
        return Err(refused(
            "agentBusy",
            format!("The conversation was not cleared. {reason}"),
        ));
    }
    if let Some(refusal) = (!daemon_missing)
        .then(|| screen_refusal(state, repository, &agent, project_id, session_id))
        .flatten()
    {
        return Err(refusal);
    }
    if daemon_missing {
        // A sleeping session is only woken here, and the caller asks again once it is up. Codex
        // resumed onto a new conversation id before a /clear queued for its start ran, so the id
        // the CLI waited for was the wake's, not the clear's (seen 2026-11-20); typed into a
        // running session only, the clear is always the change the caller sees.
        let owned = state.clone();
        let (pid, sid) = (project_id.to_string(), session_id.to_string());
        tokio::spawn(async move {
            use crate::session_chat_send_wake::{wake_session, WakeOutcome};
            if let WakeOutcome::Blocked(reason) | WakeOutcome::Failed(reason) =
                wake_session(&owned, &pid, &sid).await
            {
                eprintln!("[gxserver] clear-command: could not wake {pid}:{sid}: {reason}");
            }
        });
        return Ok(Prepared::Waking(json!({
            "ok": true,
            "projectId": project_id,
            "sessionId": session_id,
            "agent": agent,
            "command": command,
            "delivery": "waking",
        })));
    }
    let previous = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut send = Map::new();
    send.insert("projectId".to_string(), json!(project_id));
    send.insert("sessionId".to_string(), json!(session_id));
    send.insert(
        "diagnosticInputSource".to_string(),
        json!("session-clear-command"),
    );
    send.insert("submit".to_string(), Value::Bool(true));
    send.insert("text".to_string(), json!(command));
    let job = crate::zmx::read_zmx_queued_session_message(repository, &send).map_err(zmx_error)?;
    Ok(Prepared::Type {
        job,
        result: json!({
            "ok": true,
            "projectId": project_id,
            "sessionId": session_id,
            "agent": agent,
            "command": command,
            "delivery": "sent",
            "previousAgentSessionId": (!previous.is_empty()).then_some(previous),
        }),
    })
}

fn zmx_error(error: crate::zmx::ZmxEndpointError) -> DomainStateError {
    match error {
        crate::zmx::ZmxEndpointError::Domain(error) => error,
        crate::zmx::ZmxEndpointError::DependencyUnavailable(message) => {
            refused("dependencyUnavailable", message)
        }
    }
}

/// The refusal for a typing job the send guard stopped, or that failed.
fn send_error(error: crate::session_chat_send::SessionChatSendError) -> DomainStateError {
    use crate::session_chat_send::SessionChatSendFailure as Failure;
    if error.writes_pending() {
        return refused(
            "agentBusy",
            format!("The conversation was not cleared. {WRITES_PENDING}"),
        );
    }
    // The guard's own words ("Your message goes out once that is answered") are for chat sends,
    // which wait; this job never runs again, so they are not passed on.
    let reason = match error.failure {
        Failure::PromptWaiting => "a prompt came up in its terminal",
        Failure::ComposerNotReady => "its input box was not showing",
        Failure::ComposerNotCleared => "its input box could not be emptied",
        _ => return refused("dependencyUnavailable", error.message),
    };
    refused(
        if error.failure == Failure::ComposerNotReady {
            INPUT_BOX_NOT_READY
        } else {
            "agentBusy"
        },
        format!(
            "The conversation was not cleared: {reason} just before the command was typed, so nothing was typed. Nothing will be typed later."
        ),
    )
}

/// The security log entry for a clear, with its caller (the request's actor: the session, the
/// token file, or your own device), so a clear an agent or the token file did is on record (Fable
/// review of 6d5889f7f, finding 5).
fn record_clear(
    state: &AppState,
    caller: &Principal,
    project_id: &str,
    session_id: &str,
    command: &str,
) {
    let target = crate::ids::create_global_session_ref(
        state.metadata.server_id.as_str(),
        project_id,
        session_id,
    );
    crate::audit::record(
        crate::audit::Action::new(
            "chat.clearConversation",
            format!("Cleared {target}'s conversation with its {command} (ghostex clear-command)"),
        )
        .target(target.clone()),
        crate::audit::Actor::current(),
        crate::audit::Outcome::Done,
        None,
    );
    eprintln!(
        "[gxserver] clear-command: {target} {command} by {}",
        caller.kind()
    );
}

/// `/api/clearSessionConversation`. The checks run off the async workers; the command is then
/// typed by an awaited job that is never run again (Fable review of 6d5889f7f, finding 3: the
/// queued job with its retry step answered "sent" before typing, and a job a prompt refused typed
/// `/clear` and Enter up to 30 minutes later, after the caller had been told it failed).
pub(crate) async fn handle_clear_session_http(
    state: &AppState,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> super::RoutedResponse {
    let prepared = super::http_dispatch::blocking_section(|| {
        let params = crate::domain::read_domain_rpc_params(body)?;
        let db = crate::storage::open_gxserver_database(&state.paths).map_err(|error| {
            DomainStateError {
                code: "internalError",
                message: format!("SQLite gxserver state error: {error}"),
            }
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        prepare_clear(state, caller, &db, &repository, &params)
    });
    let outcome = match prepared {
        Ok(Prepared::Waking(result)) => Ok(result),
        Ok(Prepared::Type { job, result }) => match job.execute_once().await {
            Ok(_) => {
                let (project_id, session_id, command) = (
                    result["projectId"].as_str().unwrap_or_default(),
                    result["sessionId"].as_str().unwrap_or_default(),
                    result["command"].as_str().unwrap_or_default(),
                );
                crate::session_chat_app_command::record_session_chat_app_command(
                    project_id, session_id, command,
                );
                record_clear(state, caller, project_id, session_id, command);
                Ok(result)
            }
            Err(error) => Err(send_error(error)),
        },
        Err(error) => Err(error),
    };
    match outcome {
        Ok(result) => super::routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(request_id, result),
        ),
        Err(error) => {
            // A refusal of the session's state is a conflict, not a server error, which
            // `domain_error_response` makes of a code it does not know.
            if matches!(
                error.code,
                "agentBusy" | INPUT_BOX_NOT_READY | "unsupportedAgent"
            ) {
                super::routed_json(
                    Some(endpoint_path),
                    axum::http::StatusCode::CONFLICT,
                    crate::protocol::rpc_error(error.code, error.message, Some(request_id)),
                )
            } else {
                super::http_endpoints::domain_error_response(endpoint_path, request_id, error)
            }
        }
    }
}
