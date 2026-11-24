//! Safe one-click command running on Needs you cards and coordinator asks.
//!
//! Both `gx-chat-core` (to decide whether a row gets the "Run it" button) and `gxserver` (to verify
//! the stored command before spawning `ghostex`) call [`parse_runnable_command`], so the allowlist
//! has one source of truth.

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::ChatRpcMethod;

const MAX_COMMAND_LEN: usize = 2_000;

/// Strips a leading `! ` shortcut prefix (e.g. `! ghostex grokbot connect`) for clean display and
/// allowlist matching.
pub fn display_command(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix('!') {
        let rest = rest.trim_start();
        if !rest.is_empty() {
            return rest.to_string();
        }
    }
    trimmed.to_string()
}

/// Whether `raw` is a Ghostex command allowed to run via the Needs you "Run it" button.
pub fn can_run_command(raw: &str) -> bool {
    parse_runnable_command(raw).is_some()
}

/// Parses `raw` into the argument vector to pass directly to the `ghostex` binary (without any
/// shell). Returns `None` if `raw` contains shell metacharacters, control characters, unclosed
/// quotes, or a command outside the Ghostex setup/integration allowlist.
pub fn parse_runnable_command(raw: &str) -> Option<Vec<String>> {
    let cleaned = display_command(raw);
    if cleaned.is_empty() || cleaned.len() > MAX_COMMAND_LEN {
        return None;
    }
    if cleaned.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return None;
    }
    let words = split_safe_words(&cleaned)?;
    let (program, args) = words.split_first()?;
    if !matches!(program.as_str(), "ghostex" | "gx" | "gxserver") {
        return None;
    }
    if !is_allowlisted_ghostex_args(args) {
        return None;
    }
    Some(args.to_vec())
}

/// Quote-aware tokenizer that rejects shell control operators, pipes, redirections, and variable or
/// command substitutions so commands can never be chained or expanded.
fn split_safe_words(input: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match quote {
            Some('\'') => {
                if ch == '\'' {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            Some(_) => match ch {
                '"' => quote = None,
                '`' | '$' => return None,
                '\\' => {
                    let escaped = chars.next()?;
                    match escaped {
                        '"' | '\\' => current.push(escaped),
                        _ => {
                            current.push('\\');
                            current.push(escaped);
                        }
                    }
                }
                _ => current.push(ch),
            },
            None => match ch {
                c if c.is_ascii_whitespace() => {
                    if in_token {
                        words.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                }
                '\'' | '"' => {
                    quote = Some(ch);
                    in_token = true;
                }
                ';' | '|' | '&' | '`' | '$' | '>' | '<' | '(' | ')' | '{' | '}' => return None,
                '\\' => {
                    let escaped = chars.next()?;
                    if escaped.is_control() {
                        return None;
                    }
                    current.push(escaped);
                    in_token = true;
                }
                _ => {
                    current.push(ch);
                    in_token = true;
                }
            },
        }
    }

    if quote.is_some() {
        return None;
    }
    if in_token {
        words.push(current);
    }
    if words.is_empty() {
        None
    } else {
        Some(words)
    }
}

/// Strict allowlist of Ghostex integration, setup, repair, sign-in, and status commands that the
/// user can start from a Needs you card with one click.
fn is_allowlisted_ghostex_args(args: &[String]) -> bool {
    let first = match args.first() {
        Some(word) => word.as_str(),
        None => return false,
    };
    let second = args.get(1).map(String::as_str);
    match (first, second) {
        ("grokbot", Some("connect" | "test" | "status")) => true,
        (
            "connectors",
            Some(
                "sign-in" | "signin" | "login" | "connect" | "refresh" | "status" | "list" | "ls"
                | "gh" | "gws" | "executor",
            ),
        ) => true,
        ("cloud", Some("connect" | "providers" | "status" | "list" | "ls")) => true,
        ("account-login", Some("claude" | "codex")) => true,
        ("accounts", Some("status" | "list" | "hub")) => true,
        ("agent-cli", Some("status" | "install" | "update" | "add-to-path")) => true,
        ("agent-hooks", Some("status" | "install")) => true,
        ("tailcat", Some("status" | "enable")) => true,
        ("computer-code", _) => true,
        ("computers", None | Some("status" | "list" | "add" | "access")) => true,
        ("spaceo", Some("status" | "repair")) => true,
        ("settings", Some("open" | "get" | "set")) => true,
        ("issues", Some("status" | "source" | "tracker")) => true,
        ("team", Some("status" | "join" | "deploy" | "ping" | "flow")) => true,
        ("move-project-folder", _) => args.iter().any(|arg| arg == "--resume"),
        ("gbots", Some("status" | "list" | "day-watch")) => true,
        _ => false,
    }
}

/// Action payload for clicking "Run it" on a row.
pub fn run_action(key: &str, item: &str, ask: &str) -> Value {
    let mut action = json!({
        "type": "runNeedsYouCommand",
        "key": key,
        "item": item,
    });
    if !ask.is_empty() {
        action["ask"] = json!(ask);
    }
    action
}

/// Action payload for clicking "Stop" while a command is running.
pub fn stop_action(key: &str, item: &str, ask: &str) -> Value {
    let mut action = json!({
        "type": "stopNeedsYouCommand",
        "key": key,
        "item": item,
    });
    if !ask.is_empty() {
        action["ask"] = json!(ask);
    }
    action
}

/// Current live command run state for `key` (or `Value::Null` when idle).
pub fn current_run(state: &ChatState, key: &str) -> Value {
    let in_flight = state
        .extras
        .panels
        .command_run_requests
        .values()
        .any(|(req_key, _)| req_key == key);
    let local = state.extras.panels.command_runs.get(key);
    let server = state
        .session
        .needs_you
        .as_ref()
        .and_then(|ny| ny.get("commandRuns"))
        .and_then(|runs| runs.get(key))
        .filter(|run| run.is_object());

    if in_flight {
        return local
            .cloned()
            .or_else(|| server.cloned())
            .unwrap_or(Value::Null);
    }
    if let Some(local_run) = local {
        if local_run["localError"] == true {
            return local_run.clone();
        }
    }
    server
        .cloned()
        .or_else(|| local.cloned())
        .unwrap_or(Value::Null)
}

fn text<'a>(action: &'a UserAction, name: &str) -> &'a str {
    action
        .param(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// Handles `runNeedsYouCommand`: marks the row running optimistically and sends
/// `/api/coordinatorBoard { op: "runCommand", key, item, ask? }` to gxserver.
pub fn handle_run(state: &mut ChatState, action: &UserAction, request_id: u64) -> Vec<Effect> {
    let key = text(action, "key").trim().to_string();
    let item = text(action, "item").trim().to_string();
    let ask = text(action, "ask").trim().to_string();
    if key.is_empty() || item.is_empty() {
        return Vec::new();
    }
    let panels = &mut state.extras.panels;
    if panels
        .command_run_requests
        .values()
        .any(|(req_key, is_stop)| req_key == &key && !*is_stop)
    {
        return Vec::new();
    }
    panels.command_runs.insert(
        key.clone(),
        json!({
            "state": "running",
            "command": "",
            "hintTitle": "",
            "hint": "",
            "output": "",
            "error": "",
        }),
    );
    panels
        .command_run_requests
        .insert(request_id, (key.clone(), false));
    state.core.request_publish();

    let mut params = json!({
        "projectId": state.identity.project_id,
        "sessionId": state.identity.session_id,
        "op": "runCommand",
        "key": key,
        "item": item,
    });
    if !ask.is_empty() {
        params["ask"] = json!(ask);
    }
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::CoordinatorBoard,
        params: Box::new(params),
    }]
}

/// Handles `stopNeedsYouCommand`: asks gxserver to stop the running command for `key`.
pub fn handle_stop(state: &mut ChatState, action: &UserAction, request_id: u64) -> Vec<Effect> {
    let key = text(action, "key").trim().to_string();
    let item = text(action, "item").trim().to_string();
    let ask = text(action, "ask").trim().to_string();
    if key.is_empty() || item.is_empty() {
        return Vec::new();
    }
    state
        .extras
        .panels
        .command_run_requests
        .insert(request_id, (key.clone(), true));

    let mut params = json!({
        "projectId": state.identity.project_id,
        "sessionId": state.identity.session_id,
        "op": "stopCommand",
        "key": key,
        "item": item,
    });
    if !ask.is_empty() {
        params["ask"] = json!(ask);
    }
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::CoordinatorBoard,
        params: Box::new(params),
    }]
}

/// Settles a `runCommand` or `stopCommand` RPC response.
pub fn settle_rpc(
    state: &mut ChatState,
    request_id: u64,
    outcome: Result<&Value, String>,
) -> Option<Vec<Effect>> {
    let (key, is_stop) = state
        .extras
        .panels
        .command_run_requests
        .remove(&request_id)?;
    state.core.request_publish();
    match outcome {
        Ok(result) => {
            if let Some(run) = result.get("run").filter(|value| value.is_object()) {
                state.extras.panels.command_runs.insert(key, run.clone());
            } else if is_stop {
                state.extras.panels.command_runs.insert(
                    key,
                    json!({
                        "state": "stopped",
                        "error": "Stopped before it finished.",
                    }),
                );
            }
            Some(Vec::new())
        }
        Err(message) => {
            let clean = if message.trim().is_empty() {
                if is_stop {
                    "Could not stop the command.".to_string()
                } else {
                    "Could not start the command.".to_string()
                }
            } else {
                message
            };
            if !is_stop {
                state.extras.panels.command_runs.insert(
                    key,
                    json!({
                        "state": "failed",
                        "error": clean,
                        "localError": true,
                    }),
                );
            }
            Some(vec![Effect::Toast {
                level: "error".to_string(),
                message: clean,
            }])
        }
    }
}
