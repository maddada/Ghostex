use super::{error, invalidate, safe_id, snapshot, Client, Snapshot};
use crate::{domain::DomainStateError, session_chat::*};
use serde_json::{json, Map, Value};

/// The form fields the chat asks. A `hidden` field is left out (OpenCode answers it with its
/// default unless one is supplied); a field with a `when` condition is asked like the others, says
/// when it applies, and is sent only when its condition holds for the earlier answers. An
/// `external` field (a sign-in step in the browser) still sends the whole form to the terminal.
fn asked_fields(form: &Value) -> Option<Vec<&Value>> {
    let fields = form["fields"].as_array()?;
    if fields.iter().any(|field| field["type"] == "external") {
        return None;
    }
    Some(fields.iter().filter(|field| field["hidden"] != true).collect())
}

/// Whether a `when` field applies given the answers so far (`{key, op: eq|neq, value}`); a
/// multiselect answer matches when it holds the value.
fn condition_holds(field: &Value, answers: &Map<String, Value>) -> bool {
    let Some(when) = field.get("when").filter(|when| when.is_object()) else {
        return true;
    };
    let Some(answer) = when["key"].as_str().and_then(|key| answers.get(key)) else {
        return false;
    };
    let equal = match answer {
        Value::Array(values) => values.contains(&when["value"]),
        other => other == &when["value"],
    };
    if when["op"] == "neq" {
        !equal
    } else {
        equal
    }
}

fn condition_label(field: &Value, fields: &[&Value]) -> Option<String> {
    let when = field.get("when").filter(|when| when.is_object())?;
    let key = when["key"].as_str()?;
    let name = fields
        .iter()
        .find(|other| other["key"] == key)
        .and_then(|other| other["title"].as_str())
        .unwrap_or(key);
    let value = when["value"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| when["value"].to_string());
    let verb = if when["op"] == "neq" { "is not" } else { "is" };
    Some(format!("only when \u{201c}{name}\u{201d} {verb} {value}"))
}

pub(crate) fn interactive_prompt(snapshot: &Snapshot) -> Option<SessionChatInteractivePrompt> {
    if let Some(permission) = snapshot.permissions.first() {
        return Some(SessionChatInteractivePrompt::Approval {
            tool: permission["action"]
                .as_str()
                .unwrap_or("OpenCode permission")
                .into(),
            summary: Some(
                permission["resources"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default(),
            ),
            tool_use_id: permission["id"].as_str().map(str::to_string),
        });
    }
    let form = snapshot.forms.first()?;
    let fields = asked_fields(form)?;
    let mut questions = Vec::new();
    for field in fields.iter().copied() {
        let mut options = field["options"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|option| SessionChatQuestionOption {
                label: option["label"].as_str().unwrap_or("").into(),
                description: option["description"].as_str().map(str::to_string),
                preview: None,
            })
            .collect::<Vec<_>>();
        if field["type"] == "boolean" {
            options = vec![
                SessionChatQuestionOption {
                    label: "Yes".into(),
                    description: None,
                    preview: None,
                },
                SessionChatQuestionOption {
                    label: "No".into(),
                    description: None,
                    preview: None,
                },
            ];
        }
        let question = field["title"]
            .as_str()
            .or(field["description"].as_str())
            .or(field["key"].as_str())
            .unwrap_or("");
        questions.push(SessionChatQuestion {
            question: match condition_label(field, &fields) {
                Some(condition) => format!("{question} ({condition})"),
                None => question.into(),
            },
            header: Some(form["title"].as_str().unwrap_or("OpenCode question").into()),
            multi_select: field["type"] == "multiselect",
            allow_custom: Some(options.is_empty() || field["custom"] == true),
            tool_name: Some("opencode_form".into()),
            recommended: None,
            preview_layout: false,
            options,
        });
    }
    Some(SessionChatInteractivePrompt::Question {
        questions,
        tool_use_id: form["id"].as_str().map(str::to_string),
    })
}

pub(crate) fn answer(id: &str, params: &Map<String, Value>) -> Result<(), DomainStateError> {
    invalidate(id);
    let current = snapshot(id)?;
    let prompt = interactive_prompt(&current)
        .ok_or_else(|| error("OpenCode has no supported pending prompt. Check its terminal."))?;
    if let Some(expected) = params.get("toolUseId").and_then(Value::as_str) {
        if prompt.tool_use_id() != Some(expected) {
            return Err(error(
                "The OpenCode question has changed. Read it again before answering.",
            ));
        }
    }
    let request_id = prompt
        .tool_use_id()
        .filter(|id| safe_id(id))
        .ok_or_else(|| error("Invalid OpenCode prompt ID."))?
        .to_string();
    let client = Client::discover()?;
    match prompt {
        SessionChatInteractivePrompt::Approval { .. } => {
            if params.get("kind").and_then(Value::as_str) != Some("approval") {
                return Err(error("OpenCode is waiting for a permission decision."));
            }
            let decision = match params
                .get("approvalSend")
                .and_then(Value::as_str)
                .unwrap_or("")
            {
                "1" | "y" | "y\r" | "once" => "once",
                "2" | "always" => "always",
                "" | "n" | "n\r" | "\u{1b}" | "reject" => "reject",
                _ => return Err(error("Unknown OpenCode permission decision.")),
            };
            client.session(
                id,
                &format!("/permission/{request_id}/reply"),
                "POST",
                Some(json!({"decision":decision})),
            )?;
        }
        SessionChatInteractivePrompt::Question { .. } => {
            if params.get("kind").and_then(Value::as_str) != Some("question") {
                return Err(error("OpenCode is waiting for a question answer."));
            }
            let selections: Vec<SessionChatQuestionSelection> =
                serde_json::from_value(params.get("selections").cloned().unwrap_or(Value::Null))
                    .map_err(|_| error("Invalid question selections."))?;
            let fields = asked_fields(&current.forms[0])
                .ok_or_else(|| error("Invalid OpenCode form."))?;
            if fields.len() != selections.len() {
                return Err(error("Answer every OpenCode question."));
            }
            let mut answers = Map::new();
            for (field, selection) in fields.iter().copied().zip(&selections) {
                let key = field["key"]
                    .as_str()
                    .ok_or_else(|| error("Invalid OpenCode question key."))?;
                // A conditional field the earlier answers rule out is not part of the reply.
                if !condition_holds(field, &answers) {
                    continue;
                }
                let mut values = Vec::new();
                for index in selection
                    .indices
                    .iter()
                    .filter(|_| field["type"] != "boolean")
                {
                    values.push(
                        field["options"]
                            .get(*index)
                            .and_then(|o| o["value"].as_str())
                            .ok_or_else(|| error("A selected option no longer exists."))?
                            .to_string(),
                    );
                }
                if let Some(other) = selection
                    .other
                    .as_ref()
                    .filter(|text| !text.trim().is_empty())
                {
                    values.push(other.clone());
                }
                let value = if field["type"] == "boolean" {
                    match selection.indices.as_slice() {
                        [0] => json!(true),
                        [1] => json!(false),
                        _ => return Err(error("Choose Yes or No.")),
                    }
                } else if field["type"] == "multiselect" {
                    json!(values)
                } else {
                    if values.len() != 1 {
                        return Err(error("Provide one answer for each question."));
                    }
                    if matches!(field["type"].as_str(), Some("number" | "integer")) {
                        serde_json::from_str::<Value>(&values[0])
                            .ok()
                            .filter(|v| v.is_number())
                            .ok_or_else(|| error("This question needs a number."))?
                    } else {
                        json!(values[0])
                    }
                };
                answers.insert(key.into(), value);
            }
            client.session(
                id,
                &format!("/form/{request_id}/reply"),
                "POST",
                Some(json!({"answer":answers})),
            )?;
        }
    }
    invalidate(id);
    Ok(())
}

pub(crate) fn interrupt(id: &str, prompt_id: Option<&str>) -> Result<(), DomainStateError> {
    let client = Client::discover()?;
    if let Some(prompt_id) = prompt_id {
        invalidate(id);
        let current = snapshot(id)?;
        if !safe_id(prompt_id) || !current.forms.iter().any(|form| form["id"] == prompt_id) {
            return Err(error(
                "The OpenCode question has changed. Read it again before cancelling.",
            ));
        }
        client.session(id, &format!("/form/{prompt_id}"), "DELETE", None)?;
    } else {
        client.session(id, "/interrupt", "POST", None)?;
        // A shell belongs to the session's project folder, and the service ends it only when the
        // delete names that folder (client.rs `request_in`); without it the delete answered 204 and
        // the shell kept running.
        let directory = client.session(id, "", "GET", None)?["data"]["location"]["directory"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| error("OpenCode didn't say where this session runs."))?;
        for message in client.messages(id)? {
            if message["type"] == "shell" && message["status"] == "running" {
                if let Some(shell) = message["shellID"].as_str().filter(|id| safe_id(id)) {
                    client.request_in("DELETE", &format!("/api/shell/{shell}"), &directory)?;
                }
            }
        }
    }
    invalidate(id);
    Ok(())
}
