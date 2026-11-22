//! What the Executor routes do: turn a workspace's Executor on or off and manage its apps, their
//! sign-ins and tools. All blocking (Executor may have to start first), so the routes run these
//! on `spawn_blocking`.
//!
//! CDXC:Connectors 2026-10-15 DECISION:
//! User: Executor is a first-class connector next to Composio, on its v2 beta now, and Ghostex downloads it the first time a workspace turns it on rather than shipping it inside the app.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

use super::api::{failed, workspace_param};
use super::executor;

fn text_param<'a>(params: &'a Map<String, Value>, key: &str) -> &'a str {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

/// `/api/enableExecutor`: turns the workspace's Executor on, downloading Executor first when this
/// computer has none (the download runs in the background; `readConnectors` shows its progress).
pub(crate) fn enable_executor(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let executor = executor::enable(&state.paths, &workspace).map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "executor": executor }))
}

/// `/api/disableExecutor`: stops the workspace's Executor and keeps its data.
pub(crate) fn disable_executor(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let executor = executor::disable(&state.paths, &workspace).map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "executor": executor }))
}

/// `/api/searchExecutorCatalog`: catalog apps matching `query`.
pub(crate) fn search_executor_catalog(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let apps = executor::search_catalog(&state.paths, &workspace, text_param(params, "query"))
        .map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "apps": apps }))
}

/// `/api/addExecutorApp`: adds a catalog app (`app`: its catalog id or name).
pub(crate) fn add_executor_app(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let app = executor::add_catalog_app(&state.paths, &workspace, text_param(params, "app"))
        .map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "app": app }))
}

/// `/api/addExecutorMcpServer`: adds a remote MCP server (`name`, `url`).
pub(crate) fn add_executor_mcp_server(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let app = executor::add_mcp_server(
        &state.paths,
        &workspace,
        text_param(params, "name"),
        text_param(params, "url"),
    )
    .map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "app": app }))
}

/// `/api/removeExecutorApp`: removes an app (`app`: its name, slug or id).
pub(crate) fn remove_executor_app(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let name = executor::remove_app(&state.paths, &workspace, text_param(params, "app"))
        .map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "removed": name }))
}

/// `/api/connectExecutorApp`: the sign-in page for an app, or `alreadyConnected`.
pub(crate) fn connect_executor_app(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let started = executor::connect_app(&state.paths, &workspace, text_param(params, "app"))
        .map_err(failed)?;
    Ok(json!({
        "workspaceId": workspace,
        "name": started.name,
        "alreadyConnected": started.url.is_none(),
        "url": started.url,
    }))
}

/// `/api/listExecutorTools`: one app's tools (`app`), or every app's.
pub(crate) fn list_executor_tools(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let app = Some(text_param(params, "app")).filter(|app| !app.is_empty());
    let apps = executor::list_tools(&state.paths, &workspace, app).map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "apps": apps }))
}

/// `/api/callExecutorTool`: runs one of an app's read-only tools (`app`, `tool`, `input`).
pub(crate) fn call_executor_tool(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let tool = text_param(params, "tool");
    if tool.is_empty() {
        return Err(failed("Name the tool to call.".to_string()));
    }
    let input = params.get("input").cloned().unwrap_or_else(|| json!({}));
    let output = executor::call_tool(
        &state.paths,
        &workspace,
        text_param(params, "app"),
        tool,
        &input,
    )
    .map_err(failed)?;
    Ok(json!({ "workspaceId": workspace, "tool": tool, "output": output }))
}
