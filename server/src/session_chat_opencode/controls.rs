use super::{Client, error, invalidate, snapshot};
use crate::domain::DomainStateError;
use serde_json::{Map, Value, json};

pub(crate) fn select(id: &str, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let client = Client::discover()?;
    let model = params.get("model").and_then(Value::as_str).unwrap_or("");
    let effort = params.get("effort").and_then(Value::as_str).unwrap_or("");
    let options = crate::session_chat_model_selection::read_options("opencode", params)?;
    let scope = crate::session_chat_model_selection::read_scope("opencode", params)?;
    if !model.is_empty() {
        let (provider, name) = model
            .split_once('/')
            .filter(|(p, n)| !p.is_empty() && !n.is_empty())
            .ok_or_else(|| error("An OpenCode model must include its provider."))?;
        let mut reference = json!({"providerID":provider,"id":name});
        if !effort.is_empty() {
            reference["variant"] = json!(effort);
        }
        client.session(id, "/model", "POST", Some(json!({"model":reference})))?;
        if scope == "default" {
            let mut saved = json!({"providerID":provider,"model":name});
            if !effort.is_empty() {
                saved["variant"] = json!(effort);
            }
            super::save_default_model(saved)?;
        }
    }
    if let Some(mode) = options.mode {
        client.session(id, "/agent", "POST", Some(json!({"agent":mode})))?;
    }
    invalidate(id);
    Ok(json!({"ok":true,"model":model,"effort":effort,"scope":scope}))
}

pub(crate) fn rewind(id: &str, message_id: &str) -> Result<Value, DomainStateError> {
    invalidate(id);
    let current = snapshot(id)?;
    if current.working {
        return Err(error("Stop the OpenCode turn before rewinding."));
    }
    if !current
        .messages
        .iter()
        .any(|m| m["id"] == message_id && m["type"] == "user")
    {
        return Err(error(
            "The selected OpenCode prompt is no longer in this conversation.",
        ));
    }
    let client = Client::discover()?;
    // Conversation rewind has the same contract as Claude's conversation-only rewind.
    client.session(
        id,
        "/revert/stage",
        "POST",
        Some(json!({"messageID":message_id,"files":false})),
    )?;
    client.session(id, "/revert/commit", "POST", None)?;
    invalidate(id);
    Ok(json!({"ok":true,"targetMessageId":message_id,"warning":null}))
}

pub(crate) fn model_catalog(client: &Client, info: &Value) -> Result<Value, DomainStateError> {
    let directory = info["location"]["directory"]
        .as_str()
        .ok_or_else(|| error("OpenCode session has no directory."))?;
    let encoded: String = url::form_urlencoded::byte_serialize(directory.as_bytes()).collect();
    let response = client.request("GET", &format!("/api/model?directory={encoded}"), None)?;
    let models = response["data"]
        .as_array()
        .ok_or_else(|| error("Invalid OpenCode model catalog."))?;
    let rows: Vec<Value> = models.iter().filter_map(|model| {
        let provider = model["providerID"].as_str()?;
        let name = model["id"].as_str()?;
        let variants: Vec<_> = model["variants"].as_array().into_iter().flatten()
            .filter_map(|v| v["id"].as_str()).collect();
        Some(json!({"value":format!("{provider}/{name}"),"label":model["name"].as_str().unwrap_or(name),"efforts":variants,"contextWindow":model["limit"]["context"]}))
    }).collect();
    let efforts: std::collections::BTreeSet<&str> = rows
        .iter()
        .flat_map(|row| row["efforts"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    Ok(
        json!({"schemaVersion":1,"updatedAt":"2026-09-25","effortLabels":{},"agents":{"opencode":{"name":"OpenCode","efforts":efforts,"models":rows}}}),
    )
}
