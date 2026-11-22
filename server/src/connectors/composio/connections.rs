//! Connecting one of the user's apps to a workspace's Composio For You (Open Dot's `isConnected`
//! and `startConnect`): `COMPOSIO_MANAGE_CONNECTIONS` lists the apps' status or returns an app's
//! sign-in link.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::account::{self, with_live};
use super::http::clip;
use super::names::pretty_name;
use super::tools::call_tool;
use super::ComposioConnectStart;

const MANAGE_CONNECTIONS: &str = "COMPOSIO_MANAGE_CONNECTIONS";
const WATCH_EVERY: Duration = Duration::from_secs(4);
const WATCH_FOR: Duration = Duration::from_secs(10 * 60);

/// `(workspace, toolkit)` pairs whose connection is being watched.
static WATCHING: LazyLock<Mutex<HashSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// `"status"\s*:\s*"ACTIVE"`, case-insensitive.
fn has_active_status(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(found) = rest.find("\"status\"") {
        let after = rest[found + "\"status\"".len()..].trim_start();
        if let Some(value) = after.strip_prefix(':') {
            if value.trim_start().starts_with("\"active\"") {
                return true;
            }
        }
        rest = &rest[found + 1..];
    }
    false
}

/// The sign-in link in the tool output: `"redirect_url": "…"` (JSON-unescaped), else the first
/// `https://` URL.
fn sign_in_link(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(found) = rest.find("\"redirect_url\"") {
        let after = rest[found + "\"redirect_url\"".len()..].trim_start();
        if let Some(value) = after.strip_prefix(':').map(str::trim_start) {
            if let Some(quoted) = value.strip_prefix('"') {
                let mut end = None;
                let mut escaped = false;
                for (index, c) in quoted.char_indices() {
                    match c {
                        '\\' if !escaped => escaped = true,
                        '"' if !escaped => {
                            end = Some(index);
                            break;
                        }
                        _ => escaped = false,
                    }
                }
                if let Some(end) = end.filter(|end| *end > 0) {
                    let raw = &quoted[..end];
                    let url = serde_json::from_str::<String>(&format!("\"{raw}\""))
                        .unwrap_or_else(|_| raw.to_string());
                    return Some(url);
                }
            }
        }
        rest = &rest[found + 1..];
    }
    let start = text.find("https://")?;
    let url = text[start..]
        .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ')' | ']'))
        .next()?;
    Some(url.to_string())
}

/// True when the app is connected in the workspace's Composio account. A failed check reads as
/// not connected.
pub(crate) fn is_connected(paths: &GxserverPaths, workspace: &str, toolkit: &str) -> bool {
    let toolkit = toolkit.trim().to_lowercase();
    if with_live(paths, workspace, |live| live.connected.contains(&toolkit)) {
        return true;
    }
    let Ok(output) = call_tool(
        paths,
        workspace,
        MANAGE_CONNECTIONS,
        &json!({ "toolkits": [{ "name": toolkit, "action": "list" }] }),
    ) else {
        return false;
    };
    let active = has_active_status(&output);
    if active {
        let added = with_live(paths, workspace, |live| {
            if live.connected.contains(&toolkit) {
                None
            } else {
                live.connected.push(toolkit.clone());
                Some(live.connected.clone())
            }
        });
        if let Some(connected) = added {
            account::remember_apps(paths, workspace, &connected);
            account::notify_changed(workspace);
        }
    }
    active
}

/// The toolkits that are connected in the workspace's Composio account, from one
/// `COMPOSIO_MANAGE_CONNECTIONS` list (`data.results.<slug>.status`), in the order asked.
pub(super) fn list_connected(
    paths: &GxserverPaths,
    workspace: &str,
    toolkits: &[String],
) -> Result<Vec<String>, String> {
    let items = toolkits
        .iter()
        .map(|toolkit| json!({ "name": toolkit, "action": "list" }))
        .collect::<Vec<_>>();
    let output = call_tool(
        paths,
        workspace,
        MANAGE_CONNECTIONS,
        &json!({ "toolkits": items }),
    )?;
    let unreadable = || {
        format!(
            "Composio's list of connected apps wasn't readable: {}",
            clip(&output, 300)
        )
    };
    let parsed: Value = serde_json::from_str(&output).map_err(|_| unreadable())?;
    let results = parsed
        .pointer("/data/results")
        .and_then(Value::as_object)
        .ok_or_else(unreadable)?;
    Ok(toolkits
        .iter()
        .filter(|toolkit| {
            results
                .get(toolkit.as_str())
                .and_then(|result| result.get("status"))
                .and_then(Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case("active"))
        })
        .cloned()
        .collect())
}

/// Creates a connect link for an app; `already_connected` when it is live.
pub(crate) fn start_connect(
    paths: &GxserverPaths,
    workspace: &str,
    toolkit: &str,
) -> Result<ComposioConnectStart, String> {
    let toolkit = toolkit.trim().to_lowercase();
    if toolkit.is_empty() {
        return Err("Name the app to connect, like gmail or slack.".to_string());
    }
    let name = pretty_name(&toolkit);
    if is_connected(paths, workspace, &toolkit) {
        return Ok(ComposioConnectStart {
            already_connected: true,
            name,
            url: None,
        });
    }
    let output = call_tool(
        paths,
        workspace,
        MANAGE_CONNECTIONS,
        &json!({ "toolkits": [{ "name": toolkit, "action": "add" }] }),
    )?;
    let url = sign_in_link(&output).ok_or_else(|| {
        format!(
            "Composio didn't return a sign-in link for {name}: {}",
            clip(&output, 300)
        )
    })?;
    watch_connection(paths, workspace, &toolkit);
    Ok(ComposioConnectStart {
        already_connected: false,
        name,
        url: Some(url),
    })
}

/// Checks every few seconds, for ten minutes, whether the app finished connecting in the browser;
/// `is_connected` announces the change once it is live.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// The connected apps listed in Composio's tool descriptions are fixed when the MCP session starts, so an app connected in the browser afterwards never shows up there; asking Composio directly is the only way to notice it.
fn watch_connection(paths: &GxserverPaths, workspace: &str, toolkit: &str) {
    let key = (workspace.to_string(), toolkit.to_string());
    if !WATCHING
        .lock()
        .map(|mut watching| watching.insert(key.clone()))
        .unwrap_or(false)
    {
        return;
    }
    let paths = paths.clone();
    std::thread::spawn(move || {
        let deadline = Instant::now() + WATCH_FOR;
        while Instant::now() < deadline && account::signed_in(&paths, &key.0) {
            std::thread::sleep(WATCH_EVERY);
            if is_connected(&paths, &key.0, &key.1) {
                break;
            }
        }
        if let Ok(mut watching) = WATCHING.lock() {
            watching.remove(&key);
        }
    });
}
