//! `/api/chatQuestions`: `ghostex question show Q12` (or `G9jt7.Q12` for another chat's) reads
//! one question whole, found as the quote under a message finds it (question.rs).

use serde_json::{json, Map, Value};

use super::question::find_question;
use super::quote::Scope;
use crate::storage::open_gxserver_database;

/// `/api/chatQuestions`: `ghostex question show`.
pub(crate) const CHAT_QUESTIONS_ENDPOINT: &str = "/api/chatQuestions";

/// `/api/chatQuestions`, off the async workers: it reads the database and the chat's transcript.
pub(crate) async fn handle_chat_questions_http(
    state: std::sync::Arc<crate::server::AppState>,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> crate::server::RoutedResponse {
    let params = match crate::domain::read_domain_rpc_params(&body) {
        Ok(params) => params,
        Err(error) => {
            return crate::server::domain_error_response(endpoint_path, request_id, error)
        }
    };
    let answer = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state.paths).map_err(|error| {
            crate::domain::DomainStateError {
                code: "internalError",
                message: format!("SQLite gxserver state error: {error}"),
            }
        })?;
        handle_chat_questions_request(&state, &db, &params)
    })
    .await;
    match answer {
        Ok(Ok(value)) => crate::server::routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(request_id, value),
        ),
        Ok(Err(error)) => crate::server::domain_error_response(endpoint_path, request_id, error),
        Err(error) => crate::server::domain_error_response(
            endpoint_path,
            request_id,
            crate::domain::DomainStateError {
                code: "internalError",
                message: error.to_string(),
            },
        ),
    }
}

/// `/api/chatQuestions {projectId, sessionId, op: "show", ref}`: each question `ref` names (`Q12`,
/// `Q12.Q13`, `G9jt7.Q12`) in the scope of the calling chat (its coordinator's too for a thread),
/// or that other chat's for a session in front.
fn handle_chat_questions_request(
    state: &crate::server::AppState,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<Value, crate::domain::DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let bad = |message: String| crate::domain::DomainStateError {
        code: "invalidParams",
        message,
    };
    let op = text("op");
    if op != "show" {
        return Err(bad(format!(
            "chatQuestions does not know \"{op}\": use show."
        )));
    }
    let written = text("ref");
    let reference = ghostex_gx_chat_core::transcript::item_references::parse_question_href(
        &format!(
            "{}{}",
            ghostex_gx_chat_core::transcript::item_references::QUESTION_LINK_SCHEME,
            written.trim_start_matches('#')
        ),
    )
    .ok_or_else(|| {
        bad(format!(
            "\"{written}\" is not a question tag. Write it as Q12, or G9jt7.Q12 for another chat's."
        ))
    })?;
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let scope = match &reference.session_id {
        Some(other) if *other != session_id => Scope::of_session(db, other).ok_or_else(|| {
            crate::domain::DomainStateError {
                code: "notFound",
                message: format!("This computer has no session {other}."),
            }
        })?,
        _ => Scope::of(db, &project_id, &session_id),
    };
    let mut questions: Vec<Value> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for tag in &reference.tags {
        let written = match &reference.session_id {
            Some(session) => format!("{session}.{tag}"),
            None => tag.clone(),
        };
        match find_question(state, db, &scope, tag) {
            Some(found) => questions.push(found.json(&written)),
            None => missing.push(written),
        }
    }
    if questions.is_empty() {
        return Err(crate::domain::DomainStateError {
            code: "notFound",
            message: format!(
                "Ghostex has nothing tagged {} in this chat.",
                missing.join(", ")
            ),
        });
    }
    Ok(json!({ "questions": questions, "missing": missing }))
}
