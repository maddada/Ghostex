//! Bring to This Computer: a cloud session continues in a new worktree of its project, in a local
//! Claude session that teleports it (`claude --teleport <session>`), and the cloud row parks.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User picked "into a new worktree; the new local row takes over and the cloud row parks". A new worktree never touches the project folder (teleport needs a clean checkout and checks out the session's branch). The local session runs on the Claude account the cloud session started on, because teleport only finds sessions of the same claude.ai account.

use serde_json::{json, Map, Value};

use super::record::{session_cloud_record, CloudState};
use super::start::{load, open, save};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// `bringBack {projectId, sessionId}` → `{projectId, sessionId, worktreePath, branch}` of the new
/// local session.
pub(crate) async fn bring_cloud_session_back(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = text(params, "projectId").unwrap_or_default();
    let session_id = text(params, "sessionId").unwrap_or_default();
    let (session, record) = {
        let db = open(state)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        load(&repository, &project_id, &session_id)?
    };
    if record.state == CloudState::Moved {
        return Err(DomainStateError::bad_request(
            "This session is already on this computer.",
        ));
    }
    let cloud_id = record
        .cloud_session_id()
        .filter(|_| record.state == CloudState::Running)
        .ok_or_else(|| {
            DomainStateError::bad_request("This cloud session has not started on claude.ai yet.")
        })?;
    let title = session
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Claude cloud session")
        .to_string();
    let mut create = Map::new();
    create.insert("projectId".to_string(), json!(project_id));
    create.insert("agentId".to_string(), json!("claude"));
    create.insert("title".to_string(), json!(title));
    create.insert("teleportCloudSession".to_string(), json!(cloud_id));
    if let Some(account_id) = record.raw.get("accountId").and_then(Value::as_str) {
        create.insert("accountId".to_string(), json!(account_id));
    }
    let created = crate::server::create_worktree_session(state, &create)
        .await
        .map_err(|error| match error {
            crate::server::ProjectWorktreeOperationError::Domain(error) => error,
            crate::server::ProjectWorktreeOperationError::Typed(error) => {
                DomainStateError::bad_request(format!(
                    "Could not make the worktree: {}",
                    error.message
                ))
            }
            crate::server::ProjectWorktreeOperationError::ProjectPath(_) => {
                DomainStateError::bad_request(
                    "Could not make the worktree: the project's folder is not usable.",
                )
            }
        })?;
    let local_session_id = created
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // The cloud row parks and points at the session that took over.
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    if let Some(session) = repository.get_session(&project_id, &session_id)? {
        if let Some(mut record) = session_cloud_record(&session) {
            record.state = CloudState::Moved;
            record.raw.insert(
                "movedTo".to_string(),
                json!({ "projectId": project_id, "sessionId": local_session_id }),
            );
            save(state, &db, &repository, &project_id, &session_id, &record)?;
        }
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id));
        update.insert("sessionId".to_string(), json!(session_id));
        update.insert("isParked".to_string(), json!(true));
        repository.update_session(&update)?;
        let _ = crate::server::schedule_presentation_session_delta(
            state,
            &db,
            &repository,
            &project_id,
            &session_id,
        );
    }
    Ok(json!({
        "projectId": project_id,
        "sessionId": local_session_id,
        "worktreePath": created.get("worktreePath").cloned().unwrap_or(Value::Null),
        "branch": created.get("branch").cloned().unwrap_or(Value::Null),
    }))
}
