//! An agent's call to one of its workspace's connector tools: reads run at once; a call that
//! would change something follows the workspace's rules (`rules.rs`) and, when they say ask,
//! waits for Ghostex's card (`cards.rs`).
//!
//! CDXC:Connectors 2026-10-16 DECISION:
//! User (Q12): agents also get Executor's `execute` programs, and a step inside a program asks only when its app declares approval. So Ghostex asks before each `executor_call` (through the Executor CLI it can't tell which only read, connectors/executor/agent.rs), but a program can change things its apps don't guard. Ghostex answers Executor's approval pauses itself after its card, and agents never get `resume`.
//!
//! CDXC:Connectors 2026-11-27 WHY:
//! Since every `executor_call` asks, a program was the easy way round the rules: Executor pauses only for tools whose app declares approval, so Never and Ask held only there. A program now follows the workspace's rules for every app and tool it names (connectors/executor/paths.rs reads them as Executor does): refused under a Never default or a Never for one of them, a card showing the code under Ask. Inside a call, an Approve answers one pause only, and an Executor card gives up before Executor's own 15-minute pause does.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::server::AppState;

use super::super::composio::{self, ComposioDecision};
use super::super::executor::agent::{self as executor, Step};
use super::cards::{self, Answer, CARD_TIMEOUT};
use super::rules::{self, Rule};
use super::session::AgentSession;

/// How much of a call's input a card shows.
const CARD_INPUT_LIMIT: usize = 1_500;
/// How much of a program's code a card shows.
const CARD_CODE_LIMIT: usize = 3_000;
/// Approval pauses one program may raise (Executor stops a program after 100 tool calls).
const MAX_PAUSES: usize = 100;
/// An Executor card gives up before Executor's own pause expires (15 minutes), so an answer
/// never arrives after the step it answers is gone.
const EXECUTOR_CARD_TIMEOUT: Duration = Duration::from_secs(14 * 60);

pub(super) fn text_result(text: impl Into<String>, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": is_error })
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn cut_text(text: &str, limit: usize, cut: &mut bool) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_string();
    }
    *cut = true;
    let kept: String = text.chars().take(limit).collect();
    format!("{kept}… ({} more characters)", count - limit)
}

/// The value with each long text and list shortened, every field kept.
fn shortened(value: &Value, text_limit: usize, items_limit: usize, cut: &mut bool) -> Value {
    match value {
        Value::String(text) => Value::String(cut_text(text, text_limit, cut)),
        Value::Array(items) => {
            let mut kept: Vec<Value> = items
                .iter()
                .take(items_limit)
                .map(|item| shortened(item, text_limit, items_limit, cut))
                .collect();
            if items.len() > items_limit {
                *cut = true;
                kept.push(json!(format!("… {} more items", items.len() - items_limit)));
            }
            Value::Array(kept)
        }
        Value::Object(fields) => {
            let mut kept = Map::new();
            for (key, field) in fields {
                let mut name = cut_text(key, text_limit, cut);
                while kept.contains_key(&name) {
                    name.push('…');
                }
                kept.insert(name, shortened(field, text_limit, items_limit, cut));
            }
            Value::Object(kept)
        }
        other => other.clone(),
    }
}

/// A call's input as a card shows it: all of it when it fits; else every field with its long
/// values shortened, so a padded field can't push another off the card; and only when even that
/// is too long, the start, saying how much is hidden.
fn card_input(value: &Value) -> String {
    let full = pretty(value);
    if full.chars().count() <= CARD_INPUT_LIMIT {
        return full;
    }
    for (text_limit, items_limit) in [(400, 20), (160, 10), (60, 5), (24, 3)] {
        let mut cut = false;
        let text = pretty(&shortened(value, text_limit, items_limit, &mut cut));
        if text.chars().count() <= CARD_INPUT_LIMIT {
            return format!("{text}\n(Long values are shortened here; the call sends them whole.)");
        }
    }
    let hidden = full.chars().count() - CARD_INPUT_LIMIT;
    let kept: String = full.chars().take(CARD_INPUT_LIMIT).collect();
    format!("{kept}…\n(Cut: {hidden} more characters of the input aren't shown. Deny unless you expected this much.)")
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
            question.push_str(&card_input(input));
        }
    }
    question
}

/// A program's card: what it can use first, in full, then its code.
fn program_question(labels: &[String], code: &str) -> String {
    let count = code.chars().count();
    let code = if count <= CARD_CODE_LIMIT {
        code.to_string()
    } else {
        let kept: String = code.chars().take(CARD_CODE_LIMIT).collect();
        format!(
            "{kept}…\n(Cut: {} more characters of the program aren't shown.)",
            count - CARD_CODE_LIMIT
        )
    };
    format!(
        "Allow this agent to run a program through Executor? It can use {}.\n\n{code}",
        labels.join(", ")
    )
}

/// What happens to a call that would change something.
enum Permission {
    Run,
    Refused(String),
}

/// A rule's answer, with the card when it says ask; Always allow writes an Allow rule for
/// `allow_keys`.
#[allow(clippy::too_many_arguments)]
async fn settle_rule(
    state: &Arc<AppState>,
    session: &AgentSession,
    call_id: Option<&str>,
    rule: Rule,
    named: &str,
    allow_keys: Vec<String>,
    question: String,
    timeout: Duration,
) -> Permission {
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
                question,
                timeout,
            )
            .await
        }
    };
    match answer {
        Answer::Approve => Permission::Run,
        Answer::Always => {
            let (paths, workspace) = (state.paths.clone(), session.workspace.clone());
            let _ = blocking(move || {
                for key in &allow_keys {
                    let _ = rules::set(&paths, &workspace, Some(key), Some(Rule::Allow));
                }
            })
            .await;
            Permission::Run
        }
        Answer::Deny => Permission::Refused(format!("The user said no, so {named} didn't run.")),
        Answer::Expired => Permission::Refused(format!(
            "Nobody answered within {} minutes, so {named} didn't run.",
            timeout.as_secs() / 60
        )),
        Answer::Cancelled => {
            Permission::Refused(format!("The call was cancelled, so {named} didn't run."))
        }
        Answer::Restarting => Permission::Refused(format!(
            "Ghostex restarted before anyone answered, so {named} didn't run."
        )),
    }
}

/// The workspace's rule for the call's tools, and the card when the rule says ask.
#[allow(clippy::too_many_arguments)]
async fn permission(
    state: &Arc<AppState>,
    session: &AgentSession,
    call_id: Option<&str>,
    provider: &str,
    keys: Vec<String>,
    labels: Vec<String>,
    inputs: Vec<Value>,
    timeout: Duration,
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
    let question = card_question(provider, &labels, &inputs);
    settle_rule(
        state, session, call_id, rule, &named, keys, question, timeout,
    )
    .await
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
            CARD_TIMEOUT,
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
    work: impl FnOnce() -> Result<Step, String> + Send + 'static,
) -> Result<Step, String> {
    blocking(work)
        .await
        .unwrap_or_else(|| Err("The call stopped unexpectedly.".to_string()))
}

/// An `executor_execute`: refused when Ghostex can't tell what it reaches or the rules say never,
/// asked about with its code under Ask.
async fn program_permission(
    state: &Arc<AppState>,
    session: &AgentSession,
    call_id: Option<&str>,
    arguments: &Value,
) -> Permission {
    // A program's steps can change what its apps don't guard, so a GBot that only reads runs
    // single read calls only.
    if session.reads_only {
        return Permission::Refused(crate::gbots::read_only_refusal("this program"));
    }
    let planned = {
        let arguments = arguments.clone();
        blocking(move || executor::plan_program(&arguments))
            .await
            .unwrap_or_else(|| Err("The call stopped unexpectedly.".to_string()))
    };
    let program = match planned {
        Ok(program) => program,
        Err(error) => return Permission::Refused(error),
    };
    let rule = {
        let (paths, workspace) = (state.paths.clone(), session.workspace.clone());
        let (keys, apps) = (program.keys.clone(), program.whole_apps.clone());
        blocking(move || rules::decide_program(&paths, &workspace, &keys, &apps))
            .await
            .unwrap_or(Rule::Ask)
    };
    let question = program_question(&program.labels, &program.code);
    settle_rule(
        state,
        session,
        call_id,
        rule,
        "this program",
        program.keys,
        question,
        EXECUTOR_CARD_TIMEOUT,
    )
    .await
}

async fn executor_call(
    state: &Arc<AppState>,
    session: &AgentSession,
    call_id: Option<&str>,
    tool: String,
    arguments: Value,
) -> Value {
    // The calls the user already approved on this call's card, one pause each, so Executor's own
    // pause for one of them doesn't ask again.
    let mut approved: Vec<String> = Vec::new();
    match tool.as_str() {
        "call" => {
            let planned = {
                let arguments = arguments.clone();
                blocking(move || executor::plan_calls(&arguments))
                    .await
                    .unwrap_or_else(|| Err("The call stopped unexpectedly.".to_string()))
            };
            let planned = match planned {
                Ok(planned) => planned,
                Err(error) => return text_result(error, true),
            };
            // Nothing says which Executor tools only read, so every call follows the rules.
            let keys: Vec<String> = planned.iter().map(|call| call.key.clone()).collect();
            let permission = permission(
                state,
                session,
                call_id,
                "Executor",
                keys.clone(),
                planned.iter().map(|call| call.label.clone()).collect(),
                planned.iter().map(|call| call.input.clone()).collect(),
                EXECUTOR_CARD_TIMEOUT,
            )
            .await;
            match permission {
                Permission::Run => approved = keys,
                Permission::Refused(text) => return text_result(text, true),
            }
        }
        "execute" => {
            if let Permission::Refused(text) =
                program_permission(state, session, call_id, &arguments).await
            {
                return text_result(text, true);
            }
        }
        // Reads: finding tools and reading their descriptions and skills.
        "skills" | "search" | "describe" => {}
        other => {
            return text_result(
                format!("Ghostex gives agents no Executor tool named executor_{other}."),
                true,
            )
        }
    }
    let mut step = executor_step(move || executor::call(&tool, &arguments)).await;
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
        let approve = match approved.iter().position(|done| *done == key) {
            Some(index) => {
                approved.remove(index);
                true
            }
            None => matches!(
                permission(
                    state,
                    session,
                    call_id,
                    "Executor",
                    vec![key],
                    vec![label],
                    vec![input],
                    EXECUTOR_CARD_TIMEOUT,
                )
                .await,
                Permission::Run
            ),
        };
        step = executor_step(move || executor::resume(&request_id, approve)).await;
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
