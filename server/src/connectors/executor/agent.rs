//! What a workspace's agents get from its Executor: Executor's own MCP tools (`skills`, `search`,
//! `describe`, `call`, `execute`) through its `/mcp` endpoint in model mode, where a step that
//! needs approval pauses as `approval-required` and `resume` continues it. Ghostex answers those
//! itself after its card (crate::connectors::agent), so agents never get `resume`.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::super::composio::mcp_client::{self, McpError};
use super::apps;
use super::client::Endpoint;
use super::live::{lock, with_live};
use super::process;
use super::release;
use super::tools as app_tools;

const SERVER: &str = "Executor";
const CLIENT_NAME: &str = "Ghostex";
/// What agents see in front of Executor's tool names, next to Composio's `COMPOSIO_*` ones.
pub(crate) const TOOL_PREFIX: &str = "executor_";
/// Executor ends a program after 5 minutes; a little longer so its own answer comes back.
const CALL_TIMEOUT: Duration = Duration::from_secs(6 * 60);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
/// A paused program that keeps asking for input Ghostex can't collect is given up on after this.
const MAX_INPUT_CANCELS: usize = 20;
const APPROVAL_NOTE: &str = "Ghostex asks the user itself before a step that needs approval, so a call returns only once it finished or the user declined it.";
const MANAGEMENT_REFUSED: &str = "Agents can't use Executor's own app (its apps, accounts and settings). The user manages Executor in Ghostex.";

/// Executor's MCP tools as agents get them, the same for every workspace on one Executor version.
static TOOLS: Mutex<Option<Vec<Value>>> = Mutex::new(None);

fn mcp_url(endpoint: &Endpoint) -> String {
    format!(
        "http://127.0.0.1:{}/mcp?elicitation_mode=model",
        endpoint.port
    )
}

/// One JSON-RPC request to the workspace's Executor MCP server, starting Executor first when
/// needed; a session Executor dropped (after it restarted) is opened again once.
fn rpc(
    paths: &GxserverPaths,
    workspace: &str,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let endpoint = process::ensure_running(paths, workspace)?;
    let url = mcp_url(&endpoint);
    for _ in 0..2 {
        let session = match with_live(paths, workspace, |live| live.mcp_session.clone()) {
            Some(session) => session,
            None => {
                let session =
                    mcp_client::initialize(SERVER, &url, Some(&endpoint.api_key), CLIENT_NAME)
                        .map_err(|error| error.message())?;
                with_live(paths, workspace, |live| {
                    live.mcp_session = Some(session.clone())
                });
                session
            }
        };
        match mcp_client::request(
            SERVER,
            &url,
            Some(&endpoint.api_key),
            &session,
            method,
            params.clone(),
            timeout,
        ) {
            Ok(result) => return Ok(result),
            Err(McpError::SessionGone) => {
                with_live(paths, workspace, |live| live.mcp_session = None);
            }
            Err(error) => {
                if matches!(error, McpError::Unreached(_)) {
                    with_live(paths, workspace, |live| live.mcp_session = None);
                }
                return Err(error.message());
            }
        }
    }
    Err("Executor ended the session twice in a row.".to_string())
}

/// Whether the workspace's agents get Executor's tools: it is on and installed.
pub(crate) fn enabled(paths: &GxserverPaths, workspace: &str) -> bool {
    release::installed()
        && super::settings::load(paths, workspace)
            .ok()
            .flatten()
            .is_some_and(|settings| settings.enabled)
}

fn tools_cache(paths: &GxserverPaths) -> PathBuf {
    paths
        .root_dir
        .join("connectors")
        .join(format!("executor-mcp-tools-{}.json", release::VERSION))
}

/// A description without what only Executor's own approval flow and feedback need.
fn agent_description(text: &str) -> String {
    let kept: String = text
        .split_inclusive(". ")
        .filter(|sentence| {
            let lower = sentence.to_ascii_lowercase();
            !lower.contains("resume") && !lower.contains("feedback")
        })
        .collect();
    format!("{} {APPROVAL_NOTE}", kept.trim())
}

fn agent_tool(tool: &Value) -> Option<Value> {
    let name = tool.get("name").and_then(Value::as_str)?;
    if name == "resume" {
        return None;
    }
    let mut tool = tool.clone();
    let description = agent_description(
        tool.get("description")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    tool["name"] = json!(format!("{TOOL_PREFIX}{name}"));
    tool["description"] = json!(description);
    Some(tool)
}

/// Executor's tools as agents get them: `executor_<name>`, `resume` left out. Read from
/// Executor once per Executor version (starting it if it isn't running), then kept on disk, so
/// a new session doesn't wake an idle Executor.
pub(crate) fn agent_tools(paths: &GxserverPaths, workspace: &str) -> Result<Vec<Value>, String> {
    if let Some(tools) = lock(&TOOLS).clone() {
        return Ok(tools);
    }
    let cached = fs::read_to_string(tools_cache(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
        .filter(|tools| !tools.is_empty());
    let tools = match cached {
        Some(tools) => tools,
        None => {
            let listed = rpc(paths, workspace, "tools/list", json!({}), LIST_TIMEOUT)?;
            let tools: Vec<Value> = listed
                .get("tools")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(agent_tool)
                .collect();
            if !tools.is_empty() {
                let cache = tools_cache(paths);
                if let Some(parent) = cache.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&cache, serde_json::to_vec(&tools).unwrap_or_default());
            }
            tools
        }
    };
    *lock(&TOOLS) = Some(tools.clone());
    Ok(tools)
}

/// One call inside an agent's `executor_call`: its rule key, a label for the card, whether it
/// only reads, and its input.
pub(crate) struct PlannedCall {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) read_only: bool,
    pub(crate) input: Value,
}

fn is_management_path(path: &str) -> bool {
    path == "tools.executor" || path.starts_with("tools.executor.")
}

/// The calls an `executor_call` would make (`calls`: one `{ path, input }` or an array), each
/// with whether Executor marks its tool read-only. A path Ghostex can't place counts as one that
/// may change something.
pub(crate) fn plan_calls(
    paths: &GxserverPaths,
    workspace: &str,
    arguments: &Value,
) -> Result<Vec<PlannedCall>, String> {
    let calls = match arguments.get("calls") {
        Some(Value::Array(items)) => items.clone(),
        Some(item @ Value::Object(_)) => vec![item.clone()],
        _ => Vec::new(),
    };
    let endpoint = process::ensure_running(paths, workspace)?;
    let all = apps::all(&endpoint)?;
    calls
        .iter()
        .map(|call| {
            let path = call
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if is_management_path(path) {
                return Err(MANAGEMENT_REFUSED.to_string());
            }
            let address = path.strip_prefix("tools.").unwrap_or(path);
            let (slug, tool) = address.split_once('.').unwrap_or((address, ""));
            let app = all.iter().find(|app| apps::app_slug(app) == slug);
            let read_only = match app {
                Some(app) if !tool.is_empty() => {
                    app_tools::is_read_only(&endpoint, app, tool)?.unwrap_or(false)
                }
                _ => false,
            };
            let input = call.get("input").cloned().unwrap_or(Value::Null);
            // `{ accountId, input }` picks a signed-in account; the card shows what the tool gets.
            let input = match &input {
                Value::Object(fields) if fields.len() == 2 && fields.contains_key("accountId") => {
                    fields.get("input").cloned().unwrap_or(Value::Null)
                }
                _ => input,
            };
            Ok(PlannedCall {
                key: format!("executor:{slug}.{tool}"),
                label: format!("{tool} in {}", app.map(apps::app_name).unwrap_or(slug)),
                read_only,
                input,
            })
        })
        .collect()
}

/// Whether a program names Executor's own app, which agents may not use.
pub(crate) fn program_uses_management(arguments: &Value) -> bool {
    let code = arguments
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    ["tools.executor", "tools[\"executor\"]", "tools['executor']"]
        .iter()
        .any(|needle| code.contains(needle))
}

pub(crate) fn management_refused() -> &'static str {
    MANAGEMENT_REFUSED
}

/// Where an agent's Executor call stands.
pub(crate) enum Step {
    /// The MCP result to hand the agent.
    Done(Value),
    /// A step paused for the user's approval.
    Approval {
        request_id: String,
        /// `executor:<app>.<tool>`, for the workspace's rules.
        key: String,
        label: String,
        input: Value,
    },
}

/// A tool result's own outcome: `structuredContent`, else its first text block read as JSON.
fn outcome_of(result: &Value) -> Value {
    if let Some(structured) = result.get("structuredContent").filter(|v| v.is_object()) {
        return structured.clone();
    }
    result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|block| block.get("text"))
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or(Value::Null)
}

fn resume_rpc(
    paths: &GxserverPaths,
    workspace: &str,
    request_id: &str,
    response: Value,
) -> Result<Value, String> {
    rpc(
        paths,
        workspace,
        "tools/call",
        json!({ "name": "resume", "arguments": { "requestId": request_id, "response": response } }),
        CALL_TIMEOUT,
    )
}

/// Turns Executor's answer into a step. A tool that asks for input (a form) is cancelled, since
/// Ghostex can't collect it yet, and the program goes on without it.
fn settle(paths: &GxserverPaths, workspace: &str, mut result: Value) -> Result<Step, String> {
    for _ in 0..MAX_INPUT_CANCELS {
        let outcome = outcome_of(&result);
        let request_id = outcome
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        match outcome.get("status").and_then(Value::as_str) {
            Some("approval-required") if !request_id.is_empty() => {
                let invocation = outcome.get("invocation").cloned().unwrap_or(Value::Null);
                let app_id = invocation
                    .get("app")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let tool = invocation
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let endpoint = process::ensure_running(paths, workspace)?;
                let app = apps::all(&endpoint)?
                    .into_iter()
                    .find(|app| apps::app_id(app) == app_id);
                let slug = app.as_ref().map(apps::app_slug).unwrap_or(app_id);
                let name = app.as_ref().map(apps::app_name).unwrap_or(slug);
                return Ok(Step::Approval {
                    key: format!("executor:{slug}.{tool}"),
                    label: format!("{tool} in {name}"),
                    input: invocation.get("input").cloned().unwrap_or(Value::Null),
                    request_id,
                });
            }
            Some("input-required") if !request_id.is_empty() => {
                result = resume_rpc(paths, workspace, &request_id, json!({ "action": "cancel" }))?;
            }
            _ => return Ok(Step::Done(result)),
        }
    }
    Err(
        "Executor kept asking for input Ghostex can't collect, so the call was stopped."
            .to_string(),
    )
}

/// Calls one of Executor's MCP tools for an agent (`tool` without the `executor_` prefix).
pub(crate) fn call(
    paths: &GxserverPaths,
    workspace: &str,
    tool: &str,
    arguments: &Value,
) -> Result<Step, String> {
    let result = rpc(
        paths,
        workspace,
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
        CALL_TIMEOUT,
    )?;
    settle(paths, workspace, result)
}

/// Answers a paused step: approve or decline it.
pub(crate) fn resume(
    paths: &GxserverPaths,
    workspace: &str,
    request_id: &str,
    approve: bool,
) -> Result<Step, String> {
    let response = if approve {
        json!({ "action": "accept", "content": {} })
    } else {
        json!({ "action": "decline" })
    };
    let result = resume_rpc(paths, workspace, request_id, response)?;
    settle(paths, workspace, result)
}
