//! `/api/chief`: `{ op: "status" | "guide" | "open" | "newChat" | "setAgent" | "prompt", agentId?, prompt? }`.
//!
//! - `status` (any caller): whether Chief is on, its record, the agents it can run on, and the
//!   one its first open starts on (`defaultAgentId`, the user's default agent).
//! - `guide` (any caller): Chief's playbook.
//! - `open { agentId? }` (no agent: the default one), `newChat`, `setAgent { agentId }`,
//!   `prompt { prompt, agentId? }` (opens Chief if needed and sends `prompt`): only the owner's
//!   devices and the user's own terminal (the token file). No session may create or move Chief,
//!   Chief included, and none of them while Chief is turned off.

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
    let changes = matches!(op.as_str(), "open" | "newChat" | "setAgent" | "prompt");
    if changes && !may_change_chief(caller) {
        return error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "forbidden",
                message: "Only you can open Ghostex AI, start a new chat with it or change its agent, from its row in the sidebar.".to_string(),
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
    if op == "prompt" {
        return match prompt_chief(state, &params).await {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => error_response(endpoint_path, request_id, error),
        };
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
                message: format!("Ghostex AI could not answer: {error}"),
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
        "guide" => Ok(json!({
            "guide": super::chief_playbook(crate::gbots::gbots_on(&state.paths))
        })),
        "open" => open_chief(state, agent_id),
        "newChat" => replace_chief(state, None),
        "setAgent" => {
            if agent_id.is_none_or(|id| id.trim().is_empty()) {
                return Err(DomainStateError::bad_request(
                    "Pass the agentId Ghostex AI should run on.",
                ));
            }
            replace_chief(state, agent_id)
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown Ghostex AI op {other}: use status, guide, open, newChat, setAgent or prompt."
        ))),
    }
}

/// Opens Ask Ghostex AI (creating its session on the default agent when needed) and sends `prompt`.
///
/// When Chief still holds its unstarted first message (just created or never started), the held
/// greeting is replaced with a single startup prompt that keeps Chief's coordinator/guide intro
/// and hands it `prompt` directly, avoiding an extra Getting Started form turn. When Chief already
/// has an ongoing conversation, `prompt` is sent right away (waking it if asleep).
async fn prompt_chief(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let prompt = params
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("Pass the prompt to send to Ghostex AI."))?
        .to_string();
    let agent_id = params
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let worker = state.clone();
    let mut result =
        tokio::task::spawn_blocking(move || open_chief(&worker, agent_id.as_deref()))
            .await
            .map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("Ghostex AI could not answer: {error}"),
            })??;
    let (project_id, session_id) = result["chief"]["projectId"]
        .as_str()
        .zip(result["chief"]["sessionId"].as_str())
        .map(|(project, session)| (project.to_string(), session.to_string()))
        .ok_or_else(|| DomainStateError {
            code: "internalError",
            message: "Ghostex AI opened without a session.".to_string(),
        })?;
    if let Some(first_message) =
        crate::session_chat_first_message::edit(state, &project_id, &session_id).await?
    {
        const GREETING_TAIL: &str =
            "Then greet the user as its \"Your first reply\" section says.";
        let replacement = format!(
            "Skip the getting-started greeting and handle this request from the user right away:\n\n{prompt}"
        );
        let combined = if first_message.contains(GREETING_TAIL) {
            first_message.replacen(GREETING_TAIL, &replacement, 1)
        } else {
            format!("{first_message}\n\n{replacement}")
        };
        crate::server::queue_session_chat_prompt(state, &project_id, &session_id, &combined, true)?;
    } else {
        let body = json!({
            "params": {
                "projectId": project_id,
                "sessionId": session_id,
                "text": prompt,
                "sendRequestId": uuid::Uuid::new_v4().to_string(),
            }
        });
        let routed = crate::session_chat_send::handle_send_session_chat_message_http(
            state,
            "/api/sendSessionChatMessage".to_string(),
            uuid::Uuid::new_v4().to_string(),
            &body,
        )
        .await;
        if !routed.response.status().is_success() {
            return Err(DomainStateError {
                code: "internalError",
                message: "Ghostex AI opened, but the prompt could not be sent.".to_string(),
            });
        }
    }
    result["sent"] = json!(true);
    Ok(result)
}

fn status(state: &AppState) -> Result<Value, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let record =
        read_record(&db).filter(|record| super::session::session_open(&repository, record));
    let earlier_chats = super::session::earlier_chats(&repository, &db);
    // Only agents whose CLI is installed are offered (CDXC:AgentLauncher 2026-11-21 in
    // agents/cli_missing.rs).
    let agents: Vec<Value> = chief_agent_rows(&repository.list_projects()?)
        .into_iter()
        .filter(|row| {
            !crate::agents::launcher_row_cli_missing(row, state.paths.agent_config_home_dir())
        })
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
        // The agent its first open starts on (model.rs); clients that see it skip the agent
        // question.
        "defaultAgentId": super::model::agent_for_next_chat(
            &state.paths,
            &db,
            &chief_agent_rows(&repository.list_projects()?),
        ),
        "agents": agents,
        // Its earlier chats still open (asleep), newest first, for the row's Earlier Chats.
        "earlierChats": earlier_chats,
    }))
}
