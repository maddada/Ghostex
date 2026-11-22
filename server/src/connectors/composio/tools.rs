//! A workspace's Composio tools: the tool list, the connected apps it reveals, and calling a tool
//! (Open Dot's `refresh`, `apps`, `mcpTools`, `callTool`).

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::account::{self, with_live, RpcFailure};
use super::connections::list_connected;
use super::http::clip;
use super::names::{logo_url, pretty_name};
use super::SUGGESTED_APPS;

/// Tools agents never see (Open Dot's `HIDDEN_TOOLS`): Composio's sandbox shell and workbench,
/// its skill and feedback tools, and the blocking wait the connect card replaces.
const HIDDEN_TOOLS: &[&str] = &[
    "REMOTE_BASH",
    "REMOTE_WORKBENCH",
    "SKILL",
    "SUBMIT_FEEDBACK",
    "WAIT_FOR_CONNECTIONS",
];
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const CALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const OUTPUT_LIMIT: usize = 30_000;
/// How long a failed `tools/list` waits before a read tries again on its own.
const LIST_RETRY_AFTER: Duration = Duration::from_secs(60);

fn hidden(name: &str) -> bool {
    HIDDEN_TOOLS.iter().any(|hidden| name.contains(hidden))
}

/// The apps whose connection a refresh asks about: the suggestions, the ones remembered and the
/// ones this process saw connected.
fn candidate_apps(paths: &GxserverPaths, workspace: &str) -> Vec<String> {
    let mut candidates: Vec<String> = SUGGESTED_APPS.iter().map(|slug| slug.to_string()).collect();
    let known = account::remembered_apps(paths, workspace)
        .into_iter()
        .chain(with_live(paths, workspace, |live| live.connected.clone()));
    for slug in known {
        if !candidates.contains(&slug) {
            candidates.push(slug);
        }
    }
    candidates
}

/// The connected app slugs: what this process last heard from Composio, plus the ones remembered
/// from before when it hasn't asked yet.
fn connected_apps(paths: &GxserverPaths, workspace: &str) -> Vec<String> {
    let (asked, mut connected) = with_live(paths, workspace, |live| {
        (live.listed, live.connected.clone())
    });
    if !asked {
        for slug in account::remembered_apps(paths, workspace) {
            if !connected.contains(&slug) {
                connected.push(slug);
            }
        }
    }
    connected
}

/// Re-lists Composio's tools and asks which apps are connected.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// Open Dot read the connected apps from `COMPOSIO_SEARCH_TOOLS`' description ("connected the apps: …"); Composio no longer writes that, so the apps come from a `COMPOSIO_MANAGE_CONNECTIONS` list instead, which only answers for apps it is asked about.
pub(crate) fn refresh(paths: &GxserverPaths, workspace: &str) -> Result<(), String> {
    with_live(paths, workspace, |live| {
        live.list_attempted_at = Some(Instant::now())
    });
    let mut tools: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        let params = match &cursor {
            Some(cursor) => json!({ "cursor": cursor }),
            None => json!({}),
        };
        let result = account::rpc(paths, workspace, "tools/list", params, LIST_TIMEOUT)
            .map_err(|failure| failure.message().to_string())?;
        if let Some(page) = result.get("tools").and_then(Value::as_array) {
            tools.extend(page.iter().cloned());
        }
        cursor = result
            .get("nextCursor")
            .and_then(Value::as_str)
            .filter(|cursor| !cursor.is_empty())
            .map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    let connected = list_connected(paths, workspace, &candidate_apps(paths, workspace)).ok();
    let visible = tools
        .into_iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| !hidden(name))
        })
        .collect::<Vec<_>>();
    let remembered = account::remembered_apps(paths, workspace);
    with_live(paths, workspace, |live| {
        live.tools = visible;
        live.listed = true;
        match &connected {
            Some(connected) => live.connected = connected.clone(),
            None => {
                for slug in remembered {
                    if !live.connected.contains(&slug) {
                        live.connected.push(slug);
                    }
                }
            }
        }
    });
    if let Some(connected) = connected {
        account::remember_apps(paths, workspace, &connected);
    }
    account::notify_changed(workspace);
    Ok(())
}

/// True when a read may list the tools on its own: signed in, never listed, and no failed try in
/// the last minute.
fn list_due(paths: &GxserverPaths, workspace: &str) -> bool {
    account::signed_in(paths, workspace)
        && with_live(paths, workspace, |live| {
            !live.listed
                && live
                    .list_attempted_at
                    .is_none_or(|at| at.elapsed() >= LIST_RETRY_AFTER)
        })
}

/// The workspace's Composio tools while signed in: `{ name, description, inputSchema }`. Lists
/// them first when this process hasn't yet.
pub(crate) fn mcp_tools(paths: &GxserverPaths, workspace: &str) -> Vec<Value> {
    if !account::signed_in(paths, workspace) {
        return Vec::new();
    }
    if list_due(paths, workspace) {
        let _ = refresh(paths, workspace);
    }
    with_live(paths, workspace, |live| {
        live.tools
            .iter()
            .filter_map(|tool| {
                let name = tool.get("name").and_then(Value::as_str)?;
                let description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(name);
                let input_schema = tool
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object" }));
                Some(json!({
                    "name": name,
                    "description": description,
                    "inputSchema": input_schema,
                }))
            })
            .collect()
    })
}

/// The workspace's app list for Settings: connected apps first, then the suggestions.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// Settings reads this whenever it opens or hears a change, so it never waits on Composio: the first read after a restart lists the tools on a background thread and the change event refetches.
pub(crate) fn apps(paths: &GxserverPaths, workspace: &str) -> Vec<Value> {
    if list_due(paths, workspace) {
        let start = with_live(paths, workspace, |live| {
            !std::mem::replace(&mut live.background_refresh_running, true)
        });
        if start {
            let paths = paths.clone();
            let workspace = workspace.to_string();
            std::thread::spawn(move || {
                let _ = refresh(&paths, &workspace);
                with_live(&paths, &workspace, |live| {
                    live.background_refresh_running = false
                });
            });
        }
    }
    let connected = connected_apps(paths, workspace);
    connected
        .iter()
        .map(String::as_str)
        .chain(
            SUGGESTED_APPS
                .iter()
                .copied()
                .filter(|slug| !connected.iter().any(|connected| connected == slug)),
        )
        .map(|slug| {
            json!({
                "slug": slug,
                "name": pretty_name(slug),
                "logo": logo_url(slug),
                "connected": connected.iter().any(|connected| connected == slug),
            })
        })
        .collect()
}

/// `text` parts as text, anything else as `[type]`; "(no output)" when empty; clipped; "Error: "
/// first when the tool failed.
fn tool_result_text(result: &Value) -> String {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .map(|part| {
                    let kind = part.get("type").and_then(Value::as_str).unwrap_or_default();
                    if kind == "text" {
                        part.get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    } else {
                        format!("[{kind}]")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let text = if text.is_empty() {
        "(no output)".to_string()
    } else {
        text
    };
    let is_error = result.get("isError").and_then(Value::as_bool) == Some(true);
    format!(
        "{}{}",
        if is_error { "Error: " } else { "" },
        clip(&text, OUTPUT_LIMIT)
    )
}

/// Calls one of Composio's tools (5 minute limit).
///
/// CDXC:Connectors 2026-10-15 WHY:
/// Open Dot retried any failed call once after reconnecting; here the one retry happens only when the call itself could not connect (DNS, connect or TLS handshake failed), so a send, post or delete whose connection broke after the request went out is never run twice.
pub(crate) fn call_tool(
    paths: &GxserverPaths,
    workspace: &str,
    name: &str,
    arguments: &Value,
) -> Result<String, String> {
    let run = || {
        account::rpc(
            paths,
            workspace,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
            CALL_TIMEOUT,
        )
        .map(|result| tool_result_text(&result))
    };
    match run() {
        Ok(text) => Ok(text),
        Err(RpcFailure::Unreached(_)) => {
            account::drop_session(paths, workspace);
            run().map_err(|failure| failure.message().to_string())
        }
        Err(failure) => Err(failure.message().to_string()),
    }
}
