use serde_json::{Map, Value};

use super::args::{parse_boolean, Flags};
use super::rpc::{CliError, CliResult};

/// CDXC:SessionChat 2026-09-12 WHY:
/// Mobile reaches the model-selection API over SSH through this verb. Without it the chat transport cannot expose Codex model or effort choices, including on drafts.
pub(super) fn parse(mut params: Map<String, Value>, flags: &Flags) -> CliResult<Value> {
    let model = flags.text("model").unwrap_or_default();
    let effort = flags.text("effort").unwrap_or_default();
    let mut options = Map::new();
    for key in ["mode", "fastMode", "permissions"] {
        if let Some(value) = flags.text(key) {
            options.insert(key.to_string(), Value::String(value));
        }
    }
    if model.trim().is_empty() && options.is_empty() {
        return Err(CliError::Other(
            "select-session-chat-model requires --model <model>, --mode <mode>, --fast-mode <on|off>, or --permissions <mode>.".to_string(),
        ));
    }
    if let Some(scope) = flags.text("scope") {
        if !matches!(scope.as_str(), "session" | "default") {
            return Err(CliError::Other(
                "select-session-chat-model --scope takes session or default.".to_string(),
            ));
        }
        params.insert("scope".to_string(), Value::String(scope));
    }
    params.insert("model".to_string(), Value::String(model));
    params.insert("effort".to_string(), Value::String(effort));
    if !options.is_empty() {
        params.insert("options".to_string(), Value::Object(options));
    }
    if let Some(value) = flags.0.get("defer") {
        params.insert("defer".to_string(), Value::Bool(parse_boolean(value)));
    }
    Ok(Value::Object(params))
}
