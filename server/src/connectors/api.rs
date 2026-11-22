//! What the Connectors routes do: read every workspace's connectors, sign a workspace in to and
//! out of Composio, list its tools again, and connect one of its apps. All blocking (Composio is
//! called over the network), so the routes run these on `spawn_blocking`.
//!
//! CDXC:Connectors 2026-10-15 DECISION:
//! User: connectors (Composio apps, later the user's own MCP servers) live on a Settings > Connectors page that every agent can use (Claude, Codex and OpenDots' dots), not inside OpenDots only, because they are needed for MCPs and connectors in general.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

use super::composio;
use super::workspace::{connector_workspaces, resolve_workspace};

pub(super) fn workspace_param(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<String, DomainStateError> {
    resolve_workspace(
        &state.paths,
        params.get("workspaceId").and_then(Value::as_str),
    )
}

pub(super) fn failed(message: String) -> DomainStateError {
    DomainStateError::bad_request(message)
}

/// gxserver's own origin on this computer, where Composio sends the browser back.
fn redirect_base(state: &AppState) -> String {
    format!(
        "http://{}:{}",
        state.config.listeners.local.host, state.config.listeners.local.port
    )
}

fn composio_status(state: &AppState, workspace: &str) -> Value {
    let signed_in = composio::signed_in(&state.paths, workspace);
    json!({
        "signedIn": signed_in,
        "unreadable": !signed_in && composio::sign_in_unreadable(&state.paths, workspace),
        "apps": if signed_in { composio::apps(&state.paths, workspace) } else { Vec::new() },
        "suggestedApps": composio::suggested_apps(),
    })
}

/// `/api/readConnectors`: every workspace's connectors (or the one `workspaceId` names), in the
/// sidebar's order. Never waits on Composio.
pub(crate) fn read_connectors(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let only = match params.get("workspaceId").and_then(Value::as_str) {
        Some(_) => Some(workspace_param(state, params)?),
        None => None,
    };
    let workspaces = connector_workspaces(&state.paths)?
        .into_iter()
        .filter(|workspace| only.as_deref().is_none_or(|only| only == workspace.id))
        .map(|workspace| {
            json!({
                "workspaceId": workspace.id,
                "name": workspace.name,
                "composio": composio_status(state, &workspace.id),
                "executor": super::executor::status(&state.paths, &workspace.id),
                "rules": super::agent::connector_rules(state, &workspace.id),
                "cli": super::cli::status(&state.paths, &workspace.id),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "workspacesEnabled": crate::workspaces::workspaces_feature_enabled(),
        "secretsSealedWith": crate::vault::vault_backend(&state.paths),
        "workspaces": workspaces,
    }))
}

/// `/api/signInComposio`: the page to open in the browser, or none when already signed in.
pub(crate) fn sign_in_composio(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let url = composio::sign_in(&state.paths, &workspace, &redirect_base(state)).map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "signInUrl": url }))
}

/// `/api/signOutComposio`.
pub(crate) fn sign_out_composio(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    composio::sign_out(&state.paths, &workspace);
    Ok(json!({ "workspaceId": workspace, "signedIn": false }))
}

/// `/api/refreshComposio`: lists the workspace's tools and connected apps again.
pub(crate) fn refresh_composio(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    composio::refresh(&state.paths, &workspace).map_err(failed)?;
    Ok(json!({
        "workspaceId": workspace,
        "composio": composio_status(state, &workspace),
    }))
}

/// `/api/connectComposioApp`: the sign-in page for one app (`app`: a Composio toolkit slug such
/// as `gmail`), or `alreadyConnected`.
pub(crate) fn connect_composio_app(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let app = params
        .get("app")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let started = composio::start_connect(&state.paths, &workspace, app).map_err(failed)?;
    Ok(json!({
        "workspaceId": workspace,
        "app": app.trim().to_lowercase(),
        "name": started.name,
        "alreadyConnected": started.already_connected,
        "url": started.url,
    }))
}

/// `/api/listComposioTools`: the tools the workspace's agents would get, by name, or whole
/// (`full`: name, description and input schema).
pub(crate) fn list_composio_tools(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    if !composio::signed_in(&state.paths, &workspace) {
        return Err(failed(
            if composio::sign_in_unreadable(&state.paths, &workspace) {
                composio::UNREADABLE
            } else {
                composio::NOT_CONNECTED
            }
            .to_string(),
        ));
    }
    let tools = composio::mcp_tools(&state.paths, &workspace);
    if params.get("full").and_then(Value::as_bool) == Some(true) {
        return Ok(json!({ "workspaceId": workspace, "tools": tools }));
    }
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
        .collect::<Vec<_>>();
    Ok(json!({ "workspaceId": workspace, "tools": names }))
}

/// `/api/callComposioTool`: runs one of the workspace's Composio tools (`tool`, `arguments`) and
/// returns its output as text.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// The user decided Ghostex asks with its own card before a connector sends, posts, creates, changes or deletes anything; until that card exists, such calls are refused here instead of running unasked.
pub(crate) fn call_composio_tool(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let tool = params
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if tool.is_empty() {
        return Err(failed("Name the tool to call.".to_string()));
    }
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if composio::call_decision(tool, &arguments) == composio::ComposioDecision::Ask {
        return Err(failed(format!(
            "{} could change something in your apps (send, post, create, edit or delete). Ghostex can't ask you about those yet, so only reads run for now.",
            composio::call_label(tool, &arguments)
        )));
    }
    let output = composio::call_tool(&state.paths, &workspace, tool, &arguments).map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "tool": tool, "output": output }))
}

/// The apps (Composio slugs) connected in `workspace`, as Connectors last saw them; none while it
/// isn't signed in. No network call (a due refresh runs in the background).
pub(crate) fn composio_connected_apps(
    paths: &crate::paths::GxserverPaths,
    workspace: &str,
) -> Vec<String> {
    if !composio::signed_in(paths, workspace) {
        return Vec::new();
    }
    composio::apps(paths, workspace)
        .iter()
        .filter(|app| app["connected"] == Value::Bool(true))
        .filter_map(|app| app["slug"].as_str().map(str::to_string))
        .collect()
}
