//! Reading a subagent out of a tool call/result pair.
//!
//! **This is family f's rule** (`packages/shared/session-chat-presentation/subagent.ts`), ported
//! here because a tool row carries its subagent chip. `PROGRESS.md` lists it under "to fold into
//! family f"; when `src/extras/subagent_target.rs` lands, this file is deleted.

use serde_json::{Map, Value};

use crate::transcript::jsstr::js_trim;
use crate::transcript::tool_fold::ToolPair;

/// CDXC:SessionChat 2026-11-28 WHY:
/// Grok Build names its spawn tool `spawn_subagent` (its label can read "Spawn subagent"), so its
/// rows had no chip. Spaces fold to underscores before the lookup.
pub(crate) const SUBAGENT_TOOL_NAMES: [&str; 6] = [
    "spawn_agent",
    "spawn_subagent",
    "agent",
    "task",
    "send_message",
    "followup_task",
];

/// The tool part of a call name (`mcp.x:Task` is `task`), lower-cased with spaces as underscores.
pub(crate) fn subagent_tool_key(name: &str) -> String {
    // `.toLowerCase()` is a FULL Unicode fold here, not an ASCII one: it is a plain method call
    // rather than a non-`u` regex, so U+212A KELVIN SIGN really does become `k` and `TAS\u{212a}`
    // really is the `task` tool.
    name.split(['.', ':'])
        .next_back()
        .unwrap_or(name)
        .to_lowercase()
        .replace(' ', "_")
}

/// CDXC:SessionChat 2026-11-28 WHY:
/// The child's id as each agent prints it in a spawn result: Claude's `agentId: <id>`, OpenCode's
/// `task_id: ses_… (for resuming…)` and Grok's `agent id: <uuid>` / `subagent_id: <uuid>`. Without
/// the last three the chip fell back to the task's description, which no server lookup knows, so an
/// OpenCode or Grok chip opened "not available" while the Subagents menu row for the same child
/// opened it fine. Claude's label comes first: a Claude child's own report can quote the others.
const CHILD_ID_LABELS: [&str; 4] = ["agentId:", "task_id:", "agent id:", "subagent_id:"];

/// The first child id any of `CHILD_ID_LABELS` names in `output`, in that order.
pub(crate) fn child_id_in_output(output: &str) -> Option<String> {
    CHILD_ID_LABELS
        .iter()
        .find_map(|label| labelled_id_in_output(output, label))
}

/// A value that is an object, or a JSON string that parses into one.
///
/// `record` in `subagent.ts:20` calls ITSELF on the parse result, so a double-encoded input (a
/// JSON string whose text is another JSON string) unwraps all the way down. Parsing once and
/// refusing anything that is not an object dropped the whole target.
fn record(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::String(text) => match serde_json::from_str::<Value>(text) {
            Ok(parsed) => record(Some(&parsed)),
            Err(_) => None,
        },
        other if other.is_object() => Some(other.clone()),
        _ => None,
    }
}

/// A non-blank string field, trimmed.
fn text(record: Option<&Value>, key: &str) -> Option<String> {
    let value = record?.get(key)?.as_str()?;
    let trimmed = js_trim(value);
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// `\b<label>\s*([a-zA-Z0-9_-]+)`. `agentId:` matches its exact case (as before); the all-lower
/// labels match any ASCII case (Grok prints both `Agent ID:` and `agent id:`).
fn labelled_id_in_output(output: &str, label: &str) -> Option<String> {
    let folded;
    let haystack = if label.bytes().any(|byte| byte.is_ascii_uppercase()) {
        output
    } else {
        // An ASCII fold keeps every byte offset, so positions found here index `output` too.
        folded = output.to_ascii_lowercase();
        folded.as_str()
    };
    let mut cursor = 0;
    while let Some(at) = haystack[cursor..].find(label) {
        let start = cursor + at;
        cursor = start + label.len();
        let before = output[..start].chars().next_back();
        if before.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_') {
            continue;
        }
        let rest = &output[cursor..];
        let space: usize = rest
            .chars()
            .take_while(|character| crate::transcript::jsstr::is_js_space(*character))
            .map(char::len_utf8)
            .sum();
        let id: String = rest[space..]
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || *character == '_' || *character == '-'
            })
            .collect();
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}

/// The subagent a tool row links to, as both renderers read it.
pub fn tool_subagent(pair: &ToolPair<'_>, agent_path: &str) -> Option<Value> {
    let tool = subagent_tool_key(pair.call_name()?);
    if tool.is_empty() || !SUBAGENT_TOOL_NAMES.contains(&tool.as_str()) {
        return None;
    }
    let input = record(pair.call_input());
    let output = record(
        pair.result_output()
            .map(|text| Value::String(text.to_string()))
            .as_ref(),
    );
    let mut target = Map::new();
    if tool == "send_message" || tool == "followup_task" {
        let id = text(input.as_ref(), "id");
        let selector_source = text(input.as_ref(), "target").or_else(|| id.clone())?;
        // `target === input?.id` compares against the RAW field, not the trimmed one: an id of
        // `" abc "` is not equal to the target `"abc"`, so the selector gets the agent path.
        let raw_id = input
            .as_ref()
            .and_then(|input| input.get("id"))
            .and_then(Value::as_str);
        let selector = if selector_source.starts_with('/') || raw_id == Some(&selector_source) {
            selector_source.clone()
        } else {
            format!("{agent_path}/{selector_source}")
        };
        target.insert(
            "name".to_string(),
            selector_source
                .split('/')
                .next_back()
                .unwrap_or(&selector_source)
                .into(),
        );
        target.insert("selector".to_string(), selector.into());
        return Some(Value::Object(target));
    }
    let task = text(input.as_ref(), "task_name");
    let name = task
        .clone()
        .or_else(|| text(input.as_ref(), "name"))
        .or_else(|| text(input.as_ref(), "description"))
        .or_else(|| text(output.as_ref(), "agent_nickname"));
    let id = text(output.as_ref(), "agent_id")
        .or_else(|| text(output.as_ref(), "agentId"))
        .or_else(|| child_id_in_output(pair.result_output().unwrap_or_default()));
    let selector = id
        .or_else(|| text(output.as_ref(), "task_name"))
        .or_else(|| {
            task.as_ref().map(|task| {
                if task.starts_with('/') {
                    task.clone()
                } else {
                    format!("{agent_path}/{task}")
                }
            })
        })
        .or_else(|| name.clone())?;
    target.insert("selector".to_string(), selector.clone().into());
    target.insert("name".to_string(), name.unwrap_or(selector).into());
    if let Some(agent_type) =
        text(input.as_ref(), "subagent_type").or_else(|| text(input.as_ref(), "agent_type"))
    {
        target.insert("agentType".to_string(), agent_type.into());
    }
    if let Some(task) = text(input.as_ref(), "description") {
        target.insert("task".to_string(), task.into());
    }
    Some(Value::Object(target))
}

/// A selector that points back at the conversation being read is not a link.
pub fn is_subagent_self(selector: &str, agent_path: &str) -> bool {
    selector == "/root" || selector == agent_path
}
