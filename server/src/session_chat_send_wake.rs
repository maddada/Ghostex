//! A chat message sent to a session whose agent is asleep or still starting.

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

/// How long a send carrying images waits for a starting agent. The queue cannot hold images, and
/// the chat client gives the whole request 60 seconds.
const IMAGE_SEND_START_WAIT_MS: u64 = 40_000;

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: pressing Enter in the chat must always send, even when the session "wasn't attached, or it wasn't awake". Sends that found no zmx daemon, or found the agent still printing its restore banner, came back as "input box is not accepting input" or "could not be cleared". Now the send wakes a sleeping session itself, and a session that is still starting takes the message as a startup send: the chat shows it at once and the queue types it the moment the input box appears, as a new chat already does (2026-09-09 decision).
/// WHY: only a daemon that `zmx` reports missing is woken, and only the restore banner with no input box under it counts as starting; any other missing input box is still refused, because it can be a dialog that would eat the message.
pub(crate) async fn starting_session_refusal(
    state: &AppState,
    target: &SessionChatSendTarget,
    detection: &SessionChatTerminalDetection,
) -> Option<DomainStateError> {
    // A message already held for this session goes first; typing the next one now would put it
    // ahead of the one the user sent earlier.
    if startup_send_pending(state, target) {
        return Some(starting(
            "An earlier message is waiting for the agent to start.",
        ));
    }
    if detection.captured {
        let booting = detection.composer.is_not_ready()
            && !detection.composer.should_dismiss_with_escape()
            && detection.notice.is_none()
            && detection
                .composer
                .screen_tail
                .iter()
                .any(|line| line.trim() == RESTORE_BANNER);
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
    // No screen could be read: find out whether the daemon is gone before waking anything.
    if !provider_missing(state, target).await {
        return None;
    }
    let body = json!({
        "params": { "projectId": target.project_id, "sessionId": target.session_id }
    });
    let woke = crate::server::handle_zmx_lifecycle_http(
        state,
        "/api/wakeSession".to_string(),
        uuid::Uuid::new_v4().to_string(),
        &body,
    )
    .await
    .response
    .status()
    .is_success();
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        if woke {
            "sessionChatSendWokeSession"
        } else {
            "sessionChatSendWakeFailed"
        },
        &target.project_id,
        &target.session_id,
        "The session's zmx daemon was not running when the message was sent.",
        &[],
    );
    Some(if woke {
        starting("The session was asleep and is starting now.")
    } else {
        DomainStateError {
            code: "dependencyUnavailable",
            message: "The session is asleep and could not be woken, so nothing was sent."
                .to_string(),
        }
    })
}

fn startup_send_pending(state: &AppState, target: &SessionChatSendTarget) -> bool {
    open_gxserver_database(&state.paths).is_ok_and(|db| {
        crate::session_chat_queue::read_session_chat_queue_snapshot_with(
            &db,
            &target.project_id,
            &target.session_id,
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

async fn provider_missing(state: &AppState, target: &SessionChatSendTarget) -> bool {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let lifecycle = crate::zmx::LifecycleParams {
        project_id: target.project_id.clone(),
        session_id: target.session_id.clone(),
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

/// Delivers a composer send that met a starting session: text goes into the queue as a startup
/// send; images, which the queue cannot hold, wait here for the input box and then send.
pub(crate) async fn deliver_when_started(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
    target: &SessionChatSendTarget,
    text: &str,
    image_paths: &[String],
    draft_version: Option<&crate::session_chat_draft_versions::DraftVersion>,
) -> RoutedResponse {
    if image_paths.is_empty() {
        return queue_startup_send(state, endpoint_path, request_id, params, target, text);
    }
    let wait = crate::session_chat_composer::wait_for_session_chat_composer_by_ids(
        &state.paths,
        state.metadata.server_id.as_str(),
        &target.project_id,
        &target.session_id,
        crate::session_chat_composer::SessionChatComposerWaitPolicy {
            settle_ms: 0,
            timeout_ms: IMAGE_SEND_START_WAIT_MS,
            unknown_hold_ms: IMAGE_SEND_START_WAIT_MS,
        },
    )
    .await;
    if wait != crate::session_chat_composer::SessionChatComposerWait::Ready {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "composerNotReady",
                message: "The agent is still starting, so the message with images was not sent. Send it again once its input box appears.".to_string(),
            },
        );
    }
    match crate::session_chat_queue_runtime::send_session_chat_message_with_draft(
        state,
        &target.project_id,
        &target.session_id,
        text,
        image_paths,
        crate::session_chat_queue_runtime::SessionChatMessageSource::Composer,
        draft_version,
    )
    .await
    {
        Ok(text_bytes) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(
                request_id,
                json!({ "queued": true, "textBytes": text_bytes }),
            ),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/// A send accepted now and typed by the queue once the agent's input box appears; the chat draws
/// it in the transcript, not in the queue strip.
pub(crate) fn queue_startup_send(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
    target: &SessionChatSendTarget,
    text: &str,
) -> RoutedResponse {
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
