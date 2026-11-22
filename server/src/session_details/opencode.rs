//! An OpenCode session's own sections, from its background service: the context window of the
//! model it runs (so Context shows a percentage), its MCP servers, and its background shells.
//!
//! CDXC:SessionDetails 2026-10-25 WHY:
//! OpenCode 2 keeps all of this in its service, not in a transcript: `/api/model` gives each model's `limit.context`, and `/api/mcp` and `/api/shell` list the MCP servers and the running shells of one project folder, which the service answers for the folder named in `x-opencode-directory` (without it, for its own folder). A shell leaves `/api/shell` when it ends, so the finished ones are the session's own `shell` calls with `background: true` whose `shellID` is no longer running; OpenCode records no end time or exit code for them. The stop button ends a shell the way the chat's interrupt does (`DELETE /api/shell/<id>`). OpenCode 2 stores only a hash of the instruction files it loaded, and its 2.1 sessions are offered no `todowrite`, so Memory files has no OpenCode source and Tasks shows only when a session does write todos.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::server::read_runtime_text;
use crate::session_chat_opencode::{safe_id, snapshot, Client};

/// How long the model list (for the context window) is reused.
const MODELS_FRESH_FOR: Duration = Duration::from_secs(5 * 60);
/// Finished background shells kept for the panel's fold.
const FINISHED_KEPT: usize = 20;
const MAX_TITLE_CHARS: usize = 160;

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}\u{2026}", text[..end].trim_end()),
        None => text.to_string(),
    }
}

/// `limit.context` of `provider`/`model` among the models OpenCode offers in `directory`.
fn context_window(client: &Client, directory: &str, provider: &str, model: &str) -> Option<u64> {
    type Cache = Mutex<HashMap<String, (Instant, Vec<Value>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let cached = cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(directory).cloned())
        .filter(|(at, _)| at.elapsed() < MODELS_FRESH_FOR)
        .map(|(_, models)| models);
    let models = match cached {
        Some(models) => models,
        None => {
            let encoded: String =
                url::form_urlencoded::byte_serialize(directory.as_bytes()).collect();
            let models = client
                .request("GET", &format!("/api/model?directory={encoded}"), None)
                .ok()?["data"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if let Ok(mut cache) = cache.lock() {
                cache.retain(|_, (at, _)| at.elapsed() < MODELS_FRESH_FOR);
                cache.insert(directory.to_string(), (Instant::now(), models.clone()));
            }
            models
        }
    };
    models
        .iter()
        .find(|row| row["providerID"] == provider && row["id"] == model)
        .and_then(|row| row.pointer("/limit/context"))
        .and_then(Value::as_u64)
        .filter(|window| *window > 0)
}

fn mcp_servers(client: &Client, directory: &str) -> Vec<Value> {
    let Ok(listed) = client.request_in("GET", "/api/mcp", directory) else {
        return Vec::new();
    };
    listed["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|server| {
            let name = server["name"].as_str()?;
            let status = match server.pointer("/status/status").and_then(Value::as_str) {
                Some("disabled") => return None,
                Some("connected") => "connected",
                Some("failed") => "failed",
                Some(status) if status.starts_with("needs") => "needsAuth",
                _ => "pending",
            };
            Some(json!({
                "key": name,
                "name": name,
                "tools": Value::Null,
                "status": status,
                "scope": Value::Null,
            }))
        })
        .collect()
}

fn iso(ms: Option<i64>) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms?)
        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// `{running, finished}` background shells of OpenCode session `id`.
fn background(client: &Client, directory: &str, id: &str, messages: &[Value]) -> Option<Value> {
    let shells = client.request_in("GET", "/api/shell", directory).ok()?;
    let running: Vec<Value> = shells["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|shell| {
            shell.pointer("/metadata/sessionID").and_then(Value::as_str) == Some(id)
                && shell["status"] == "running"
        })
        .filter_map(|shell| {
            let shell_id = shell["id"].as_str().filter(|shell_id| safe_id(shell_id))?;
            let command = shell["command"].as_str().unwrap_or_default();
            Some(json!({
                "id": shell_id,
                "kind": "shell",
                "title": cut(command.lines().next().unwrap_or(command), MAX_TITLE_CHARS),
                "command": command,
                "startedAt": iso(shell.pointer("/time/started").and_then(Value::as_i64)),
                "stoppable": true,
            }))
        })
        .collect();
    let mut finished: Vec<Value> = messages
        .iter()
        .filter(|message| message["type"] == "assistant")
        .flat_map(|message| message["content"].as_array().into_iter().flatten())
        .filter(|part| {
            part["type"] == "tool"
                && part["name"] == "shell"
                && part.pointer("/state/input/background") == Some(&Value::Bool(true))
        })
        .filter_map(|part| {
            let shell_id = part
                .pointer("/state/metadata/shellID")
                .and_then(Value::as_str)?;
            if running.iter().any(|row| row["id"] == shell_id) {
                return None;
            }
            let command = part
                .pointer("/state/input/command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Some(json!({
                "id": shell_id,
                "kind": "shell",
                "title": cut(command.lines().next().unwrap_or(command), MAX_TITLE_CHARS),
                "command": command,
                "status": "ended",
            }))
        })
        .collect();
    finished.reverse();
    finished.truncate(FINISHED_KEPT);
    (!running.is_empty() || !finished.is_empty())
        .then(|| json!({"running": running, "finished": finished}))
}

/// Fills Context's window, Connectors and Background tasks for an OpenCode session. Blocking.
pub(super) fn read_opencode_details(session: &Value, folder: Option<&str>, result: &mut Value) {
    let Some(id) = read_runtime_text(session, "agentSessionId").filter(|id| safe_id(id)) else {
        return;
    };
    let Ok(snapshot) = snapshot(&id) else {
        return;
    };
    let Ok(client) = Client::discover() else {
        return;
    };
    let Some(directory) = snapshot
        .info
        .pointer("/location/directory")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| folder.map(str::to_string))
    else {
        return;
    };
    let model = &snapshot.info["model"];
    if let (Some(provider), Some(model)) = (model["providerID"].as_str(), model["id"].as_str()) {
        if let Some(window) = context_window(&client, &directory, provider, model) {
            result["context"] = json!({"live": {"windowSize": window}});
        }
    }
    let servers = mcp_servers(&client, &directory);
    if !servers.is_empty() {
        result["connectors"] = json!({"workspace": Value::Null, "servers": servers});
    }
    if let Some(background) = background(&client, &directory, &id, &snapshot.messages) {
        result["background"] = background;
    }
}

/// Ends OpenCode background shell `shell` of session `id`, after checking it is that session's.
pub(super) fn stop_opencode_shell(
    session: &Value,
    folder: Option<&str>,
    shell: &str,
) -> Result<(), String> {
    let id = read_runtime_text(session, "agentSessionId")
        .filter(|id| safe_id(id))
        .ok_or_else(|| "This OpenCode session has no conversation yet.".to_string())?;
    let client = Client::discover().map_err(|error| error.message)?;
    let directory = snapshot(&id)
        .ok()
        .and_then(|snapshot| {
            snapshot
                .info
                .pointer("/location/directory")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| folder.map(str::to_string))
        .ok_or_else(|| "OpenCode didn't say where this session runs.".to_string())?;
    let shells = client
        .request_in("GET", "/api/shell", &directory)
        .map_err(|error| error.message)?;
    let owned = shells["data"].as_array().into_iter().flatten().any(|row| {
        row["id"] == shell
            && row.pointer("/metadata/sessionID").and_then(Value::as_str) == Some(id.as_str())
    });
    if !owned {
        return Err("That background command has already ended.".to_string());
    }
    client
        .request_in("DELETE", &format!("/api/shell/{shell}"), &directory)
        .map_err(|error| error.message)?;
    crate::session_chat_opencode::invalidate(&id);
    Ok(())
}
