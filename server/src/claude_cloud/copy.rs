//! Start a Copy on Another Account: a new cloud session that carries on a session whose account
//! ran out of usage, on the same branch, with a summary task, on the account the user picked. The
//! original stays where it is (CDXC:ClaudeCloud in accounts.rs, the user's decision).

use serde_json::{json, Map, Value};

use super::record::{CloudState, RECORD_KEY};
use super::start::{load, open};
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

/// `copy {projectId, sessionId, accountId}` → `{projectId, sessionId}` of the new session, which
/// the caller starts.
pub(crate) fn create_cloud_copy(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(String, String), DomainStateError> {
    let project_id = text(params, "projectId").unwrap_or_default();
    let session_id = text(params, "sessionId").unwrap_or_default();
    let account_id = text(params, "accountId")
        .ok_or_else(|| DomainStateError::bad_request("Pick the account the copy runs on."))?;
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let (session, record) = load(&repository, &project_id, &session_id)?;
    if record.state != CloudState::Running {
        return Err(DomainStateError::bad_request(
            "Only a cloud session that is running on claude.ai can be copied.",
        ));
    }
    if record.raw.get("accountId").and_then(Value::as_str) == Some(account_id.as_str()) {
        return Err(DomainStateError::bad_request(
            "The copy has to run on another account.",
        ));
    }
    let branch = record.branch.clone().ok_or_else(|| {
        DomainStateError::bad_request("This session has no branch to carry on from.")
    })?;
    let original_task = record
        .raw
        .get("task")
        .and_then(Value::as_str)
        .unwrap_or("(not recorded)");
    let original_account = record
        .raw
        .get("accountName")
        .and_then(Value::as_str)
        .unwrap_or("its account");
    let summary = format!(
        "This carries on the Claude cloud session {url}, which stopped because {original_account} ran out of usage.\n\nIts task was:\n\n{original_task}\n\nIts work so far is on the branch `{branch}`. Read what it did there (its commits and its diff against the default branch), then carry on from where it stopped and push your commits to `{branch}`.",
        url = record.url.as_deref().unwrap_or("(no link)"),
    );
    let title = format!(
        "{} (copy)",
        session
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Claude cloud session")
    );
    let mut create = Map::new();
    create.insert("projectId".to_string(), json!(project_id));
    create.insert("agentId".to_string(), json!(super::CLAUDE_CLOUD_AGENT_ID));
    create.insert("title".to_string(), json!(title));
    create.insert(
        "runtimeSettings".to_string(),
        json!({ "accountId": account_id, "firstUserMessage": summary }),
    );
    let create = super::cloud_create_params(&repository, &create)?.unwrap_or(create);
    let project = repository.resolve_create_session_project(&create)?;
    let mut resolved =
        crate::agents::create_agent_session_params_for_project(&db, &project, &create)?;
    if let Some(record) = resolved
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
        .and_then(|runtime| runtime.get_mut(RECORD_KEY))
        .and_then(Value::as_object_mut)
    {
        // Works on the original's branch, which GitHub already has.
        record.insert("branch".to_string(), json!(branch));
        record.insert("onBranch".to_string(), json!(true));
        record.insert(
            "copyOf".to_string(),
            json!({ "projectId": project_id, "sessionId": session_id }),
        );
    }
    if let Some(runtime) = resolved
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
    {
        runtime.insert("titleSource".to_string(), json!("user"));
    }
    let created = repository.create_session(&resolved, false)?;
    let field = |key: &str| {
        created
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (new_project, new_session) = (field("projectId"), field("sessionId"));
    let _ = crate::server::schedule_presentation_session_delta(
        state,
        &db,
        &repository,
        &new_project,
        &new_session,
    );
    Ok((new_project, new_session))
}
