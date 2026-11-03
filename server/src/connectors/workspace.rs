//! Which workspace a connector belongs to: every workspace has its own while Workspaces is on;
//! while it is off everything is the default workspace's (Personal), as for the Claude account.
//!
//! CDXC:Connectors 2026-10-15 DECISION:
//! User: connectors are enabled or disabled per workspace (work or personal, for example), and each workspace signs in to Composio on its own, so work apps stay in Work and personal apps in Personal and a work agent never sees personal apps.

use serde_json::Value;

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::workspaces::{
    find_workspace_id, project_workspace_id, read_sidebar_workspaces, workspaces_feature_enabled,
    DEFAULT_WORKSPACE_ID,
};

/// A workspace as the Connectors page lists it.
pub(crate) struct ConnectorWorkspace {
    pub(crate) id: String,
    pub(crate) name: String,
}

fn read_workspaces(paths: &GxserverPaths) -> Result<Value, DomainStateError> {
    let db = crate::storage::open_gxserver_database(paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    read_sidebar_workspaces(&db)
}

fn workspace_name(workspaces: &Value, id: &str) -> String {
    workspaces
        .get("workspaces")
        .and_then(|all| all.get(id))
        .and_then(|workspace| workspace.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "Personal".to_string())
}

/// The workspaces that have connectors, in the sidebar's order: all of them while Workspaces is
/// on, else only the default one.
pub(crate) fn connector_workspaces(
    paths: &GxserverPaths,
) -> Result<Vec<ConnectorWorkspace>, DomainStateError> {
    let workspaces = read_workspaces(paths)?;
    if !workspaces_feature_enabled() {
        return Ok(vec![ConnectorWorkspace {
            id: DEFAULT_WORKSPACE_ID.to_string(),
            name: workspace_name(&workspaces, DEFAULT_WORKSPACE_ID),
        }]);
    }
    let ids = workspaces
        .get("order")
        .and_then(Value::as_array)
        .map(|order| {
            order
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![DEFAULT_WORKSPACE_ID.to_string()]);
    Ok(ids
        .into_iter()
        .map(|id| ConnectorWorkspace {
            name: workspace_name(&workspaces, &id),
            id,
        })
        .collect())
}

/// The workspace whose connectors a project's agents get (a worktree follows its parent
/// checkout); always the default one while Workspaces is off.
pub(crate) fn project_workspace(
    db: &rusqlite::Connection,
    repository: &crate::domain::DomainRepository<'_>,
    project: &Value,
) -> Result<String, DomainStateError> {
    if !workspaces_feature_enabled() {
        return Ok(DEFAULT_WORKSPACE_ID.to_string());
    }
    let workspaces = read_sidebar_workspaces(db)?;
    // Only the parent checkout matters (a worktree follows it); reading every project made each
    // agent tool request slow (2026-10-17, gxserver sampled at 25 sessions).
    let parent = project
        .pointer("/worktree/parentProjectId")
        .and_then(Value::as_str)
        .map(|parent_id| repository.get_project(parent_id))
        .transpose()?
        .flatten();
    Ok(project_workspace_id(
        &workspaces,
        project,
        parent.as_slice(),
    ))
}

/// The workspace a request names, by id or name; none names the default workspace. While
/// Workspaces is off it is always the default workspace.
pub(crate) fn resolve_workspace(
    paths: &GxserverPaths,
    reference: Option<&str>,
) -> Result<String, DomainStateError> {
    let reference = reference.map(str::trim).filter(|text| !text.is_empty());
    if !workspaces_feature_enabled() {
        return Ok(DEFAULT_WORKSPACE_ID.to_string());
    }
    let Some(reference) = reference else {
        return Ok(DEFAULT_WORKSPACE_ID.to_string());
    };
    named_workspace(&read_workspaces(paths)?, reference)
}

/// The workspace `reference` names among `workspaces`. Its error repeats nothing typed: it goes to
/// the request log, and a stale name can spell a credential a workspace stored.
fn named_workspace(workspaces: &Value, reference: &str) -> Result<String, DomainStateError> {
    find_workspace_id(workspaces, reference)
        .ok_or_else(|| DomainStateError::bad_request(crate::workspaces::NO_SUCH_WORKSPACE))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_unknown_workspace_is_named_in_fixed_words() {
        let workspaces = json!({ "workspaces": { "ws-known": { "name": "Work" } } });
        let error = named_workspace(&workspaces, "ws-typed-ref-66").expect_err("unknown");
        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, crate::workspaces::NO_SUCH_WORKSPACE);
        assert!(!error.message.contains("ws-typed-ref-66"));
        assert_eq!(
            named_workspace(&workspaces, " work ").expect("by name"),
            "ws-known"
        );
    }
}
