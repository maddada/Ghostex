//! `/api/chief`: `{ op: "status" | "guide" | "open" | "newChat" | "setAgent", agentId? }`.
//!
//! - `status` (any caller): whether Chief is on, its record, and the agents it can run on (the
//!   first-open picker's list).
//! - `guide` (any caller): Chief's playbook.
//! - `open { agentId? }`, `newChat`, `setAgent { agentId }`: only the owner's devices and the
//!   user's own terminal (the token file). No session may create or move Chief, Chief included,
//!   and none of them while Chief is turned off.

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::record::read_record;
use super::session::{chief_agent_rows, open_chief, replace_chief};
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::{rpc_error, rpc_success};
use crate::security::Principal;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

pub(crate) const CHIEF_ENDPOINT: &str = "/api/chief";

/// Whether `caller` may create, renew or move Chief's session.
pub(crate) fn may_change_chief(caller: &Principal) -> bool {
    matches!(
        caller,
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent
    )
}

pub(crate) async fn handle_chief_http(
    state: &AppState,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let op = params
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or("status")
        .to_string();
    let changes = matches!(op.as_str(), "open" | "newChat" | "setAgent");
    if changes && !may_change_chief(caller) {
        return error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "forbidden",
                message: "Only you can open Chief of Staff, start a new chat with it or change its agent, from its row in the sidebar.".to_string(),
            },
        );
    }
    if changes && !super::chief_on(&state.paths) {
        return error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "turnedOff",
                message: super::chief_off_message(),
            },
        );
    }
    let worker = state.clone();
    let outcome = tokio::task::spawn_blocking(move || run(&worker, &op, &params)).await;
    match outcome {
        Ok(Ok(result)) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Ok(Err(error)) => error_response(endpoint_path, request_id, error),
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: format!("Chief of Staff could not answer: {error}"),
            },
        ),
    }
}

/// Chief's own refusals carry their HTTP status (403 for a caller that may not, 409 while it is
/// off or before its agent is picked); everything else is a domain error as usual.
fn error_response(
    endpoint_path: String,
    request_id: String,
    error: DomainStateError,
) -> RoutedResponse {
    let status = match error.code {
        "forbidden" => StatusCode::FORBIDDEN,
        "turnedOff" | "chiefNeedsAgent" => StatusCode::CONFLICT,
        _ => return domain_error_response(endpoint_path, request_id, error),
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

fn run(state: &AppState, op: &str, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let agent_id = params.get("agentId").and_then(Value::as_str);
    match op {
        "status" => status(state),
        "guide" => Ok(json!({ "guide": super::CHIEF_PLAYBOOK })),
        "open" => open_chief(state, agent_id),
        "newChat" => replace_chief(state, None),
        "setAgent" => {
            if agent_id.is_none_or(|id| id.trim().is_empty()) {
                return Err(DomainStateError::bad_request(
                    "Pass the agentId Chief of Staff should run on.",
                ));
            }
            replace_chief(state, agent_id)
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown Chief of Staff op {other}: use status, guide, open, newChat or setAgent."
        ))),
    }
}

fn status(state: &AppState) -> Result<Value, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let record = read_record(&db).filter(|record| {
        repository
            .get_session(&record.project_id, &record.session_id)
            .ok()
            .flatten()
            .is_some()
    });
    let agents: Vec<Value> = chief_agent_rows(&repository.list_projects()?)
        .into_iter()
        .map(|row| {
            json!({
                "agentId": row.get("agentId").cloned().unwrap_or(Value::Null),
                "name": row.get("name").cloned().unwrap_or(Value::Null),
                "icon": row.get("icon").cloned().unwrap_or(Value::Null),
                "family": crate::coordinators::coordinator_launcher_family(&row),
            })
        })
        .collect();
    Ok(json!({
        "enabled": super::chief_on(&state.paths),
        "title": super::CHIEF_TITLE,
        "serverId": state.metadata.server_id.as_str(),
        "chief": record,
        "globalRef": record.as_ref().map(|record| crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            &record.project_id,
            &record.session_id,
        )),
        "needsAgent": record.is_none(),
        "agents": agents,
    }))
}
