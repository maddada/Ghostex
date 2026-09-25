//! Reading a subagent out of a tool call and result pair, ported from
//! `packages/shared/session-chat-presentation/subagent.ts`.
//!
//! The native chat renders the result as the link chip beside a tool row's heading and takes the
//! target from here, so a Task row always points at the same transcript.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// What the subagent viewer is opened with.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTarget {
    pub selector: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// The default agent path a relative selector is resolved against.
pub const ROOT_AGENT_PATH: &str = "/root";

const SUBAGENT_TOOL_NAMES: &[&str] = &[
    "spawn_agent",
    "agent",
    "task",
    "send_message",
    "followup_task",
];

/// `sessionChatToolSubagent`: the subagent a tool call points at, or `None` when it points at none.
pub fn tool_subagent(
    call: Option<&Value>,
    result: Option<&Value>,
    agent_path: &str,
) -> Option<SubagentTarget> {
    let call_name = call?.get("name").and_then(Value::as_str)?;
    let tool = call_name.split(['.', ':']).next_back()?.to_lowercase();
    if tool.is_empty() || !SUBAGENT_TOOL_NAMES.contains(&tool.as_str()) {
        return None;
    }
    let input = record(call.and_then(|call| call.get("input")));
    let output_value = result.and_then(|result| result.get("output"));
    let output = record(output_value);
    if tool == "send_message" || tool == "followup_task" {
        let target = text(field(&input, "target")).or_else(|| text(field(&input, "id")))?;
        let relative = target.starts_with('/')
            || field(&input, "id").and_then(Value::as_str) == Some(target.as_str());
        return Some(SubagentTarget {
            name: target
                .rsplit('/')
                .next()
                .unwrap_or(target.as_str())
                .to_string(),
            selector: if relative {
                target.clone()
            } else {
                format!("{agent_path}/{target}")
            },
            ..SubagentTarget::default()
        });
    }
    let task = text(field(&input, "task_name"));
    let name = task
        .clone()
        .or_else(|| text(field(&input, "name")))
        .or_else(|| text(field(&input, "description")))
        .or_else(|| text(field(&output, "agent_nickname")));
    let id = text(field(&output, "agent_id"))
        .or_else(|| text(field(&output, "agentId")))
        .or_else(|| agent_id_in_text(output_value.and_then(Value::as_str).unwrap_or_default()));
    let selector = id
        .or_else(|| text(field(&output, "task_name")))
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
    if selector.is_empty() {
        return None;
    }
    Some(SubagentTarget {
        name: name.unwrap_or_else(|| selector.clone()),
        agent_type: text(field(&input, "subagent_type"))
            .or_else(|| text(field(&input, "agent_type"))),
        task: text(field(&input, "description")),
        selector,
        model: None,
        effort: None,
    })
}

/// `isSessionChatSubagentSelf`: a selector that points back at the conversation being read is not
/// a link.
pub fn is_subagent_self(selector: &str, agent_path: &str) -> bool {
    selector == ROOT_AGENT_PATH || selector == agent_path
}

/// `record`: an object, or an object parsed out of a JSON string, else `None`.
fn record(value: Option<&Value>) -> Option<Map<String, Value>> {
    match value? {
        Value::String(text) => match serde_json::from_str::<Value>(text) {
            Ok(parsed) => record(Some(&parsed)),
            Err(_) => None,
        },
        Value::Object(map) => Some(map.clone()),
        _ => None,
    }
}

/// `text`: a non-blank string, trimmed.
fn text(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    // `String.prototype.trim` strips U+FEFF and leaves U+0085; `extras::agent_tasks::js_trim`
    // does the opposite of that on U+0085.
    let trimmed = crate::transcript::jsstr::js_trim(text);
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn field<'a>(record: &'a Option<Map<String, Value>>, key: &str) -> Option<&'a Value> {
    record.as_ref()?.get(key)
}

/// `/\bagentId:\s*([a-zA-Z0-9_-]+)/`, the id a Codex result paints into its text.
fn agent_id_in_text(output: &str) -> Option<String> {
    let bytes = output.as_bytes();
    let mut from = 0;
    while let Some(found) = output[from..].find("agentId:") {
        let at = from + found;
        let word_boundary =
            at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        if word_boundary {
            let rest = &output[at + "agentId:".len()..];
            // JavaScript's `\s` does NOT include U+0085, so a `agentId:\u{85}abc` matches no id.
            let body = rest.trim_start_matches(crate::transcript::jsstr::is_js_space);
            let id: String = body
                .chars()
                .take_while(|character| {
                    character.is_ascii_alphanumeric() || *character == '_' || *character == '-'
                })
                .collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
        from = at + 1;
    }
    None
}
