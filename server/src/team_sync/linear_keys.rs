//! The Linear keys a Work workspace's team keeps in its Convex project (`linearKeys.ts`): the
//! team's key, which only owners set or remove, and this member's own key, uploaded while
//! "Create my Slack tickets with my own Linear key" is on. Also fills in this member's Linear user
//! from the workspace's Linear key, so tickets the Slack flow creates are assigned to them.
//!
//! CDXC:TeamSync 2026-10-09 DECISION:
//! User: "let's just use 1 key from the owner but also allow the user to override by setting their
//! own key". The member's own key is this workspace's Linear key (crate::work_mode
//! `linear_api_key`), kept in sync whenever that key changes and removed when the switch goes off
//! or the member leaves (`invites:leave` deletes it on the team's side).
//!
//! CDXC:TeamSync 2026-10-09 WHY:
//! Every key is checked with Linear here before it is stored, because Convex mutations cannot call
//! Linear; the same call reads `viewer { id }`, which is the member's Linear user.

use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::logging::{GxserverLogInput, LogLevel};
use crate::paths::GxserverPaths;
use crate::server::AppState;
use crate::work_mode::{linear_api_key, verify_linear_api_key};

use super::connections::{read_team_connections, store_team_connection, TeamConnection};
use super::convex_http::ConvexCallKind;
use super::operations::{connection_for, member_call, required, text};

/// The Linear key this workspace's projects use (the workspace's own, else the shared key).
fn workspace_linear_key(paths: &GxserverPaths, workspace_id: &str) -> Option<String> {
    linear_api_key(paths, None, Some(workspace_id))
}

/// The routes answer "turned off" while the Workspaces built-in extension is off.
fn require_workspaces() -> Result<(), String> {
    if crate::workspaces::workspaces_feature_enabled() {
        return Ok(());
    }
    Err(
        ghostex_settings_catalog::built_in_extensions::turned_off_message(
            ghostex_settings_catalog::built_in_extensions::WORKSPACES,
        ),
    )
}

fn key_args(api_key: &str, account: &Value) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("key".to_string(), json!(api_key));
    if let Some(name) = account.get("name").and_then(Value::as_str) {
        args.insert("linearUserName".to_string(), json!(name));
    }
    args
}

/// `{ workspaceId, apiKey? }`: sets the team's Linear key (owners only; the team refuses anyone
/// else), or removes it when `apiKey` is empty or missing.
pub(crate) fn set_team_linear_key(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    require_workspaces()?;
    let workspace_id = required(params, "workspaceId")?;
    let connection = connection_for(paths, &workspace_id)?;
    let (keys, account) = match text(params, "apiKey") {
        Some(api_key) => {
            let account = verify_linear_api_key(&api_key)?;
            let keys = member_call(
                &connection,
                ConvexCallKind::Mutation,
                "linearKeys:setTeamKey",
                key_args(&api_key, &account),
            )?;
            (keys, Some(account))
        }
        None => (
            member_call(
                &connection,
                ConvexCallKind::Mutation,
                "linearKeys:removeTeamKey",
                Map::new(),
            )?,
            None,
        ),
    };
    Ok(json!({
        "workspaceId": workspace_id,
        "configured": account.is_some(),
        "account": account,
        "linearKeys": keys,
    }))
}

/// `{ workspaceId, enabled }`: turns "Create my Slack tickets with my own Linear key" on (uploads
/// this workspace's Linear key for this member) or off (removes it).
pub(crate) fn set_own_linear_key(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    require_workspaces()?;
    let workspace_id = required(params, "workspaceId")?;
    let enabled = params
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or("Pass enabled: true or false.")?;
    let mut connection = connection_for(paths, &workspace_id)?;
    if enabled && workspace_linear_key(paths, &workspace_id).is_none() {
        return Err(
            "This workspace has no Linear key yet. Save your Linear API key in its Linear row first."
                .to_string(),
        );
    }
    connection.own_linear_key = enabled;
    let keys = if enabled {
        sync_member_linear_key(paths, &connection)?
    } else {
        member_call(
            &connection,
            ConvexCallKind::Mutation,
            "linearKeys:removeMyKey",
            Map::new(),
        )?
    };
    store_team_connection(paths, &connection)
        .map_err(|error| format!("Could not save the team connection: {error}"))?;
    Ok(json!({
        "workspaceId": workspace_id,
        "ownLinearKey": enabled,
        "linearKeys": keys,
    }))
}

/// Brings the team up to date with this workspace's Linear key: the member's Linear user, and,
/// while the switch is on, their own key (removed when the workspace has no key any more).
/// Returns the team's key status, or `Null` when nothing about the keys changed.
fn sync_member_linear_key(
    paths: &GxserverPaths,
    connection: &TeamConnection,
) -> Result<Value, String> {
    let Some(api_key) = workspace_linear_key(paths, &connection.workspace_id) else {
        if !connection.own_linear_key {
            return Ok(Value::Null);
        }
        return member_call(
            connection,
            ConvexCallKind::Mutation,
            "linearKeys:removeMyKey",
            Map::new(),
        );
    };
    let account = verify_linear_api_key(&api_key)?;
    if let Some(linear_user_id) = account.get("id").and_then(Value::as_str) {
        let mut args = Map::new();
        args.insert("linearUserId".to_string(), json!(linear_user_id));
        member_call(
            connection,
            ConvexCallKind::Mutation,
            "teams:setIdentity",
            args,
        )?;
    }
    if !connection.own_linear_key {
        return Ok(Value::Null);
    }
    member_call(
        connection,
        ConvexCallKind::Mutation,
        "linearKeys:setMyKey",
        key_args(&api_key, &account),
    )
}

/// Syncs every connected workspace (or one) in the background after a Linear key changed, a
/// workspace joined or connected a team, or gxserver started. Failures are logged, not shown: the
/// key itself was saved, and the next change or start tries again.
pub(crate) fn spawn_member_linear_key_sync(state: &Arc<AppState>, workspace_id: Option<String>) {
    if !crate::workspaces::workspaces_feature_enabled() {
        return;
    }
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let connections = read_team_connections(&state.paths)
            .into_iter()
            .filter(|connection| {
                workspace_id
                    .as_deref()
                    .is_none_or(|id| id == connection.workspace_id)
            });
        for connection in connections {
            if let Err(error) = sync_member_linear_key(&state.paths, &connection) {
                let _ = state.logger.log(GxserverLogInput {
                    level: LogLevel::Warn,
                    event: "teamLinearKeySyncFailed".to_string(),
                    server_id: Some(state.metadata.server_id.clone()),
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: Some(error),
                    details: Some(json!({ "workspaceId": connection.workspace_id })),
                });
            }
        }
    });
}
