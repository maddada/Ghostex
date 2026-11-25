//! Rewind in a Native chat, where the agent runs inside `ghostex acp-host` instead of a terminal.
//!
//! CDXC:SessionChat 2026-11-24 DECISION:
//! User (Q102): build Rewind for Native chats for Claude Code, Codex and Antigravity where possible. Claude Code goes back by restarting at the message before the prompt (acp/driver_claude_rewind.rs), Codex through its app-server's `thread/revert` (acp/driver_codex_rewind.rs); both leave files as they are, like Terminal's rewind. Antigravity's `agy acp` (1.5.16) has no way to go back (no fork, truncate or rewind method, no rewind in the CLI), so its chats keep Rewind hidden (`agent_supports_rewind` in packages/gx-chat-core/src/transcript/presentation.rs).

use super::codex;
use super::*;
use crate::domain::DomainRepository;
use crate::pi_chat_channel::ChannelError;
use crate::session_chat_send::SessionChatSendTarget;
use crate::storage::open_gxserver_database;

/// Restarting Claude Code (or reading Codex's turn pages) can take a while on a long chat.
const NATIVE_REWIND_TIMEOUT: Duration = Duration::from_secs(90);

pub(super) async fn rewind(
    state: &AppState,
    target: SessionChatSendTarget,
    message_id: &str,
    agent: &str,
) -> Result<Value, DomainStateError> {
    let label = match agent {
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "pi" => "Pi Agent",
        _ => "Claude Code",
    };
    if crate::acp::channel::live(&target.project_id, &target.session_id, "rewind").is_none() {
        if crate::acp::channel::live(&target.project_id, &target.session_id, "state").is_some() {
            return Err(DomainStateError {
                code: "unsupportedAgent",
                message: format!("{label} was started by an older Ghostex, so it can't go back yet. Rewind works here after {label} restarts."),
            });
        }
        return Err(session_not_running(format!(
            "{label} isn't connected yet. Try again in a moment."
        )));
    }
    idle(&target.session, label)?;
    let Some(_guard) = RewindInFlightGuard::claim(&target.project_id, &target.session_id) else {
        return Err(agent_busy("A rewind is already running for this session."));
    };
    let result = match agent {
        "codex" => rewind_codex(state, &target, message_id).await?,
        "opencode" => ask_host(&target, json!({ "messageId": message_id }), "OpenCode").await?,
        "pi" => rewind_pi(state, &target, message_id).await?,
        _ => rewind_claude(state, &target, message_id).await?,
    };
    crate::session_chat_follower::request_session_chat_resnapshot(
        state,
        &target.project_id,
        &target.session_id,
    );
    log_session_chat_rewind(
        LogLevel::Info,
        "sessionChatRewound",
        json!({
            "projectId": target.project_id,
            "sessionId": target.session_id,
            "targetMessageId": message_id,
            "agent": agent,
            "native": true,
        }),
        None,
    );
    Ok(result)
}

fn idle(session: &Value, label: &str) -> Result<(), DomainStateError> {
    if crate::presentation::effective_lifecycle_state(session) != "running" {
        return Err(session_not_running(format!("{label} is not running.")));
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if crate::presentation::presentation_activity(session, &now) == "working" {
        return Err(agent_busy(format!(
            "{label} is still working. Wait for it to finish, or stop it, and then rewind."
        )));
    }
    Ok(())
}

async fn ask_host(
    target: &SessionChatSendTarget,
    params: Value,
    label: &str,
) -> Result<Value, DomainStateError> {
    crate::acp::channel::request(
        &target.project_id,
        &target.session_id,
        "rewind",
        params,
        NATIVE_REWIND_TIMEOUT,
    )
    .await
    .map_err(|error| host_error(error, label))
}

fn host_error(error: ChannelError, label: &str) -> DomainStateError {
    match error {
        ChannelError::Failed(message) => match message.as_str() {
            "ACP_AGENT_BUSY" => agent_busy(format!(
                "{label} is still working. Wait for it to finish, or stop it, and then rewind."
            )),
            "REWIND_NOT_FOUND" => message_not_found(
                "That message is not a user prompt of this conversation, so there is nothing to rewind to.",
            ),
            "REWIND_STEER" => DomainStateError {
                code: "rewindRejected",
                message: "Codex can't go back to a message sent while it was already working. Pick the message that started that reply instead. Nothing was changed.".to_string(),
            },
            detail if detail.starts_with("REWIND_UNSUPPORTED") => {
                eprintln!("[gxserver] native rewind unsupported: {detail}");
                DomainStateError {
                    code: "unsupportedAgent",
                    message: format!("This version of {label} can't go back in this chat. Update {label}, then try again."),
                }
            }
            detail => {
                eprintln!("[gxserver] native rewind failed: {detail}");
                DomainStateError {
                    code: "rewindFailed",
                    message: format!("{label} couldn't go back to that message, so nothing changed. Try again, or do it in Terminal view."),
                }
            }
        },
        ChannelError::Unsupported(_) => DomainStateError {
            code: "unsupportedAgent",
            message: format!("{label} was started by an older Ghostex, so it can't go back yet. Rewind works here after {label} restarts."),
        },
        ChannelError::Unavailable(_) => session_not_running(format!(
            "{label} isn't connected right now. Try again in a moment."
        )),
        ChannelError::NoAnswer => DomainStateError {
            code: "timeout",
            message: format!("{label} didn't confirm that it went back. Check the chat before you try again."),
        },
    }
}

/// Claude Code: record the rewind for the chat readers first (the restarted process reads the
/// same record), then have the host restart Claude at the row before the prompt.
async fn rewind_claude(
    state: &AppState,
    target: &SessionChatSendTarget,
    message_id: &str,
) -> Result<Value, DomainStateError> {
    let path = claude_transcript_path(&target.session)?;
    // No start time: a restarted Claude Code loads only what follows the latest compaction, so a
    // prompt above one is refused (`beforeCompaction`) instead of a resume point Claude can't find.
    let rewind_target = resolve_rewind_target(&path, message_id, None, None)?;
    let previous = session_chat_pending_rewind(&path);
    let record = |path: &Path| -> Result<(), DomainStateError> {
        let cutoff_offset = std::fs::metadata(path)
            .map_err(|error| {
                session_not_running(format!(
                    "The session's Claude transcript could not be read: {error}"
                ))
            })?
            .len();
        set_session_chat_pending_rewind(
            path,
            SessionChatPendingRewind {
                leaf_id: rewind_target.leaf_id.clone(),
                cutoff_offset,
                set_at_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
        Ok(())
    };
    record(&path)?;
    let answer = match ask_host(
        target,
        json!({ "leafId": rewind_target.leaf_id }),
        "Claude Code",
    )
    .await
    {
        Ok(answer) => answer,
        Err(error) => {
            match previous {
                Some(previous) => set_session_chat_pending_rewind(&path, previous),
                None => crate::session_chat_rewind_state::clear_session_chat_pending_rewind(&path),
            }
            return Err(error);
        }
    };
    let mut warning = None;
    if rewind_target.leaf_id.is_some() {
        // Anything Claude wrote while loading is not an answer to the rewind; only the next
        // prompt is.
        record(&path)?;
    } else {
        // Before the first prompt there is no row to resume at: the host started an empty
        // conversation under a new id, which the chat follows from here.
        let adopted = answer
            .get("sessionId")
            .and_then(Value::as_str)
            .zip(read_runtime_text(&target.session, "agentSessionId"))
            .map(|(fresh, previous)| {
                adopt_identity(
                    state,
                    target,
                    &previous,
                    fresh,
                    "",
                    answer.get("childPid").and_then(Value::as_i64),
                )
            })
            .transpose()?
            .unwrap_or(false);
        if !adopted {
            warning = Some(
                "Claude Code went back, but this chat couldn't catch up. Reopen the chat to see where it is now."
                    .to_string(),
            );
        }
    }
    Ok(json!({
        "ok": true,
        "warning": warning,
        "targetMessageId": rewind_target.message_id,
        "leafId": rewind_target.leaf_id,
    }))
}

/// Codex: `thread/revert` writes a replacement rollout under the same thread id; check it holds
/// exactly the prompts before the target, then move the chat onto it.
async fn rewind_codex(
    state: &AppState,
    target: &SessionChatSendTarget,
    message_id: &str,
) -> Result<Value, DomainStateError> {
    let thread_id = read_runtime_text(&target.session, "agentSessionId")
        .ok_or_else(|| message_not_found("This session has no Codex transcript yet."))?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        SessionChatTranscriptAgent::Codex,
        Some(&thread_id),
        read_runtime_text(&target.session, "agentSessionPath").as_deref(),
    )
    .ok_or_else(|| message_not_found("This session has no Codex transcript yet."))?;
    let prompts = codex::prompts(&path)?;
    let index = prompts
        .iter()
        .position(|(id, _)| id == message_id)
        .ok_or_else(|| {
            message_not_found("That message is not an active user prompt of this conversation.")
        })?;
    let kept = prompts[index + 1..].to_vec();
    let answer = ask_host(target, json!({ "messageId": message_id }), "Codex").await?;
    let new_path = answer
        .get("transcriptPath")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let holds_kept = new_path
        .as_deref()
        .is_some_and(|path| codex::prompts(path).is_ok_and(|now| now == kept));
    let adopted = match (&new_path, holds_kept) {
        (Some(new_path), true) => adopt_identity(
            state,
            target,
            &thread_id,
            answer
                .get("threadId")
                .and_then(Value::as_str)
                .unwrap_or(&thread_id),
            &new_path.to_string_lossy(),
            answer.get("childPid").and_then(Value::as_i64),
        )?,
        _ => {
            eprintln!(
                "[gxserver] native Codex rewind of {message_id}: the reverted rollout {:?} does not hold the {} prompts before it",
                new_path,
                kept.len()
            );
            false
        }
    };
    let warning = (!adopted).then(|| {
        "Codex went back, but this chat couldn't catch up. Reopen the chat to see where it is now."
            .to_string()
    });
    Ok(json!({
        "ok": true,
        "warning": warning,
        "targetMessageId": message_id,
        "leafId": null,
    }))
}

/// Pi Agent: `fork { entryId }` writes a branched session file containing entries before the
/// chosen user message and switches the live RPC host onto it; adopt that new session identity.
async fn rewind_pi(
    state: &AppState,
    target: &SessionChatSendTarget,
    message_id: &str,
) -> Result<Value, DomainStateError> {
    let previous_id = read_runtime_text(&target.session, "agentSessionId").unwrap_or_default();
    let answer = ask_host(target, json!({ "messageId": message_id }), "Pi Agent").await?;
    let next_id = answer
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let next_path = answer
        .get("transcriptPath")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let adopted = match next_id {
        Some(next_id) => adopt_identity(
            state,
            target,
            &previous_id,
            next_id,
            next_path,
            answer.get("childPid").and_then(Value::as_i64),
        )?,
        None => false,
    };
    let warning = (!adopted).then(|| {
        "Pi Agent went back, but this chat couldn't catch up. Reopen the chat to see where it is now."
            .to_string()
    });
    Ok(json!({
        "ok": true,
        "warning": warning,
        "targetMessageId": message_id,
        "leafId": null,
    }))
}

/// Move the session onto the conversation the agent now holds, through the same verified path a
/// Terminal Codex rewind uses, so the live-process scan does not put the old one back.
fn adopt_identity(
    state: &AppState,
    target: &SessionChatSendTarget,
    previous_id: &str,
    next_id: &str,
    next_path: &str,
    child_pid: Option<i64>,
) -> Result<bool, DomainStateError> {
    let db = open_gxserver_database(&state.paths)
        .map_err(|error| session_not_running(error.to_string()))?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let adopted = crate::agents::apply_verified_rewind_session_identity(
        &repository,
        &target.project_id,
        &target.session_id,
        previous_id,
        next_id,
        next_path,
        child_pid.filter(|pid| *pid > 0).unwrap_or_default(),
    )?;
    crate::zmx::invalidate_zmx_process_identity_cache();
    Ok(adopted)
}
