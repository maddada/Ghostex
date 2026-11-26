//! What agents get from the Executor CLI: Executor's own MCP tools (`skills`, `search`,
//! `describe`, `call`, `execute`) through the relay (`relay.rs`), in model mode, where a step that
//! needs approval pauses as `approval-required` and `resume` continues it. Ghostex answers those
//! itself after its card (crate::connectors::agent), so agents never get `resume`.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use super::paths;
use super::relay;
use super::status::{self, Snapshot};

/// What agents see in front of Executor's tool names, next to Composio's `COMPOSIO_*` ones.
pub(crate) const TOOL_PREFIX: &str = "executor_";
/// Executor ends a program after 5 minutes; a little longer so its own answer comes back.
const CALL_TIMEOUT: Duration = Duration::from_secs(6 * 60);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
/// A paused program that keeps asking for input Ghostex can't collect is given up on after this.
const MAX_INPUT_CANCELS: usize = 20;
const APPROVAL_NOTE: &str = "Ghostex asks the user itself before a step that needs approval, so a call returns only once it finished or the user declined it.";
const PROGRAM_NOTE: &str = "Ghostex runs a program only when it names each app where it uses it, as tools.<app>.<tool>(…) or tools[\"<app>\"]…, and follows the workspace's rules for each.";
const NOT_READY: &str = "Executor isn't ready: sign the Executor CLI in on Settings > Connectors.";

/// Executor's MCP tools as agents get them, for the CLI version they were read from.
static TOOLS: Mutex<Option<(String, Vec<Value>)>> = Mutex::new(None);

/// One JSON-RPC request through the CLI's relay to its signed-in host.
fn rpc(
    snapshot: &Snapshot,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let (found, whoami) = snapshot.usable().ok_or(NOT_READY)?;
    relay::request(&found.executable, &whoami.host, method, params, timeout)
}

/// Whether agents get Executor's tools: the CLI is Executor 2, signed in, in model mode. The
/// sign-in is the computer's, so every workspace gets the same answer.
pub(crate) fn enabled() -> bool {
    status::snapshot().usable().is_some()
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

/// The Executor tools agents get, and the only ones Ghostex forwards: `resume` stays Ghostex's,
/// and a tool a later Executor adds reaches agents only once Ghostex knows what it does.
pub(crate) const KNOWN_TOOLS: [&str; 5] = ["skills", "search", "describe", "call", "execute"];

fn agent_tool(tool: &Value) -> Option<Value> {
    let name = tool.get("name").and_then(Value::as_str)?;
    if !KNOWN_TOOLS.contains(&name) {
        return None;
    }
    let mut tool = tool.clone();
    let description = agent_description(
        tool.get("description")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let description = if name == "execute" {
        format!("{description} {PROGRAM_NOTE}")
    } else {
        description
    };
    tool["name"] = json!(format!("{TOOL_PREFIX}{name}"));
    tool["description"] = json!(description);
    Some(tool)
}

/// Executor's tools as agents get them: `executor_<name>`, [`KNOWN_TOOLS`] only. Read through the
/// relay once per CLI version.
pub(crate) fn agent_tools() -> Result<Vec<Value>, String> {
    let snapshot = status::snapshot();
    let (found, _) = snapshot.usable().ok_or(NOT_READY)?;
    let version = found.version.clone().unwrap_or_default();
    if let Some((read_for, tools)) = TOOLS.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        if read_for == version {
            return Ok(tools);
        }
    }
    let listed = rpc(&snapshot, "tools/list", json!({}), LIST_TIMEOUT)?;
    let tools: Vec<Value> = listed
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(agent_tool)
        .collect();
    if !tools.is_empty() {
        *TOOLS.lock().unwrap_or_else(|e| e.into_inner()) = Some((version, tools.clone()));
    }
    Ok(tools)
}

/// One call inside an agent's `executor_call`: its rule key, a label for the card and its input.
pub(crate) struct PlannedCall {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) input: Value,
}

/// How a card names a tool: `send_email in Gmail (app app_1)`, with Executor's id for the app,
/// since rules follow an app's address and two apps can have taken it in turn.
fn tool_label(snapshot: &Snapshot, slug: &str, tool: Option<&str>) -> String {
    let app = snapshot.app_by_slug(slug);
    let name = app.map_or(slug, |app| app.name.as_str());
    let named = match app.map(|app| app.id.as_str()).filter(|id| !id.is_empty()) {
        Some(id) => format!("{name} (app {id})"),
        None => name.to_string(),
    };
    match tool {
        Some(tool) => format!("{tool} in {named}"),
        None => format!("any tool in {named}"),
    }
}

/// The calls an `executor_call` would make (`calls`: one `{ path, input }` or a non-empty array of
/// them, as Executor's `CallInput` takes), each read the way Executor reads its path. Anything
/// else is refused, so a call Ghostex couldn't read never runs.
///
/// CDXC:Connectors 2026-11-27 WHY:
/// The local Executor Ghostex used to run told it which tools only read; through the CLI's MCP relay nothing says so before a call (an undeclared mutation runs without pausing, tried on 2.0.0-beta.79), so every `executor_call` counts as one that may change something and follows the workspace's rules: a card until the user picks Always allow for that tool.
pub(crate) fn plan_calls(arguments: &Value) -> Result<Vec<PlannedCall>, String> {
    let unreadable = || {
        "Ghostex can't read this call: give `calls` as one { path, input } or a list of them."
            .to_string()
    };
    let calls = match arguments.get("calls") {
        Some(Value::Array(items)) if !items.is_empty() => items.clone(),
        Some(item @ Value::Object(_)) => vec![item.clone()],
        _ => return Err(unreadable()),
    };
    let snapshot = status::snapshot();
    calls
        .iter()
        .map(|call| {
            let path = call
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(unreadable)?;
            let input = call.get("input").cloned().ok_or_else(unreadable)?;
            let (slug, tool) = paths::call_path(path)?;
            Ok(PlannedCall {
                key: format!("executor:{slug}.{tool}"),
                label: tool_label(&snapshot, &slug, Some(&tool)),
                input,
            })
        })
        .collect()
}

/// An agent's `executor_execute`: its code and what it reaches.
pub(crate) struct PlannedProgram {
    pub(crate) code: String,
    /// `executor:<app>.<tool>` for each tool it calls directly.
    pub(crate) keys: Vec<String>,
    /// Apps it may use any tool of.
    pub(crate) whole_apps: Vec<String>,
    /// Each tool or app it reaches, for the card.
    pub(crate) labels: Vec<String>,
}

/// Reads a program (`code`), or says why Ghostex won't run it: Executor's own app, or `tools`
/// reached in a way Ghostex can't check (connectors/executor/paths.rs).
pub(crate) fn plan_program(arguments: &Value) -> Result<PlannedProgram, String> {
    let code = arguments
        .get("code")
        .and_then(Value::as_str)
        .ok_or("Ghostex can't read this program: give its JavaScript as `code`.")?;
    let reach = paths::program_reach(code)?;
    let snapshot = status::snapshot();
    let mut program = PlannedProgram {
        code: code.to_string(),
        keys: Vec::new(),
        whole_apps: Vec::new(),
        labels: Vec::new(),
    };
    for (slug, tool) in &reach.apps {
        program
            .labels
            .push(tool_label(&snapshot, slug, tool.as_deref()));
        match tool {
            Some(tool) => program.keys.push(format!("executor:{slug}.{tool}")),
            None => program.whole_apps.push(slug.clone()),
        }
    }
    Ok(program)
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

fn resume_rpc(snapshot: &Snapshot, request_id: &str, response: Value) -> Result<Value, String> {
    rpc(
        snapshot,
        "tools/call",
        json!({ "name": "resume", "arguments": { "requestId": request_id, "response": response } }),
        CALL_TIMEOUT,
    )
}

/// Turns Executor's answer into a step. A tool that asks for input (a form) is cancelled, since
/// Ghostex can't collect it yet, and the program goes on without it.
fn settle(snapshot: &Snapshot, mut result: Value) -> Result<Step, String> {
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
                let app = snapshot.app(app_id);
                let slug = app.map(|app| app.slug.as_str()).unwrap_or(app_id);
                let name = app.map(|app| app.name.as_str()).unwrap_or(slug);
                return Ok(Step::Approval {
                    key: format!("executor:{slug}.{tool}"),
                    label: format!("{tool} in {name}"),
                    input: invocation.get("input").cloned().unwrap_or(Value::Null),
                    request_id,
                });
            }
            Some("input-required") if !request_id.is_empty() => {
                result = resume_rpc(snapshot, &request_id, json!({ "action": "cancel" }))?;
            }
            // A pause that ended before its answer came: Executor's answer also lists every open
            // request of the sign-in, which is not the agent's to see.
            Some("unavailable") => {
                let reason = outcome
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("ended");
                return Ok(Step::Done(json!({
                    "content": [{ "type": "text", "text": format!(
                        "Executor's approval ended before the answer reached it ({reason}), so that step didn't run. Earlier steps may already have made changes: check with a read before trying again, and don't rerun it on your own."
                    ) }],
                    "isError": true,
                })));
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
pub(crate) fn call(tool: &str, arguments: &Value) -> Result<Step, String> {
    if !KNOWN_TOOLS.contains(&tool) {
        return Err(format!(
            "Executor has no tool Ghostex gives agents named {tool}."
        ));
    }
    let snapshot = status::snapshot();
    let result = rpc(
        &snapshot,
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
        CALL_TIMEOUT,
    )?;
    settle(&snapshot, result)
}

/// Answers a paused step: approve or decline it.
pub(crate) fn resume(request_id: &str, approve: bool) -> Result<Step, String> {
    let snapshot = status::snapshot();
    let response = if approve {
        json!({ "action": "accept", "content": {} })
    } else {
        json!({ "action": "decline" })
    };
    let result = resume_rpc(&snapshot, request_id, response)?;
    settle(&snapshot, result)
}
