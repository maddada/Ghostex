use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::ids::{is_gxserver_project_id, is_gxserver_session_id};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, AppState, RoutedResponse,
};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::storage::open_gxserver_database;

const MAX_TRANSCRIPT_SIZE_TARGETS: usize = 32;

#[derive(Clone)]
struct TranscriptSizeTarget {
    project_id: String,
    session_id: String,
}

/*
CDXC:QuickAccessSessionSizes 2026-08-27:
Quick Access asks for transcript sizes only after session rows enter the visible
scroll window. This endpoint keeps that deferred read cheap and private: one
SQLite connection resolves a bounded batch of stable ids, and the response
contains only those ids plus filesystem metadata, never transcript paths or
content. Path discovery and stat calls run off the async server thread.
*/
pub(crate) async fn handle_read_session_transcript_sizes_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let Some(sessions) = params.get("sessions").and_then(Value::as_array) else {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "readSessionTranscriptSizes requires a sessions array.".to_string(),
            },
        );
    };
    if sessions.is_empty() || sessions.len() > MAX_TRANSCRIPT_SIZE_TARGETS {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: format!(
                    "readSessionTranscriptSizes accepts 1 to {MAX_TRANSCRIPT_SIZE_TARGETS} sessions."
                ),
            },
        );
    }

    let mut targets = Vec::with_capacity(sessions.len());
    for session in sessions {
        let project_id = session
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let session_id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !is_gxserver_project_id(project_id) || !is_gxserver_session_id(session_id) {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: "readSessionTranscriptSizes received an invalid session reference."
                        .to_string(),
                },
            );
        }
        targets.push(TranscriptSizeTarget {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        });
    }

    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, server_id.as_str());
        let sessions = targets
            .into_iter()
            .map(|target| {
                let size_bytes = repository
                    .get_session(&target.project_id, &target.session_id)
                    .ok()
                    .flatten()
                    .and_then(|session| {
                        let agent = session_chat_agent_for_session(&session)?;
                        let transcript_agent =
                            crate::session_chat::resolve_session_chat_transcript_agent(Some(
                                &agent,
                            ))?;
                        crate::session_chat::resolve_session_chat_transcript_path(
                            transcript_agent,
                            read_runtime_text(&session, "agentSessionId").as_deref(),
                            read_runtime_text(&session, "agentSessionPath").as_deref(),
                        )
                    })
                    .and_then(|path| std::fs::metadata(path).ok())
                    .filter(|metadata| metadata.is_file())
                    .map(|metadata| metadata.len());
                json!({
                    "projectId": target.project_id,
                    "sessionId": target.session_id,
                    "sizeBytes": size_bytes,
                })
            })
            .collect::<Vec<_>>();
        Ok::<_, DomainStateError>(json!({ "sessions": sessions }))
    })
    .await;

    match result {
        Ok(Ok(result)) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Ok(Err(error)) => domain_error_response(endpoint_path, request_id, error),
        Err(_) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: "The transcript size read did not finish.".to_string(),
            },
        ),
    }
}
