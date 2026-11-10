//! A chat message sent to a session whose agent is asleep or still starting.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::session_chat_options::SessionChatTerminalDetection;
use crate::session_chat_send::SessionChatSendTarget;
use crate::storage::open_gxserver_database;

/// Refusal code the composer send raises for a session that is not up yet; the HTTP handler turns
/// it into a delivery that waits for the agent instead of an error.
pub(crate) const SESSION_CHAT_SESSION_STARTING: &str = "sessionStarting";

/// The first line the restore script prints before it runs the agent's resume command
/// (`wrap_restored_terminal_resume_command` in agents/helpers.rs).
const RESTORE_BANNER: &str = "Restoring session...";

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: pressing Enter in the chat must always send, even when the session "wasn't attached, or it wasn't awake". Sends that found no zmx daemon, or found the agent still printing its restore banner, came back as "input box is not accepting input" or "could not be cleared". Now the send wakes a sleeping session itself, and a session that is still starting takes the message as a startup send: the chat shows it at once and the queue types it the moment the input box appears, as a new chat already does (2026-09-09 decision).
/// WHY: only a daemon that `zmx` reports missing is woken. A screen counts as starting when it shows the restore banner or nothing at all with no input box (`screen_shows_agent_starting`); since 2026-10-11 any other missing input box is held too (`holds_until_agent_takes_input`), because the queue types only into an input box it has seen, so a dialog cannot eat the message.
pub(crate) async fn starting_session_refusal(
    state: &AppState,
    target: &SessionChatSendTarget,
    detection: &SessionChatTerminalDetection,
) -> Option<DomainStateError> {
    // No screen could be read: find out whether the daemon is gone before waking anything. A held
    // message too old to wake for is marked Not sent first, so this wake never types it.
    let missing = !detection.captured
        && provider_missing(state, &target.project_id, &target.session_id).await;
    if missing {
        retire_stale_startup_sends(state, &target.project_id, &target.session_id);
    }
    // A message already held for this session goes first; typing the next one now would put it
    // ahead of the one the user sent earlier. A session that is not up (an earlier wake that did
    // not stick) is woken again by this send.
    if startup_send_pending(state, target) {
        if missing {
            wake_session_holding_startup_send(state, &target.project_id, &target.session_id, None);
        }
        return Some(starting(
            "An earlier message is waiting for the agent to start.",
        ));
    }
    if detection.captured {
        let booting = detection.composer.is_not_ready()
            && !detection.composer.should_dismiss()
            && detection.notice.is_none()
            && screen_shows_agent_starting(&detection.composer.screen_tail);
        if !booting {
            return None;
        }
        crate::session_chat_send_diagnostics::record_send_recovery(
            state,
            "sessionChatSendHeldForStartingAgent",
            &target.project_id,
            &target.session_id,
            detection.composer.reason.as_deref().unwrap_or_default(),
            &detection.composer.screen_tail,
        );
        return Some(starting("The agent is still starting."));
    }
    if !missing {
        return None;
    }
    // CDXC:SessionChat 2026-11-10 WHY: waking a closed session resumes its agent, which on a slow
    // computer outlasted the 15 seconds `ghostex agents send` waits for this answer: the send printed
    // no status, the coordinator sent again, and the message was held twice (GitHub #531). The send
    // waits for the wake up to `SEND_WAKE_WAIT`; a wake still going then finishes on its own while
    // the message is held for the starting agent (a failure is retried by the queue scheduler).
    let (wake_state, project_id, session_id) = (
        state.clone(),
        target.project_id.clone(),
        target.session_id.clone(),
    );
    let mut wake = tokio::spawn(async move {
        let outcome = wake_session(&wake_state, &project_id, &session_id).await;
        record_wake(
            &wake_state,
            &project_id,
            &session_id,
            &outcome,
            "The session's zmx daemon was not running when the message was sent.",
        );
        outcome
    });
    match tokio::time::timeout(SEND_WAKE_WAIT, &mut wake).await {
        Ok(Ok(outcome)) => Some(wake_refusal(outcome)),
        Ok(Err(_)) | Err(_) => Some(starting(
            "The session is waking up; Ghostex types the message once its agent is ready.",
        )),
    }
}

/// How long a send waits for the session it wakes before holding the message.
const SEND_WAKE_WAIT: std::time::Duration = std::time::Duration::from_secs(8);

/// What a send that woke the session answers. A wake that failed for a reason time can fix holds
/// the message like a starting agent: the queue scheduler wakes the session again until it is up
/// (`wake_session_holding_startup_send`).
fn wake_refusal(outcome: WakeOutcome) -> DomainStateError {
    match outcome {
        WakeOutcome::Blocked(reason) => DomainStateError {
            code: "dependencyUnavailable",
            message: format!(
                "The session is asleep and cannot be woken ({reason}), so nothing was sent."
            ),
        },
        WakeOutcome::Woke => starting("The session was asleep and is starting now."),
        WakeOutcome::AlreadyWaking | WakeOutcome::Failed(_) => {
            starting("The session is asleep; Ghostex keeps waking it until it starts.")
        }
    }
}

/// CDXC:SessionChat 2026-10-29 DECISION:
/// User: "a chat session's terminal was sleeping, and i answered inline html fully then sent to agent. but nothing happened. it stayed sleeping and my answer wasn't sent (until i woke that terminal up manually by switching to terminal and clicking resume). pls fix this and other similar cases". Every way of sending to a sleeping session wakes it and delivers, not only Enter in the chat (2026-09-25 decision above): answers from a page or a ```questions form, a queued message, `ghostex agents send --queue`, Send now and Retry on a row. Those go into the chat queue as ordinary rows, and the queue scheduler left an ordinary row in a sleeping session waiting for a stop that never came; only a startup send woke it. Now the scheduler turns the head row of a sleeping session into a startup send (drawn in the transcript as on its way) and wakes the session; the row goes out once the woken agent's input box is up, and a wake that cannot succeed marks it Not sent with the reason, keeping its text for Retry. A queue delivery that finds the daemon gone wakes it the same way (`queued_delivery_wake_refusal`), and Retry wakes at once (`wake_for_retried_message`). Rows older than `STARTUP_SEND_WAKE_MAX_AGE_HOURS` (2026-10-12 WHY below) and the rows of a closed session do not wake it on their own; Retry still does.
/// SEE-ALSO: session_chat_queue_runtime/scheduler.rs (the sleeping-session branch), session_chat_queue_runtime/send.rs (queue deliveries), session_chat_queue_runtime/endpoints.rs (Retry).
pub(crate) fn wakes_sleeping_session_for(created_at: &str) -> bool {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(STARTUP_SEND_WAKE_MAX_AGE_HOURS);
    chrono::DateTime::parse_from_rfc3339(created_at).is_ok_and(|created| created >= cutoff)
}

/// A queued row's delivery (the scheduler, Send now, a Slack request) that finds the session's zmx
/// daemon gone wakes it; `None` when the daemon is up or its screen was read.
pub(crate) async fn queued_delivery_wake_refusal(
    state: &AppState,
    target: &SessionChatSendTarget,
    detection: &SessionChatTerminalDetection,
) -> Option<DomainStateError> {
    if detection.captured || !provider_missing(state, &target.project_id, &target.session_id).await
    {
        return None;
    }
    let outcome = wake_session(state, &target.project_id, &target.session_id).await;
    record_wake(
        state,
        &target.project_id,
        &target.session_id,
        &outcome,
        "The session's zmx daemon was not running when a queued message was delivered.",
    );
    Some(wake_refusal(outcome))
}

/// Retry on a Not sent row of a session whose daemon is gone: the row waits as a startup send and
/// the session is woken now, whatever the row's age, because the user just asked again.
pub(crate) fn wake_for_retried_message(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) {
    let state = state.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    let prompt_id = prompt_id.to_string();
    crate::session_chat_queue_runtime::forget_repeated_restarts(&project_id, &session_id);
    tokio::spawn(async move {
        if !provider_missing(&state, &project_id, &session_id).await {
            return;
        }
        if crate::session_chat_queue::hold_queued_prompt_for_wake(
            &state.paths,
            &project_id,
            &session_id,
            &prompt_id,
        ) {
            crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                &state,
                &project_id,
                &session_id,
            );
        }
        let outcome = wake_session(&state, &project_id, &session_id).await;
        record_wake(
            &state,
            &project_id,
            &session_id,
            &outcome,
            "A message was retried while the session's zmx daemon was not running.",
        );
        if let WakeOutcome::Blocked(reason) = outcome {
            fail_held_message(
                &state,
                &project_id,
                &session_id,
                &prompt_id,
                &format!("The session cannot be woken: {reason}."),
            );
        }
    });
}

/// What one `/api/wakeSession` from this module came to.
pub(crate) enum WakeOutcome {
    Woke,
    /// Another wake of the same session from this module is still running.
    AlreadyWaking,
    /// The session cannot be restored until something changes (its folder is gone).
    Blocked(String),
    /// Anything else; worth trying again later.
    Failed(String),
}

/// Sessions this module is waking right now, so the send, the queue scheduler's retry and a
/// queued command never start the same session's daemon twice at once.
static WAKING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

pub(crate) async fn wake_session(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> WakeOutcome {
    let key = format!("{project_id}\u{1f}{session_id}");
    if !WAKING
        .lock()
        .map(|mut waking| waking.insert(key.clone()))
        .unwrap_or(false)
    {
        return WakeOutcome::AlreadyWaking;
    }
    let body = json!({ "params": { "projectId": project_id, "sessionId": session_id } });
    let routed = crate::server::handle_zmx_lifecycle_http(
        state,
        "/api/wakeSession".to_string(),
        uuid::Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    let success = routed.response.status().is_success();
    let bytes = axum::body::to_bytes(routed.response.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    if let Ok(mut waking) = WAKING.lock() {
        waking.remove(&key);
    }
    let reply: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !success {
        let message = reply
            .pointer("/error/message")
            .or_else(|| reply.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| String::from_utf8_lossy(&bytes).chars().take(300).collect());
        return WakeOutcome::Failed(message);
    }
    match reply.pointer("/result/attach/restoreBlocked") {
        Some(blocked) => WakeOutcome::Blocked(match blocked.get("cwd").and_then(Value::as_str) {
            Some(cwd) => format!("its folder {cwd} no longer exists"),
            None => "its folder no longer exists".to_string(),
        }),
        None => WakeOutcome::Woke,
    }
}

fn record_wake(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    outcome: &WakeOutcome,
    reason: &str,
) {
    let (event, detail) = match outcome {
        WakeOutcome::Woke => ("sessionChatSendWokeSession", None),
        WakeOutcome::AlreadyWaking => return,
        WakeOutcome::Blocked(why) | WakeOutcome::Failed(why) => {
            ("sessionChatSendWakeFailed", Some(why.as_str()))
        }
    };
    let reason = match detail {
        Some(detail) => format!("{reason} Wake failed: {detail}"),
        None => reason.to_string(),
    };
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        event,
        project_id,
        session_id,
        &reason,
        &[],
    );
}

/// CDXC:SessionChat 2026-10-12 WHY:
/// A message sent to a sleeping session is held as a startup send and the send wakes the session once (2026-09-25 decision above), but nothing checked afterwards that it stayed up: on 2026-10-12 an orchestrator's message to a sleeping Windows thread was held, that one wake did not leave a running agent behind, the queue scheduler skipped the row for as long as the session was not running, and the next send only queued behind it, so the message waited over an hour until someone ran `ghostex wake` by hand. Now, while a session holds a startup send and its daemon is gone, the queue scheduler calls this on a backoff (every 10 s, settling at every 60 s, for as long as the row waits) and it wakes the session the way `ghostex wake` does; a session whose daemon is up is never woken. A wake that cannot succeed until something changes (the session's folder is gone) marks the row Not sent with that reason instead, which tells an agent sender through the queue's undelivered report.
/// SEE-ALSO: session_chat_queue_runtime/scheduler.rs (when it is called); server/src/ghostex_cli/coordinator/threads.rs (`orchestrator reopen` wakes a sleeping thread too).
pub(crate) fn wake_session_holding_startup_send(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt_id: Option<String>,
) {
    let state = state.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        if !provider_missing(&state, &project_id, &session_id).await {
            // The daemon is up but the agent never took input: a start that failed is healed or
            // shown (CDXC:SessionChat 2026-10-14 in session_chat_agent_restart.rs), for a held
            // message young enough to wake for.
            if prompt_id.as_deref().is_some_and(|prompt_id| {
                held_message_young_enough(&state, &project_id, &session_id, prompt_id)
            }) {
                if let Some(reason) = crate::session_chat_agent_restart::heal_failed_start(
                    &state,
                    &project_id,
                    &session_id,
                )
                .await
                {
                    // GitHub #456: no later check could start it, so the message stops waiting.
                    fail_held_message(
                        &state,
                        &project_id,
                        &session_id,
                        prompt_id.as_deref().unwrap_or_default(),
                        &crate::session_chat_send_failure::send_failure_reason(
                            "The agent in this session could not start again, so the message was not sent.",
                            Some(reason),
                        ),
                    );
                }
            }
            return;
        }
        // The scheduler's call is for a held message: once the stale ones are retired, wake only
        // if one is left. The send's own call (no prompt id) is for the message it is sending.
        retire_stale_startup_sends(&state, &project_id, &session_id);
        if prompt_id.is_some() && !startup_send_held(&state, &project_id, &session_id) {
            return;
        }
        let outcome = wake_session(&state, &project_id, &session_id).await;
        record_wake(
            &state,
            &project_id,
            &session_id,
            &outcome,
            "A message was held for the session while its zmx daemon was not running.",
        );
        let (WakeOutcome::Blocked(reason), Some(prompt_id)) = (outcome, prompt_id) else {
            return;
        };
        fail_held_message(
            &state,
            &project_id,
            &session_id,
            &prompt_id,
            &format!("The session cannot be woken: {reason}."),
        );
    });
}

/// CDXC:SessionChat 2026-10-12 WHY:
/// A held message wakes its session only while it is younger than this. On the day the wake retry above was added, the user's machine held 6 messages on closed sessions, 58 to 666 hours old (test pings, briefs to threads long since finished), and the first gxserver start would have woken all six agents and typed each stale message into them. A day still covers a machine that was off overnight; an older message is marked Not sent with the reason below, which tells an agent sender through the queue's undelivered report, and the user can still Retry it from the chat. This only gates waking: a held message in a session that is already up keeps waiting for its input box (2026-10-11 decision above).
const STARTUP_SEND_WAKE_MAX_AGE_HOURS: i64 = 24;

/// Whether the held message `prompt_id` is younger than `STARTUP_SEND_WAKE_MAX_AGE_HOURS`.
fn held_message_young_enough(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(STARTUP_SEND_WAKE_MAX_AGE_HOURS);
    crate::session_chat_queue::read_session_chat_queue_snapshot_with(&db, project_id, session_id)
        .queue
        .iter()
        .any(|prompt| {
            prompt.id == prompt_id
                && chrono::DateTime::parse_from_rfc3339(&prompt.created_at)
                    .is_ok_and(|created| created >= cutoff)
        })
}

/// Marks Not sent every held startup send of this session too old to wake for.
fn retire_stale_startup_sends(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(STARTUP_SEND_WAKE_MAX_AGE_HOURS);
    let stale: Vec<String> = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        &db, project_id, session_id,
    )
    .queue
    .into_iter()
    .filter(|prompt| {
        prompt.startup_send
            && prompt.state == crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_QUEUED
            && chrono::DateTime::parse_from_rfc3339(&prompt.created_at)
                .is_ok_and(|created| created < cutoff)
    })
    .map(|prompt| prompt.id)
    .collect();
    drop(db);
    for prompt_id in stale {
        fail_held_message(
            state,
            project_id,
            session_id,
            &prompt_id,
            &format!(
                "It waited more than {STARTUP_SEND_WAKE_MAX_AGE_HOURS} hours for the session to wake, so Ghostex stopped waking the session for it. Send it again if it is still needed."
            ),
        );
    }
}

/// CDXC:SessionChat 2026-10-14 WHY:
/// Closing a session ends the messages it still holds for its agent. A held message wakes its session on the queue scheduler's backoff (2026-10-12 above), and nothing told it the session had been closed meanwhile: on 2026-10-14 a test agent created a throwaway Claude session whose agent could not start, sent it a message (held), then closed it with `ghostex kill`; 50 seconds later the wake retry brought it back as a running, untitled "Claude Session", and every time the user closed it again it came back. Closing (`/api/killSession`, the Close transition, from any client, the CLI or a cleanup sweep) now marks those messages Not sent, which also tells an agent sender through the queue's undelivered report. A message sent to a session after it was closed is a new ask and still wakes it.
/// SEE-ALSO: server/src/server/zmx_http.rs (`dispatch_zmx_lifecycle_http_blocking` calls this after a close).
pub(crate) fn retire_held_messages_of_closed_session(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let held: Vec<String> = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        &db, project_id, session_id,
    )
    .queue
    .into_iter()
    .filter(|prompt| {
        prompt.startup_send
            && prompt.state == crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_QUEUED
    })
    .map(|prompt| prompt.id)
    .collect();
    drop(db);
    for prompt_id in held {
        fail_held_message(
            state,
            project_id,
            session_id,
            &prompt_id,
            "The session was closed before its agent took this message, so Ghostex stopped waking it. Send it again if it is still needed.",
        );
    }
}

/// Marks a held message Not sent with `reason` and tells its sender when another agent sent it.
fn fail_held_message(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
    reason: &str,
) {
    let Ok(snapshot) = crate::session_chat_queue::fail_session_chat_queued_prompt(
        &state.paths,
        project_id,
        session_id,
        prompt_id,
        reason,
    ) else {
        return;
    };
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );
    let Some(prompt) = snapshot.queue.iter().find(|prompt| prompt.id == prompt_id) else {
        return;
    };
    if let Some((sender_project, sender_session)) =
        crate::session_chat_queue_undelivered::notify_sender_of_undelivered_agent_message(
            &state.paths,
            state.metadata.server_id.as_str(),
            project_id,
            session_id,
            &prompt.text,
            reason,
        )
    {
        crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
            state,
            &sender_project,
            &sender_session,
        );
    }
}

/// CDXC:SessionChat 2026-10-11 WHY:
/// On a Windows machine whose every new PowerShell hung for minutes, a new session's screen stayed blank while the shell loaded, and the send read that as "the agent is gone": it refused the message and tried to restart the session, which would have killed the shell that was still starting. Nothing on screen yet is what a starting agent looks like (an exited agent leaves its shell prompt or its exit text), so a blank screen counts as starting, like the restore banner. `screen_tail` holds only non-blank lines.
/// CDXC:SessionChat 2026-10-14 WHY: the restore banner stays on screen when the resume fails and the script drops to the shell (cswap's "Account-5 has no stored credentials" left "Restoring session..." above the prompt of session G7v5a), so the banner counts as starting only while no shell prompt has come back after it; otherwise a send to a dead start was held as "still starting" for good.
pub(crate) fn screen_shows_agent_starting(screen_tail: &[String]) -> bool {
    screen_tail.is_empty()
        || (screen_tail.iter().any(|line| line.trim() == RESTORE_BANNER)
            && !crate::session_chat_notice::session_chat_screen_ends_with_shell_prompt(
                &screen_tail.join("\n"),
            ))
}

/// CDXC:SessionChat 2026-10-11 DECISION:
/// User: "due to the lag due to M drive when i just started a session and tried to send chat message there it goes in chat transcript then back to composer after few seconds. this shouldn't actually happen we should just keep it in chat and keep trying every X seconds until it does get posted. imagine the user sent it then went away. we need to self heal as i said many times before!! … doesn't matter the reason we need to have best self heal for all situations". A chat message refused because the agent's input box is not there yet (still starting, a shell still loading, a resume, a repaint, a panel) is never handed back to the composer: gxserver keeps it as a startup send in the session's queue, the chat draws it in the transcript as waiting, and the queue scheduler types it the moment the input box appears, retrying every 2 s and backing off to every 15 s for as long as the session lives (`session_chat_queue_undelivered.rs`). It survives closed clients and gxserver restarts because the queue is in SQLite. It ends up Not sent (with its reason, Retry and Remove) only when retrying cannot help: a question or dialog only the user can answer, or a refusal that is not about the input box. Pictures wait with the text (session_chat_queue_attachments.rs).
/// SEE-ALSO: packages/gx-chat-core/src/session/startup_sends.rs draws the held row in the transcript; apps/desktop/src/app/native_chat/startup_delivery.rs and message_actions.rs, and apps/mobile/app/src/chat/native/transcript/MessageActions.tsx, show its waiting state with Edit and Delete.
pub(crate) fn holds_until_agent_takes_input(error: &DomainStateError) -> bool {
    error.code == "composerNotReady"
        && !error.message.ends_with("Answer it in chat before sending.")
}

fn startup_send_pending(state: &AppState, target: &SessionChatSendTarget) -> bool {
    startup_send_held(state, &target.project_id, &target.session_id)
}

fn startup_send_held(state: &AppState, project_id: &str, session_id: &str) -> bool {
    open_gxserver_database(&state.paths).is_ok_and(|db| {
        crate::session_chat_queue::read_session_chat_queue_snapshot_with(
            &db, project_id, session_id,
        )
        .queue
        .iter()
        .any(|prompt| prompt.startup_send && prompt.state != "failed")
    })
}

fn starting(message: &str) -> DomainStateError {
    DomainStateError {
        code: SESSION_CHAT_SESSION_STARTING,
        message: message.to_string(),
    }
}

async fn provider_missing(state: &AppState, project_id: &str, session_id: &str) -> bool {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let lifecycle = crate::zmx::LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).ok()?;
        let repository = DomainRepository::new(&db, server_id.as_str());
        crate::zmx::probe_and_cache_session_provider(&repository, &lifecycle)
            .ok()
            .map(|(probe, ..)| probe.lifecycle_state == "missing")
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}

/// CDXC:SessionTitles 2026-09-27 WHY:
/// An agent rename is typed into the agent's own input box, and a sleeping session has none: the direct write to its missing zmx daemon was lost after the new title was already saved, so the agent's older title replaced it on the next wake. A command for a session whose daemon is gone waits in the queue as a startup send and wakes the session, the same way a chat message sent to a sleeping session does (2026-09-25 decision above).
pub(crate) fn queue_startup_command_and_wake(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    command: &str,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("text".to_string(), json!(command));
    params.insert("startupSend".to_string(), json!(true));
    crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        "/api/queueSessionChatPrompt",
        &params,
    )?;
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );
    let state = state.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let outcome = wake_session(&state, &project_id, &session_id).await;
        record_wake(
            &state,
            &project_id,
            &session_id,
            &outcome,
            "The session's zmx daemon was not running when a command was queued for it.",
        );
    });
    Ok(())
}

/// CDXC:Drafts 2026-10-06 WHY:
/// A draft's agent is started by the client that opens it (the desktop's attach, the phone's background attach), so a draft created with `ghostex create-agent --defer-start` that no client opened had no zmx daemon, and a chat message sent to it waited in the queue forever. The first message to such a draft wakes it, as a send to a sleeping session does (2026-09-25 decision); a draft whose daemon is already up is left alone.
pub(crate) fn start_draft_agent_if_missing(state: &AppState, target: &SessionChatSendTarget) {
    let state = state.clone();
    let target = SessionChatSendTarget {
        project_id: target.project_id.clone(),
        session_id: target.session_id.clone(),
        zmx_name: target.zmx_name.clone(),
        session: Value::Null,
    };
    tokio::spawn(async move {
        if !provider_missing(&state, &target.project_id, &target.session_id).await {
            return;
        }
        let outcome = wake_session(&state, &target.project_id, &target.session_id).await;
        record_wake(
            &state,
            &target.project_id,
            &target.session_id,
            &outcome,
            "The draft's agent was not running when its first message was queued.",
        );
    });
}

/// A send accepted now and typed by the queue once the agent's input box appears; the chat draws
/// it in the transcript, not in the queue strip. The row keeps the send's `imagePaths` too.
/// A chat box send that failed for good stays in the thread as a failed row, in the place a held
/// message would be, with the reason on it. The draft goes with it, as for any queued message, so
/// Retry can never send it twice.
pub(crate) fn queue_failed_composer_send(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
    target: &SessionChatSendTarget,
    text: &str,
    error: DomainStateError,
) -> RoutedResponse {
    let mut startup_params = params.clone();
    startup_params.insert("startupSend".to_string(), json!(true));
    let queued = crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        "/api/queueSessionChatPrompt",
        &startup_params,
    );
    let prompt_id = queued.ok().and_then(|result| {
        result
            .value
            .pointer("/prompt/id")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let Some(prompt_id) = prompt_id.filter(|prompt_id| {
        crate::session_chat_queue::fail_session_chat_queued_prompt(
            &state.paths,
            &target.project_id,
            &target.session_id,
            prompt_id,
            &error.message,
        )
        .is_ok()
    }) else {
        return domain_error_response(endpoint_path, request_id, error);
    };
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state,
        &target.project_id,
        &target.session_id,
    );
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({
                "queued": true,
                "textBytes": text.len(),
                "queuedPromptId": prompt_id,
                "notSent": true,
            }),
        ),
    )
}

/// The same failed row for a message that has no send call left to answer: it was typed into an
/// agent that then died and could not be restarted.
pub(crate) fn queue_failed_send(
    state: &AppState,
    target: &SessionChatSendTarget,
    text: &str,
    reason: &str,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(target.project_id));
    params.insert("sessionId".to_string(), json!(target.session_id));
    params.insert("text".to_string(), json!(text));
    params.insert("startupSend".to_string(), json!(true));
    let queued = crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        "/api/queueSessionChatPrompt",
        &params,
    )?;
    let prompt_id = queued
        .value
        .pointer("/prompt/id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    crate::session_chat_queue::fail_session_chat_queued_prompt(
        &state.paths,
        &target.project_id,
        &target.session_id,
        prompt_id,
        reason,
    )?;
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state,
        &target.project_id,
        &target.session_id,
    );
    Ok(())
}

pub(crate) fn queue_startup_send(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
    target: &SessionChatSendTarget,
    text: &str,
) -> RoutedResponse {
    // CDXC:SessionChat 2026-11-10 WHY: an agent that resends a message while the first copy still
    // waits for the starting agent (GitHub #531: a coordinator whose send printed no status) gets the
    // held copy back instead of a second one. The chat box (a `draftVersion`) is left alone: a
    // person may mean to send the same words twice.
    if params.get("draftVersion").is_none_or(Value::is_null) {
        if let Some(held) = held_startup_send_with_text(state, target, text) {
            return routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({
                        "queued": true,
                        "textBytes": text.len(),
                        "queuedPromptId": held,
                        "alreadyHeld": true,
                    }),
                ),
            );
        }
    }
    let mut startup_params = params.clone();
    startup_params.insert("startupSend".to_string(), json!(true));
    match crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        "/api/queueSessionChatPrompt",
        &startup_params,
    ) {
        Ok(result) => {
            crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                state,
                &target.project_id,
                &target.session_id,
            );
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({
                        "queued": true,
                        "textBytes": text.len(),
                        "queuedPromptId": result.value.pointer("/prompt/id"),
                    }),
                ),
            )
        }
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/// The id of a message with exactly this text already held for the session's starting agent.
fn held_startup_send_with_text(
    state: &AppState,
    target: &SessionChatSendTarget,
    text: &str,
) -> Option<String> {
    let db = open_gxserver_database(&state.paths).ok()?;
    crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        &db,
        &target.project_id,
        &target.session_id,
    )
    .queue
    .into_iter()
    .find(|prompt| {
        prompt.startup_send
            && prompt.state == crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_QUEUED
            && prompt.text == text
    })
    .map(|prompt| prompt.id)
}
