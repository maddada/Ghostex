use super::{error, invalidate, snapshot, Client};
use crate::domain::DomainStateError;
use serde_json::{json, Map, Value};

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

/// Renames the conversation through OpenCode's service (`PATCH /api/session/{id}`, what the TUI's own
/// `/rename` calls), instead of typing `/rename` into the terminal.
pub(crate) fn rename(id: &str, title: &str) -> Result<(), DomainStateError> {
    Client::discover()?.session(id, "", "PATCH", Some(json!({ "title": title })))?;
    invalidate(id);
    Ok(())
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
    let entry = lineup_entry(&response, || {
        client
            .request("GET", &format!("/api/provider?directory={encoded}"), None)
            .ok()
    })
    .ok_or_else(|| error("Invalid OpenCode model catalog."))?;
    Ok(
        json!({"schemaVersion":1,"updatedAt":"2026-09-25","effortLabels":{},"agents":{"opencode":entry}}),
    )
}

/// The catalog agent entry for an answer of OpenCode's `/api/model`: one row per model, keyed
/// `provider/model` as `select` sends it, with the variants it offers as its efforts. Shared by a
/// thread's own lineup and the one gxserver serves to every thread (`agent_model_lineups.rs`).
///
/// CDXC:AgentProviders 2026-11-07 WHY:
/// Issue #482: OpenCode Console (`opencode`) and OpenCode Go (`opencode-go`) list 17 models under the same name, so the picker showed identical rows and the pill could not say whether a thread bills per token or uses the Go subscription. A name more than one provider uses gets that provider's own display name (`/api/provider`, asked through `providers` only when such a clash exists, else the provider id): "DeepSeek V4.1 Flash · OpenCode Go". The row label carries it, so the picker rows, the pill and the search all read it on the desktop and the phone alike; a name only one provider uses stays bare.
pub(crate) fn lineup_entry(
    response: &Value,
    providers: impl FnOnce() -> Option<Value>,
) -> Option<Value> {
    let models = response["data"].as_array()?;
    let named: Vec<(&str, &str, String, &Value)> = models
        .iter()
        .filter_map(|model| {
            let provider = model["providerID"].as_str()?;
            let name = model["id"].as_str()?;
            let label = model["name"]
                .as_str()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .unwrap_or(name);
            Some((provider, name, label.to_string(), model))
        })
        .collect();
    let mut providers_of_label: std::collections::HashMap<String, std::collections::HashSet<&str>> =
        std::collections::HashMap::new();
    for (provider, _, label, _) in &named {
        providers_of_label
            .entry(label.to_lowercase())
            .or_default()
            .insert(provider);
    }
    let shared = |label: &str| {
        providers_of_label
            .get(&label.to_lowercase())
            .is_some_and(|providers| providers.len() > 1)
    };
    let provider_names: std::collections::HashMap<String, String> =
        if named.iter().any(|(_, _, label, _)| shared(label)) {
            providers()
                .and_then(|answer| answer["data"].as_array().cloned())
                .into_iter()
                .flatten()
                .filter_map(|provider| {
                    let id = provider["id"].as_str()?;
                    let name = provider["name"].as_str().map(str::trim)?;
                    (!name.is_empty()).then(|| (id.to_string(), name.to_string()))
                })
                .collect()
        } else {
            Default::default()
        };
    let rows: Vec<Value> = named
        .iter()
        .map(|(provider, name, label, model)| {
            let variants: Vec<_> = model["variants"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v["id"].as_str())
                .collect();
            let label = if shared(label) {
                let provider_name = provider_names
                    .get(*provider)
                    .map(String::as_str)
                    .unwrap_or(provider);
                format!("{label} \u{b7} {provider_name}")
            } else {
                label.clone()
            };
            json!({"value":format!("{provider}/{name}"),"label":label,"efforts":variants,"contextWindow":model["limit"]["context"]})
        })
        .collect();
    let efforts: std::collections::BTreeSet<&str> = rows
        .iter()
        .flat_map(|row| row["efforts"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    Some(json!({"name":"OpenCode","efforts":efforts,"models":rows}))
}
