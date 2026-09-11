use std::time::{Duration, Instant};

use super::{
    build_agent_tui_clear_input, capture_session_terminal_text_vt, write_session_chat_payload,
    SessionChatSendError, SessionChatSendFailure, AGENT_TUI_CLEAR_LINE_SLACK,
    SESSION_CHAT_CLEAR_INPUT_SETTLE_MS, SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
    SESSION_CHAT_SEND_CANCELLED,
};
use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, session_chat_composer_input, SessionChatComposerState,
};

#[derive(Clone, Copy)]
enum ComposerClearMethod {
    InterruptOnce,
    KillLines,
}

fn composer_clear_method(agent: &str) -> Option<ComposerClearMethod> {
    match agent {
        "claude" | "codex" | "cursor" | "grok" | "hermes-agent" | "pi" | "omp" => {
            Some(ComposerClearMethod::InterruptOnce)
        }
        "antigravity" | "openclaude" => Some(ComposerClearMethod::KillLines),
        _ => None,
    }
}

pub(super) fn supports_verified_composer_clear(agent: &str) -> bool {
    composer_clear_method(agent).is_some()
}

/// CDXC:SessionChat 2026-09-08 DECISION:
/// User: sending from Chat must clear existing Claude or Codex terminal text and send the chat input. Rewind's restored prompt belongs in Chat, and switching to Terminal must not append another copy.
/// The clear is checked against the live draft, not sized solely from the replacement text; a longer old draft can require several separately delivered bursts.
/// CDXC:SessionChat 2026-09-11 DECISION:
/// User approved agent-specific clearing followed by verification before sending. Spaces and newlines count as empty.
/// CDXC:SessionChat 2026-09-11 WHY:
/// The ghostex-web audit cleared populated drafts with one Ctrl+C in seven agents, but Codex and Hermes exited on empty input and Antigravity retained its draft. OpenClaude was unavailable.
/// Send Ctrl+C only once after positive draft evidence; Antigravity and OpenClaude retain line deletion. See docs/2026-09-09/ctrl-c-agent-tests/RESULTS.md.
/// Grok must not receive Ctrl+U because it quits to install a pending update.
pub async fn clear_session_chat_composer(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    agent: &str,
    cancelled: &(impl Fn() -> bool + Sync + ?Sized),
) -> Result<(), SessionChatSendError> {
    let deadline = Instant::now() + Duration::from_millis(SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS);
    let agent = crate::agents::identity::normalize_agent_id(Some(agent))
        .unwrap_or_else(|| agent.to_string());
    let method = composer_clear_method(&agent);
    let mut interrupt_sent = false;
    loop {
        if cancelled() {
            return Err(SessionChatSendError::not_attempted(
                SESSION_CHAT_SEND_CANCELLED.to_string(),
            ));
        }
        if let Some(screen) = capture_session_terminal_text_vt(zmx_name).await {
            let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(
                Some(&agent),
                &screen,
            );
            let ready =
                detect_session_chat_composer_readiness(Some(&agent), &screen, notice.as_ref());
            if ready.state == SessionChatComposerState::Ready {
                if let Some(input) = session_chat_composer_input(&agent, &screen) {
                    if input.is_empty() {
                        return Ok(());
                    }
                    if cancelled() {
                        return Err(SessionChatSendError::not_attempted(
                            SESSION_CHAT_SEND_CANCELLED.to_string(),
                        ));
                    }
                    let clear = match method {
                        Some(ComposerClearMethod::InterruptOnce) if !interrupt_sent => {
                            interrupt_sent = true;
                            Some("\u{3}".to_string())
                        }
                        Some(ComposerClearMethod::KillLines) => Some(build_agent_tui_clear_input(
                            input.rows + AGENT_TUI_CLEAR_LINE_SLACK,
                        )),
                        _ => None,
                    };
                    if let Some(clear) = clear {
                        write_session_chat_payload(
                            project_id, session_id, zmx_name, source, &clear,
                        )
                        .await
                        .map_err(|message| {
                            SessionChatSendError::new(SessionChatSendFailure::Write, message)
                        })?;
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(SESSION_CHAT_CLEAR_INPUT_SETTLE_MS)).await;
    }
    Err(SessionChatSendError::new(
        SessionChatSendFailure::ComposerNotCleared,
        "The terminal draft could not be cleared and verified. Your chat draft has been kept."
            .to_string(),
    ))
}

async fn place_session_chat_draft(
    target: &super::SessionChatSendTarget,
    content: &str,
    state_dir: &std::path::Path,
) -> Result<serde_json::Value, crate::domain::DomainStateError> {
    if content.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(crate::domain::DomainStateError {
            code: "invalidParams",
            message: "The draft exceeds the terminal input size limit.".to_string(),
        });
    }
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&target.session)
        .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(&target.session));
    // Capture already saves and clears the old input through the agent's editor.
    // A second clear burst here would erase keystrokes typed after that handshake.
    let mut steps = vec![
        super::SessionChatSendStep::WaitForComposer {
            agent: agent.clone(),
            settle_ms: super::SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS,
            timeout_ms: super::SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
        },
        super::SessionChatSendStep::PreserveTerminalDraft {
            replacement: Some(content.to_string()),
            state_dir: state_dir.to_path_buf(),
            prompt_editor_input: if agent.as_deref() == Some("grok") {
                super::SESSION_CHAT_GROK_PROMPT_EDITOR_INPUT
            } else {
                super::SESSION_CHAT_PROMPT_EDITOR_INPUT
            }
            .to_string(),
        },
        super::SessionChatSendStep::WaitForComposer {
            agent: agent.clone(),
            settle_ms: super::SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS,
            timeout_ms: super::SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
        },
        super::SessionChatSendStep::Write(super::build_session_chat_paste_bytes(content)),
    ];
    if let Some(verify) = super::session_chat_verify_step(content) {
        steps.push(verify);
    }
    super::execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-draft-to-terminal",
        steps,
    )
    .await
    .map_err(|error| crate::domain::DomainStateError {
        code: "sessionInputFailed",
        message: error.message,
    })?;
    Ok(serde_json::json!({ "replaced": true }))
}

pub(crate) async fn handle_replace_session_chat_draft_http(
    state: &crate::server::AppState,
    endpoint_path: String,
    request_id: String,
    body: &serde_json::Value,
) -> crate::server::RoutedResponse {
    use crate::{
        domain::{read_domain_rpc_params, DomainStateError},
        server::{domain_error_response, routed_json},
    };
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let target =
        match super::resolve_session_chat_send_target(state, &params, "replaceSessionChatDraft") {
            Ok(target) => target,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        };
    let Some(content) = params
        .get("content")
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.trim().is_empty())
    else {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "A terminal draft replacement requires content.".to_string(),
            },
        );
    };
    let db = match crate::storage::open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: error.to_string(),
                },
            )
        }
    };
    let id = params
        .get("handoffId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let staged = (|| {
        if let Some(previous) = crate::session_chat_draft_handoffs::result(
            &db,
            &target.project_id,
            &target.session_id,
            &id,
        )? {
            if previous["content"].as_str() != Some(content) {
                return Err(DomainStateError::bad_request(
                    "A draft transfer ID cannot be reused for different text.",
                ));
            }
            if previous["state"] == "placed" || previous["state"] == "received" {
                return Ok(Some(previous));
            }
            return Err(DomainStateError::bad_request(
                "This transfer is already pending. Its text remains in Recovered.",
            ));
        }
        if let Some(version) = crate::session_chat_draft_versions::parse(&params)? {
            crate::session_chat_draft_versions::require_saved(
                &db,
                &target.project_id,
                &target.session_id,
                content,
                &version,
            )?;
        }
        let version = crate::session_chat_draft_versions::parse(&params)?.unwrap_or(
            crate::session_chat_draft_versions::DraftVersion {
                draft_id: uuid::Uuid::new_v4().to_string(),
                revision: 1,
            },
        );
        crate::session_chat_draft_handoffs::stage(
            &db,
            &target.project_id,
            &target.session_id,
            &id,
            content,
            &version,
            "terminal",
        )?;
        Ok(None)
    })();
    match staged {
        Ok(Some(result)) => {
            return routed_json(
                Some(endpoint_path),
                axum::http::StatusCode::OK,
                crate::protocol::rpc_success(request_id, result),
            )
        }
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
        Ok(None) => {}
    }
    match place_session_chat_draft(&target, content, &state.paths.app_state_dir)
        .await
        .and_then(|result| {
            crate::session_chat_draft_handoffs::placed(&db, &id)?;
            Ok(result)
        }) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
