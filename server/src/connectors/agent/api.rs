//! What the agent-tool routes do: list a session's connector tools, run one (the call route is
//! async because it may wait on a card), cancel a waiting call, and read or change a workspace's
//! Ask/Allow/Never rules.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

use super::super::api::{failed, workspace_param};
use super::super::composio;
use super::super::executor::agent as executor;
use super::rules::{self, Rule};

/// How long a workspace's tool list is reused while nothing about its connectors changed.
const TOOLS_FRESH_FOR: Duration = Duration::from_secs(60);
/// How long `waitConnectorAgentTools` holds a request before answering anyway.
const WAIT_AT_MOST: Duration = Duration::from_secs(4 * 60);

/// A workspace's tools, with the revision and time they were read.
static TOOLS: Mutex<Option<HashMap<String, (u64, Instant, Vec<Value>)>>> = Mutex::new(None);

fn workspace_tools(state: &AppState, workspace: &str) -> Vec<Value> {
    let revision = super::revision::tools_revision();
    let cached = TOOLS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .get(workspace)
        .filter(|(read_at_revision, at, _)| {
            *read_at_revision == revision && at.elapsed() < TOOLS_FRESH_FOR
        })
        .map(|(_, _, tools)| tools.clone());
    if let Some(tools) = cached {
        return tools;
    }
    let mut tools = composio::mcp_tools(&state.paths, workspace);
    if executor::enabled(&state.paths, workspace) {
        // A workspace whose Executor can't start still gets its Composio tools.
        if let Ok(executor_tools) = executor::agent_tools(&state.paths, workspace) {
            tools.extend(executor_tools);
        }
    }
    TOOLS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(
            workspace.to_string(),
            (revision, Instant::now(), tools.clone()),
        );
    tools
}

/// `/api/listConnectorAgentTools`: the tools a session's agents get from its workspace's
/// connectors (`{ name, description, inputSchema }` each), and the revision to wait past for the
/// next change. Blocking.
pub(crate) fn list_connector_agent_tools(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let session = super::session::resolve_cached(state, params)?;
    let revision = super::revision::tools_revision();
    let tools = workspace_tools(state, &session.workspace);
    Ok(json!({ "workspaceId": session.workspace, "tools": tools, "revision": revision }))
}

/// `/api/waitConnectorAgentTools`: answers once any workspace's connectors changed since
/// `revision`, after a few minutes, or when gxserver stops (a held request would keep its
/// shutdown waiting), with the revision then.
pub(crate) async fn wait_connector_agent_tools(
    state: &AppState,
    params: Map<String, Value>,
) -> Value {
    let seen = params.get("revision").and_then(Value::as_u64).unwrap_or(0);
    let shutdown = state.shutdown_tx.subscribe();
    json!({ "revision": super::revision::wait_past(seen, WAIT_AT_MOST, shutdown).await })
}

/// `/api/callConnectorAgentTool`: runs one (`tool`, `arguments`, `callId`) and returns its MCP
/// result.
pub(crate) async fn call_connector_agent_tool(
    state: Arc<AppState>,
    params: Map<String, Value>,
) -> Value {
    super::calls::call(&state, params).await
}

/// `/api/cancelConnectorAgentToolCall`: the agent stopped a call (`callId`); its card goes away.
pub(crate) fn cancel_connector_agent_tool_call(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let call_id = params
        .get("callId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(json!({ "cancelled": super::cards::cancel_call(state, call_id) }))
}

/// The workspace's rules as `/api/readConnectors` shows them.
pub(crate) fn connector_rules(state: &AppState, workspace: &str) -> Value {
    rules_view(workspace, &rules::load(&state.paths, workspace))
}

fn rules_view(workspace: &str, rules: &rules::WorkspaceRules) -> Value {
    json!({
        "workspaceId": workspace,
        "default": rules.default.unwrap_or(Rule::Ask),
        "tools": rules.tools,
    })
}

/// `/api/readConnectorRules`: what the workspace does with calls that change something.
pub(crate) fn read_connector_rules(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    Ok(rules_view(
        &workspace,
        &rules::load(&state.paths, &workspace),
    ))
}

/// `/api/setConnectorRule`: `rule` (`ask`, `allow`, `never`, or `default` to drop a tool's own
/// rule) for `tool`, or for every call when `tool` is absent or `all`.
pub(crate) fn set_connector_rule(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace = workspace_param(state, params)?;
    let tool = params
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tool| !tool.is_empty() && !tool.eq_ignore_ascii_case("all"));
    let rule_text = params
        .get("rule")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let rule = match rule_text.trim().to_ascii_lowercase().as_str() {
        "default" => None,
        other => Some(
            Rule::parse(other)
                .ok_or_else(|| failed("Pick a rule: ask, allow, never, or default.".to_string()))?,
        ),
    };
    let rules = rules::set(&state.paths, &workspace, tool, rule).map_err(failed)?;
    Ok(rules_view(&workspace, &rules))
}
