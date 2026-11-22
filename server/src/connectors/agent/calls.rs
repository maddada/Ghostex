//! An agent's call to one of its workspace's connector tools: reads run at once; a call that
//! would change something follows the workspace's rules (`rules.rs`) and, when they say ask,
//! waits for Ghostex's card (`cards.rs`).
//!
//! CDXC:Connectors 2026-10-16 DECISION:
//! User (Q12): agents also get Executor's `execute` programs, and a step inside a program asks only when its app declares approval. So Ghostex asks before each `executor_call` that isn't read-only, but a program can change things its apps don't guard. Ghostex answers Executor's approval pauses itself after its card, and agents never get `resume`.

use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::server::AppState;

use super::super::composio::{self, ComposioDecision};
use super::super::executor::agent::{self as executor, Step};
use super::cards::{self, Answer};
use super::rules::{self, Rule};
use super::session::AgentSession;

/// How much of a call's input a card shows.
const CARD_INPUT_LIMIT: usize = 1_500;
/// Approval pauses one program may raise (Executor stops a program after 100 tool calls).
const MAX_PAUSES: usize = 100;

pub(super) fn text_result(text: impl Into<String>, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": is_error })
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

fn clipped_json(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    match text.char_indices().nth(CARD_INPUT_LIMIT) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text,
    }
}

fn card_question(provider: &str, labels: &[String], inputs: &[Value]) -> String {
    let mut question = format!(
        "Allow this agent to run {} through {provider}?",
        labels.join(", ")
    );
    for input in inputs {
        let empty = match input {
            Value::Null => true,
            Value::Object(fields) => fields.is_empty(),
            _ => false,
        };
        if !empty {
            question.push_str("\n\n");
            question.push_str(&clipped_json(input));
        }
    }
    question
}

/// What happens to a call that would change something.
enum Permission {
    Run,
    Refused(String),
}

/// The workspace's rule for the call's tools, and the card when the rule says ask.
async fn permission(
    state: &Arc<AppState>,
    session: &AgentSession,
    call_id: Option<&str>,
    provider: &str,
    keys: Vec<String>,
    labels: Vec<String>,
    inputs: Vec<Value>,
) -> Permission {
    let named = labels.join(", ");
    if session.reads_only {
        return Permission::Refused(crate::gbots::read_only_refusal(&named));
    }
    let rule = {
        let (paths, workspace, keys) =
            (state.paths.clone(), session.workspace.clone(), keys.clone());
        blocking(move || rules::decide(&paths, &workspace, &keys))
            .await
            .unwrap_or(Rule::Ask)
    };
    let answer = match rule {
        Rule::Allow => return Permission::Run,
        Rule::Never => {
            return Permission::Refused(format!(
                "This workspace's Connectors rules never let agents run {named}."
            ))
        }
        Rule::Ask => {
            cards::ask(
                state,
                &session.project_id,
                &session.session_id,
                call_id,
                card_question(provider, &labels, &inputs),
            )
            .await
        }
    };
    match answer {
        Answer::Approve => Permission::Run,
        Answer::Always => {
            let (paths, workspace) = (state.paths.clone(), session.workspace.clone());
            let _ = blocking(move || {
                for key in &keys {
                    let _ = rules::set(&paths, &workspace, Some(key), Some(Rule::Allow));
                }
            })
            .await;
            Permission::Run
        }
        Answer::Deny => Permission::Refused(format!("The user said no, so {named} didn't run.")),
        Answer::Expired => Permission::Refused(format!(
            "Nobody answered within 30 minutes, so {named} didn't run."
        )),
        Answer::Cancelled => {
            Permission::Refused(format!("The call was cancelled, so {named} didn't run."))
        }
        Answer::Restarting => Permission::Refused(format!(
            "Ghostex restarted before anyone answered, so {named} didn't run."
        )),
    }
}

async fn composio_call(
    state: &Arc<AppState>,
    session: &AgentSession,
    call_id: Option<&str>,
    tool: String,
    arguments: Value,
) -> Value {
    if composio::call_decision(&tool, &arguments) == ComposioDecision::Ask {
        let keys = composio::rule_tools(&tool, &arguments);
        let inputs = match arguments.get("tools") {
            Some(Value::Array(items)) if tool == "COMPOSIO_MULTI_EXECUTE_TOOL" => items
                .iter()
                .map(|item| item.get("arguments").cloned().unwrap_or(Value::Null))
                .collect(),
            _ => vec![arguments.clone()],
        };
        let permission = permission(
            state,
            session,
            call_id,
            "Composio",
            keys.iter().map(|key| rules::rule_key(key)).collect(),
            keys,
            inputs,
        )
        .await;
        if let Permission::Refused(text) = permission {
            return text_result(text, true);
        }
    }
    let (paths, workspace) = (state.paths.clone(), session.workspace.clone());
    match blocking(move || composio::call_tool(&paths, &workspace, &tool, &arguments)).await {
        Some(Ok(text)) => {
            let failed = text.starts_with("Error: ");
            text_result(text, failed)
        }
        Some(Err(error)) => text_result(error, true),
        None => text_result("The call stopped unexpectedly.", true),
    }
}

async fn executor_step(
    state: &Arc<AppState>,
    session: &AgentSession,
    work: impl FnOnce(&crate::paths::GxserverPaths, &str) -> Result<Step, String> + Send + 'static,
) -> Result<Step, String> {
    let (paths, workspace) = (state.paths.clone(), session.workspace.clone());
    blocking(move || work(&paths, &workspace))
        .await
        .unwrap_or_else(|| Err("The call stopped unexpectedly.".to_string()))
}

async fn executor_call(
    state: &Arc<AppState>,
    session: &AgentSession,
    call_id: Option<&str>,
    tool: String,
    arguments: Value,
) -> Value {
    // What the user already approved in this call, so Executor's own pause for it doesn't ask again.
    let mut approved: Vec<String> = Vec::new();
    match tool.as_str() {
        "call" => {
            let planned = {
                let (paths, workspace, arguments) = (
                    state.paths.clone(),
                    session.workspace.clone(),
                    arguments.clone(),
                );
                blocking(move || executor::plan_calls(&paths, &workspace, &arguments))
                    .await
                    .unwrap_or_else(|| Err("The call stopped unexpectedly.".to_string()))
            };
            let planned = match planned {
                Ok(planned) => planned,
                Err(error) => return text_result(error, true),
            };
            let writes: Vec<_> = planned.into_iter().filter(|call| !call.read_only).collect();
            if !writes.is_empty() {
                let keys: Vec<String> = writes.iter().map(|call| call.key.clone()).collect();
                let permission = permission(
                    state,
                    session,
                    call_id,
                    "Executor",
                    keys.clone(),
                    writes.iter().map(|call| call.label.clone()).collect(),
                    writes.iter().map(|call| call.input.clone()).collect(),
                )
                .await;
                match permission {
                    Permission::Run => approved = keys,
                    Permission::Refused(text) => return text_result(text, true),
                }
            }
        }
        "execute" if executor::program_uses_management(&arguments) => {
            return text_result(executor::management_refused(), true);
        }
        // A program's steps can change what its apps don't guard, so a GBot that only reads
        // runs single read calls only.
        "execute" if session.reads_only => {
            return text_result(crate::gbots::read_only_refusal("this program"), true);
        }
        _ => {}
    }
    let mut step = executor_step(state, session, move |paths, workspace| {
        executor::call(paths, workspace, &tool, &arguments)
    })
    .await;
    for _ in 0..MAX_PAUSES {
        let (request_id, key, label, input) = match step {
            Err(error) => return text_result(error, true),
            Ok(Step::Done(result)) => return result,
            Ok(Step::Approval {
                request_id,
                key,
                label,
                input,
            }) => (request_id, key, label, input),
        };
        let approve = approved.contains(&key)
            || matches!(
                permission(
                    state,
                    session,
                    call_id,
                    "Executor",
                    vec![key.clone()],
                    vec![label],
                    vec![input],
                )
                .await,
                Permission::Run
            );
        if approve {
            approved.push(key);
        }
        step = executor_step(state, session, move |paths, workspace| {
            executor::resume(paths, workspace, &request_id, approve)
        })
        .await;
    }
    text_result(
        "The program asked for approval too many times, so Ghostex stopped answering it.",
        true,
    )
}

/// Runs an agent's call and returns its MCP result.
pub(crate) async fn call(state: &Arc<AppState>, params: Map<String, Value>) -> Value {
    let tool = params
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let arguments = params
        .get("arguments")
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let call_id = params
        .get("callId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let session = {
        let state = state.clone();
        blocking(move || super::session::resolve(&state, &params)).await
    };
    let session = match session {
        Some(Ok(session)) => session,
        Some(Err(error)) => return text_result(error.message, true),
        None => return text_result("The call stopped unexpectedly.", true),
    };
    match tool.strip_prefix(executor::TOOL_PREFIX) {
        Some(name) => {
            executor_call(
                state,
                &session,
                call_id.as_deref(),
                name.to_string(),
                arguments,
            )
            .await
        }
        None => composio_call(state, &session, call_id.as_deref(), tool, arguments).await,
    }
}
