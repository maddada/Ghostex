//! The Connectors section: the apps of the session's Ghostex Connectors workspace, then every other
//! MCP server the session loaded.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User: "for connectors we have connectors in the app but we need to surface which ones are active now for this session". From the mockups (Q10) they picked "Workspace apps and MCP servers, in two groups": the connected apps and signed-in CLIs of the session's Connectors workspace (they reach Claude as the one `ghostex-connectors` MCP server, which is therefore not listed again), then the MCP servers Claude's transcript shows this session loaded, green when connected and amber while connecting or needing a sign-in.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::server::AppState;

use super::scan::{mcp_server_key, TranscriptFacts};

/// The MCP server Ghostex Connectors adds to every Claude launch (connectors/agent/launch.rs).
const CONNECTORS_SERVER: &str = "ghostex-connectors";

/// How long a workspace's app list is reused (reading it can start the workspace's Executor).
const WORKSPACE_FRESH_FOR: Duration = Duration::from_secs(60);

/// The workspace group, remembered for a minute per session. Blocking.
pub(super) fn workspace_group(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<Value> {
    static KNOWN: OnceLock<Mutex<HashMap<(String, String), (Instant, Option<Value>)>>> =
        OnceLock::new();
    let key = (project_id.to_string(), session_id.to_string());
    let known = KNOWN.get_or_init(Mutex::default);
    if let Some((_, group)) = known
        .lock()
        .ok()
        .and_then(|known| known.get(&key).cloned())
        .filter(|(at, _)| at.elapsed() < WORKSPACE_FRESH_FOR)
    {
        return group;
    }
    let group = read_workspace_group(state, project_id, session_id);
    if let Ok(mut known) = known.lock() {
        known.retain(|_, (at, _)| at.elapsed() < WORKSPACE_FRESH_FOR);
        known.insert(key, (Instant::now(), group.clone()));
    }
    group
}

/// The workspace group, or `None` while Connectors is off. Blocking.
fn read_workspace_group(state: &AppState, project_id: &str, session_id: &str) -> Option<Value> {
    if !crate::connectors::connectors_enabled(&state.paths) {
        return None;
    }
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    let workspace = crate::connectors::resolve_connector_session(state, &params)
        .ok()?
        .workspace;
    let mut read = Map::new();
    read.insert("workspaceId".into(), json!(workspace));
    let connectors = crate::connectors::read_connectors(state, &read).ok()?;
    let entry = connectors["workspaces"].as_array()?.first()?.clone();
    let mut rows = Vec::new();
    for app in entry["composio"]["apps"].as_array().into_iter().flatten() {
        if app["connected"] == true {
            rows.push(json!({
                "name": app["name"],
                "slug": app["slug"],
                "status": "connected",
            }));
        }
    }
    for app in entry["executor"]["apps"].as_array().into_iter().flatten() {
        let name = app["name"].as_str().or_else(|| app["slug"].as_str());
        if let (Some(name), true) = (name, app["connected"] == true) {
            rows.push(json!({"name": name, "slug": app["slug"], "status": "connected"}));
        }
    }
    if let Some(cli) = entry["cli"].as_object() {
        for (slug, tool) in cli {
            if tool["signedIn"] == true {
                rows.push(json!({
                    "name": tool["name"],
                    "slug": slug,
                    "detail": tool["account"],
                    "status": "connected",
                }));
            }
        }
    }
    Some(json!({
        "workspaceId": workspace,
        "name": entry["name"],
        "apps": rows,
    }))
}

/// How Claude names a server whose real name it never printed: `claude_ai_Notion` was `claude.ai
/// Notion`; anything else keeps its key.
fn display_name(key: &str, facts: &TranscriptFacts) -> String {
    if let Some(name) = facts.mcp_names.get(key) {
        return name.clone();
    }
    match key.strip_prefix("claude_ai_") {
        Some(rest) => format!("claude.ai {}", rest.replace('_', " ")),
        None => key.to_string(),
    }
}

/// `{workspace, servers}`: the workspace group and the session's other MCP servers.
pub(super) fn read_connectors(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    facts: Option<&TranscriptFacts>,
    mcp_status: Option<&Value>,
) -> Value {
    let workspace = workspace_group(state, project_id, session_id);
    let mut servers = Vec::new();
    if let Some(facts) = facts {
        let mut keys: Vec<&String> = facts
            .mcp_tools
            .keys()
            .chain(facts.mcp_pending.iter())
            .chain(facts.mcp_failed.iter())
            .collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            if key == CONNECTORS_SERVER && workspace.is_some() {
                continue;
            }
            let name = display_name(key, facts);
            // The probe's `mcp_status` is fresher than the transcript when it ran after it.
            let probed = mcp_status
                .and_then(|status| status.get(&name))
                .and_then(|server| server["status"].as_str());
            // Tools Claude announced mean the live session can use the server, whatever the
            // probe's own copy saw (claude.ai connectors stay "pending" there until first used).
            let has_tools = facts
                .mcp_tools
                .get(key)
                .is_some_and(|tools| !tools.is_empty());
            let status = match probed {
                Some("needs-auth") => "needsAuth",
                Some("failed") if !has_tools => "failed",
                _ if has_tools => "connected",
                Some("connected") => "connected",
                Some("pending") => "pending",
                _ if facts.mcp_failed.contains(key) => "failed",
                _ if facts.mcp_pending.contains(key) => "pending",
                _ => "connected",
            };
            let scope = mcp_status
                .and_then(|status| status.get(&name))
                .and_then(|server| server["scope"].as_str())
                .or_else(|| key.starts_with("claude_ai_").then_some("claudeai"));
            servers.push(json!({
                "key": key,
                "name": if key == CONNECTORS_SERVER { "Ghostex Connectors".to_string() } else { name },
                "tools": facts.mcp_tools.get(key).map_or(0, |tools| tools.len()),
                "status": status,
                "scope": scope,
            }));
        }
    }
    // Without a transcript to read, the probe's own list is all there is.
    if facts.is_none() {
        for (name, server) in mcp_status.and_then(Value::as_object).into_iter().flatten() {
            let key = mcp_server_key(name);
            if key == CONNECTORS_SERVER && workspace.is_some() {
                continue;
            }
            servers.push(json!({
                "key": key,
                "name": name,
                "tools": 0,
                "status": server["status"],
                "scope": server["scope"],
            }));
        }
    }
    json!({
        "workspace": workspace,
        "servers": servers,
    })
}
