//! Move to project: `/api/moveSessionToProject { projectId, sessionId, targetProjectId }` takes a
//! chat out of No project into one of the user's projects.
//!
//! CDXC:Projects 2026-11-27 DECISION:
//! User (Q5262 B only, not Turn into project): a chat in No project can move to an existing project: pick one of your projects and the chat resumes in that project's folder with its history, under a new session ref. Its files stay where they are (it worked in ~, so nothing is copied).
//! CDXC:Projects 2026-11-27 WHY:
//! How the conversation follows depends on the agent. Claude Code resumes a conversation by its id from any folder and keeps writing the transcript it has (verified with Claude Code 2.1.296, project_folder_move/repair.rs), and Codex resumes by id and takes the new folder from `-C` (agents/resume_plan.rs `with_codex_moved_cwd`, which reads `movedFromNoProject`). Those chats move in place: every row Ghostex keeps about the chat (draft, queue, notes, side chats) is re-keyed to the project (upgrade.rs `rekey_sessions`), its folder becomes the project's, and a running one is put to sleep and woken there. No other agent is known to resume another folder's conversation, so for them Move to project is a handover (`start_fresh`).

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::upgrade::{rekey_sessions, session_id_taken};
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, internal_rpc, routed_json, schedule_presentation_project_delta,
    AppState, RoutedResponse,
};
use crate::storage::open_gxserver_database;

/// The runtime mark on a chat that left No project: `{ projectId, sessionId, movedAt }` of where it
/// came from. Codex's resume reads it to start in the new folder.
pub(crate) const MOVED_FROM_NO_PROJECT_KEY: &str = "movedFromNoProject";

pub(crate) async fn handle_move_session_to_project(
    state: Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match move_session(&state, params).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn required(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("moveSessionToProject needs {key}.")))
}

fn internal(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError::corrupt_state(error.to_string())
}

struct Plan {
    project_id: String,
    session_id: String,
    session: Value,
    target: Value,
    /// The agent can take its conversation to the new folder.
    follows: bool,
    running: bool,
    side_chats: Vec<String>,
}

fn plan(state: &AppState, params: &Map<String, Value>) -> Result<Plan, DomainStateError> {
    let project_id = required(params, "projectId")?;
    let session_id = required(params, "sessionId")?;
    let target_reference = required(params, "targetProjectId")?;
    let db = open_gxserver_database(&state.paths).map_err(internal)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    // An id from the clients, or a name or folder from `ghostex move-session-to-project --to`.
    let target_id = crate::workspaces::resolve_project_reference(&repository, &target_reference)?;
    let project = repository
        .get_project(&project_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("No project {project_id}.")))?;
    if !super::is_no_project_project(&project) {
        return Err(DomainStateError::bad_request(
            "Only a chat in No project can move to a project.",
        ));
    }
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::not_found("That chat no longer exists."))?;
    if text(&session, "kind") != "agent" {
        return Err(DomainStateError::bad_request(
            "Only a chat can move to a project; a terminal can't.",
        ));
    }
    let target = repository
        .get_project(&target_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("No project {target_id}.")))?;
    let system = super::is_no_project_project(&target)
        || crate::chief::lists_under_main_hub(&target)
        || target
            .get("gbotsProject")
            .is_some_and(|value| !value.is_null());
    if system {
        return Err(DomainStateError::bad_request(
            "Pick one of your own projects to move the chat to.",
        ));
    }
    let folder = text(&target, "path");
    if folder.is_empty() || !std::path::Path::new(&folder).is_dir() {
        return Err(DomainStateError::bad_request(format!(
            "{}'s folder is missing, so the chat can't move there.",
            text(&target, "name")
        )));
    }
    let family = crate::agents::session_agent_family_id(&project, &session);
    let follows = matches!(family.as_deref(), Some("claude" | "codex"));
    let side_chats = repository
        .list_sessions(Some(&project_id))?
        .iter()
        .filter(|chat| crate::side_chats::parent_of(chat).as_deref() == Some(session_id.as_str()))
        .map(|chat| text(chat, "sessionId"))
        .collect();
    Ok(Plan {
        running: text(&session, "lifecycleState") == "running",
        project_id,
        session_id,
        session,
        target,
        follows,
        side_chats,
    })
}

async fn lifecycle(
    state: &Arc<AppState>,
    path: &str,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    internal_rpc(
        state,
        path,
        json!({"projectId": project_id, "sessionId": session_id, "reason": "moveToProject"}),
    )
    .await
    .map_err(|error| DomainStateError::bad_request(format!("The chat could not be moved: {error}")))
}

async fn move_session(
    state: &Arc<AppState>,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let worker = state.clone();
    let plan = tokio::task::spawn_blocking(move || plan(&worker, &params))
        .await
        .map_err(internal)??;
    let moved_from = json!({
        "projectId": plan.project_id,
        "sessionId": plan.session_id,
        "movedAt": crate::coordinators::now_iso(),
    });
    if plan.running {
        lifecycle(
            state,
            "/api/sleepSession",
            &plan.project_id,
            &plan.session_id,
        )
        .await?;
    }
    if plan.follows {
        let running = plan.running;
        let worker = state.clone();
        let moved = tokio::task::spawn_blocking(move || move_in_place(&worker, &plan, moved_from))
            .await
            .map_err(internal)??;
        let (project_id, session_id) = (text(&moved, "projectId"), text(&moved, "sessionId"));
        if running {
            // It was working when it moved: it carries on in the new folder.
            let _ = lifecycle(state, "/api/wakeSession", &project_id, &session_id).await;
        }
        return Ok(json!({
            "moved": "resumed",
            "session": moved,
            "ref": format!("{}:{project_id}:{session_id}", state.metadata.server_id),
        }));
    }
    start_fresh(state, &plan, moved_from).await
}

/// Claude and Codex: the chat itself goes to the project.
fn move_in_place(
    state: &AppState,
    plan: &Plan,
    moved_from: Value,
) -> Result<Value, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(internal)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let target_id = text(&plan.target, "projectId");
    let folder = text(&plan.target, "path");
    let mut new_ids: Vec<String> = Vec::new();
    let mut fresh_id = |old: &str| -> String {
        let mut id = old.to_string();
        while session_id_taken(&db, &target_id, &id) || new_ids.contains(&id) {
            id = crate::ids::create_session_id();
        }
        new_ids.push(id.clone());
        id
    };
    let moves: Vec<(String, String)> = std::iter::once(plan.session_id.clone())
        .chain(plan.side_chats.iter().cloned())
        .map(|old| {
            let new = fresh_id(&old);
            (old, new)
        })
        .collect();
    rekey_sessions(
        &db,
        &state.metadata.server_id,
        &plan.project_id,
        &target_id,
        &moves,
    )?;
    let new_session_id = moves[0].1.clone();
    for (index, (_, id)) in moves.iter().enumerate() {
        let Some(row) = repository.get_session(&target_id, id)? else {
            continue;
        };
        let mut runtime = row
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime.insert(MOVED_FROM_NO_PROJECT_KEY.to_string(), moved_from.clone());
        if index > 0 {
            runtime.insert(
                crate::side_chats::PARENT_KEY.to_string(),
                json!(new_session_id),
            );
        }
        let mut update = Map::new();
        update.insert("projectId".into(), json!(target_id));
        update.insert("sessionId".into(), json!(id));
        update.insert("cwd".into(), json!(folder));
        update.insert("runtimeSettings".into(), Value::Object(runtime));
        repository.update_session(&update)?;
    }
    for project_id in [&plan.project_id, &target_id] {
        let _ = schedule_presentation_project_delta(
            state,
            &db,
            &repository,
            project_id,
            "projectUpdated",
        );
    }
    repository
        .get_session(&target_id, &new_session_id)?
        .ok_or_else(|| DomainStateError::corrupt_state("The moved chat is missing."))
}

/// Every other agent: a handover. The conversation is written to a Markdown file (the user's
/// messages and the agent's final reply of each turn, no tool calls or their output), a new chat of
/// the same agent starts in the project with that file's link in its message box, and the old chat
/// is closed (it stays in No project's history).
///
/// CDXC:Projects 2026-11-27 DECISION:
/// User (Q5284 note): "if agent doesn't suport u should do a handover fam (just do an md we start form). shouldn't include tool calls should include basically agent final reply and chat". The handover is Ghostex's own (session_transcript_export with the `prompts_and_answers` preset, and the first message every handoff starts with, gx-core `handoff_draft`), left in the box unsent like every handoff's, and the source closes as a handoff's does by default.
async fn start_fresh(
    state: &Arc<AppState>,
    plan: &Plan,
    moved_from: Value,
) -> Result<Value, DomainStateError> {
    let old_ref = format!(
        "{}:{}:{}",
        state.metadata.server_id, plan.project_id, plan.session_id
    );
    let title = text(&plan.session, "title");
    let transcript = internal_rpc(
        state,
        "/api/exportSessionTranscript",
        json!({
            "projectId": plan.project_id,
            "sessionId": plan.session_id,
            "preset": "prompts_and_answers",
            "finalAnswerOnly": true,
            "includeCommands": false,
            "includePatches": false,
            "includeReasoning": false,
        }),
    )
    .await
    .map_err(|error| {
        DomainStateError::bad_request(format!(
            "This agent can't take its conversation to another folder, and Ghostex could not write a handover for it: {error}"
        ))
    })
    .map(|exported| text(&exported, "path"))?;
    if transcript.is_empty() {
        return Err(DomainStateError::bad_request(
            "This agent can't take its conversation to another folder, and Ghostex could not write a handover for it.",
        ));
    }
    let draft = ghostex_gx_core::session_handoff::handoff_draft(
        &transcript,
        &title,
        super::project::NO_PROJECT_NAME,
    );
    let mut runtime = Map::new();
    runtime.insert("firstUserInputDraft".into(), json!(draft));
    runtime.insert(MOVED_FROM_NO_PROJECT_KEY.to_string(), moved_from);
    let create = json!({
        "projectId": text(&plan.target, "projectId"),
        "agentId": text(&plan.session, "agentId"),
        "title": title,
        "runtimeSettings": runtime,
    });
    let created = internal_rpc(state, "/api/createAgentSession", create)
        .await
        .map_err(|error| {
            DomainStateError::bad_request(format!(
                "The chat could not start in the project: {error}"
            ))
        })?;
    let session = created.get("session").cloned().unwrap_or(created);
    let (project_id, session_id) = (text(&session, "projectId"), text(&session, "sessionId"));
    // The handover is the chat now; the old one closes into No project's history.
    let _ = lifecycle(
        state,
        "/api/killSession",
        &plan.project_id,
        &plan.session_id,
    )
    .await;
    Ok(json!({
        "moved": "handover",
        "session": session,
        "ref": format!("{}:{project_id}:{session_id}", state.metadata.server_id),
        "previous": {"projectId": plan.project_id, "sessionId": plan.session_id, "ref": old_ref},
        "handoverPath": transcript,
    }))
}
