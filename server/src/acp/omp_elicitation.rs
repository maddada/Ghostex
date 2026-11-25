//! OMP ACP helpers: `configOptions` metadata parsing and `elicitation/create` form bridging.
//!
//! OMP (`omp acp`) exposes model, thinking level, and mode choices through ACP `configOptions`
//! rather than `models` / `session/set_model`, and bridges its `ask` tool and extension UI
//! dialogs (`confirm`, `select`, `input`, `editor`) through ACP `elicitation/create` form
//! schemas (including paired `<id>` + `<id>__other` free-text fields).

use serde_json::{json, Map, Value};

/// Metadata attached to a `PendingPermission` originated from an ACP `elicitation/create`
/// JSON-RPC request rather than `session/request_permission`.
#[derive(Clone, Debug)]
pub(super) enum ElicitationMeta {
    SingleConfirm,
    SingleSelect {
        /// Wire values corresponding 1:1 with the displayed options.
        option_values: Vec<String>,
    },
    SingleInput,
    Form {
        fields: Vec<ElicitationField>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct ElicitationField {
    pub(super) key: String,
    pub(super) other_key: Option<String>,
    pub(super) field_type: String,
    pub(super) multi_select: bool,
    pub(super) option_values: Vec<String>,
}

/// Extracted session metadata from ACP `configOptions`.
#[derive(Default, Debug)]
pub(super) struct ParsedConfigOptions {
    pub(super) current_model: Option<String>,
    pub(super) available_models: Vec<Value>,
    pub(super) current_effort: Option<String>,
    pub(super) current_mode: Option<String>,
    pub(super) available_modes: Vec<Value>,
}

fn flatten_config_option_entries(raw_options: &Value) -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    let Some(items) = raw_options.as_array() else {
        return out;
    };
    for item in items {
        // Grouped config option: `{ group, name, options: [{ value, name, description }] }`.
        if let Some(nested) = item.get("options").and_then(Value::as_array) {
            for sub in nested {
                if let Some(val) = sub
                    .get("value")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    let name = sub
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(val);
                    let desc = sub
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                    out.push((val.to_string(), name.to_string(), desc));
                }
            }
            continue;
        }
        // Flat config option: `{ value, name, description }`.
        if let Some(val) = item
            .get("value")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(val);
            let desc = item
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            out.push((val.to_string(), name.to_string(), desc));
        }
    }
    out
}

/// Extract model, thinking/effort, and mode state from an ACP `configOptions` array.
pub(super) fn parse_config_options(configs: &Value) -> ParsedConfigOptions {
    use ghostex_gx_chat_core::menus::catalog::{
        is_placeholder_model_label, is_placeholder_model_value,
    };
    let mut parsed = ParsedConfigOptions::default();
    let Some(items) = configs.as_array() else {
        return parsed;
    };
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let category = item
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let current = item
            .get("currentValue")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let entries = item
            .get("options")
            .map(flatten_config_option_entries)
            .unwrap_or_default();

        if category == "model" || id == "model" {
            if let Some(val) = current.filter(|m| !is_placeholder_model_value(m)) {
                parsed.current_model = Some(val.to_string());
            }
            for (val, name, _desc) in entries {
                if is_placeholder_model_value(&val) || is_placeholder_model_label(&name) {
                    continue;
                }
                parsed.available_models.push(json!({
                    "modelId": val,
                    "id": val,
                    "name": name,
                }));
            }
        } else if category.contains("thought")
            || category.contains("effort")
            || id == "thinking"
            || id.contains("effort")
        {
            if let Some(val) = current
                .filter(|s| !ghostex_gx_chat_core::menus::catalog::is_placeholder_effort_value(s))
            {
                parsed.current_effort = Some(val.to_ascii_lowercase());
            }
        } else if category == "mode" || id == "mode" {
            if let Some(val) = current {
                parsed.current_mode = Some(val.to_string());
            }
            for (val, name, desc) in entries {
                let mut mode_obj = json!({
                    "id": val,
                    "name": name,
                });
                if let Some(description) = desc {
                    mode_obj["description"] = json!(description);
                }
                parsed.available_modes.push(mode_obj);
            }
        }
    }
    parsed
}

/// Locate the ACP `configId` for `"model"` inside `config_options`, when present.
pub(super) fn find_model_config_id(config_options: &Value) -> Option<String> {
    config_options.as_array()?.iter().find_map(|item| {
        let cid = item.get("id").and_then(Value::as_str)?.trim();
        let cat = item
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        (cat == "model" || cid == "model").then(|| cid.to_string())
    })
}

/// Locate the ACP `configId` for thinking/effort inside `config_options`, when present.
pub(super) fn find_effort_config_id(config_options: &Value) -> Option<String> {
    config_options.as_array()?.iter().find_map(|item| {
        let cid = item.get("id").and_then(Value::as_str)?.trim();
        let cat = item
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        (cat.contains("thought")
            || cat.contains("effort")
            || cid.contains("effort")
            || cid.contains("thinking"))
        .then(|| cid.to_string())
    })
}

fn extract_schema_options(schema: &Map<String, Value>) -> Vec<(String, String, Option<String>)> {
    if let Some(one_of) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(Value::as_array)
    {
        let mut out = Vec::new();
        for entry in one_of {
            let Some(obj) = entry.as_object() else {
                continue;
            };
            let Some(val) = obj
                .get("const")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let label = obj
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(val);
            let desc = obj
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            out.push((val.to_string(), label.to_string(), desc));
        }
        if !out.is_empty() {
            return out;
        }
    }
    if let Some(enum_vals) = schema.get("enum").and_then(Value::as_array) {
        let names = schema.get("enumNames").and_then(Value::as_array);
        let mut out = Vec::new();
        for (idx, item) in enum_vals.iter().enumerate() {
            let Some(val) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
                continue;
            };
            let label = names
                .and_then(|arr| arr.get(idx))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(val);
            out.push((val.to_string(), label.to_string(), None));
        }
        return out;
    }
    Vec::new()
}

/// Parse an ACP `elicitation/create` request into a Ghostex `PendingPermission` prompt + response
/// mapping metadata. Returns `None` for non-form or unsupported elicitation modes.
pub(super) fn parse_elicitation_request(
    rpc_id: Value,
    params: &Value,
) -> Option<(super::host::PendingPermission, ElicitationMeta)> {
    let mode = params.get("mode").and_then(Value::as_str).unwrap_or("form");
    if mode != "form" {
        return None;
    }
    let message = params
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let tool_use_id = params
        .get("toolCallId")
        .or_else(|| params.get("tool_call_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| Some(format!("elicitation-{}", rpc_id)));
    let properties = params
        .pointer("/requestedSchema/properties")
        .and_then(Value::as_object)?;
    if properties.is_empty() {
        return None;
    }

    // Single-value OMP extension dialog (`confirm`, `select`, `input`, `editor`).
    if properties.len() == 1 {
        if let Some(val_schema) = properties.get("value").and_then(Value::as_object) {
            let field_type = val_schema
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("string");
            let title = val_schema
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(if message.is_empty() {
                    "Question"
                } else {
                    message
                });
            if field_type == "boolean" {
                let summary = if !message.is_empty() {
                    message.to_string()
                } else {
                    title.to_string()
                };
                let prompt = json!({
                    "kind": "approval",
                    "tool": title,
                    "summary": summary,
                    "toolUseId": tool_use_id,
                    "options": [
                        { "optionId": "allow_once", "label": "Allow", "kind": "allow_once" },
                        { "optionId": "reject_once", "label": "Deny", "kind": "reject_once" }
                    ],
                });
                return Some((
                    super::host::PendingPermission {
                        rpc_id,
                        tool_name: title.to_string(),
                        summary,
                        tool_use_id,
                        reason: None,
                        is_question: false,
                        options: Vec::new(),
                        custom_prompt_override: Some(prompt),
                    },
                    ElicitationMeta::SingleConfirm,
                ));
            }

            let opts = extract_schema_options(val_schema);
            if !opts.is_empty() {
                let option_values: Vec<String> = opts.iter().map(|(v, _, _)| v.clone()).collect();
                let formatted_opts: Vec<Value> = opts
                    .iter()
                    .map(|(_, label, desc)| {
                        let mut item = json!({ "label": label });
                        if let Some(d) = desc {
                            item["description"] = json!(d);
                        }
                        item
                    })
                    .collect();
                let question_text = if !message.is_empty() { message } else { title };
                let prompt = json!({
                    "kind": "question",
                    "toolUseId": tool_use_id,
                    "questions": [{
                        "question": question_text,
                        "header": "Question",
                        "multiSelect": false,
                        "allowCustom": false,
                        "options": formatted_opts,
                    }],
                });
                return Some((
                    super::host::PendingPermission {
                        rpc_id,
                        tool_name: question_text.to_string(),
                        summary: question_text.to_string(),
                        tool_use_id,
                        reason: None,
                        is_question: true,
                        options: Vec::new(),
                        custom_prompt_override: Some(prompt),
                    },
                    ElicitationMeta::SingleSelect { option_values },
                ));
            }

            let question_text = if !message.is_empty() { message } else { title };
            let prompt = json!({
                "kind": "question",
                "toolUseId": tool_use_id,
                "questions": [{
                    "question": question_text,
                    "header": "Question",
                    "multiSelect": false,
                    "allowCustom": true,
                    "options": [],
                }],
            });
            return Some((
                super::host::PendingPermission {
                    rpc_id,
                    tool_name: question_text.to_string(),
                    summary: question_text.to_string(),
                    tool_use_id,
                    reason: None,
                    is_question: true,
                    options: Vec::new(),
                    custom_prompt_override: Some(prompt),
                },
                ElicitationMeta::SingleInput,
            ));
        }
    }

    // Multi-field elicitation form (including OMP's `ask` tool `qN` + `qN__other` pairs).
    let mut fields = Vec::new();
    let mut questions = Vec::new();

    for (key, raw_prop) in properties {
        if let Some(base) = key.strip_suffix("__other") {
            if properties.contains_key(base) {
                continue;
            }
        }
        let Some(prop) = raw_prop.as_object() else {
            continue;
        };
        let other_key_candidate = format!("{key}__other");
        let other_key = properties
            .contains_key(&other_key_candidate)
            .then_some(other_key_candidate);

        let field_type = prop
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("string")
            .to_string();
        let (multi_select, opts) = if field_type == "array" {
            let item_opts = prop
                .get("items")
                .and_then(Value::as_object)
                .map(extract_schema_options)
                .unwrap_or_default();
            (true, item_opts)
        } else {
            (false, extract_schema_options(prop))
        };

        let question_text = prop
            .get("title")
            .or_else(|| prop.get("description"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(key.as_str());
        let header = prop
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != question_text && s.len() <= 32)
            .unwrap_or("Question");

        let allow_custom = other_key.is_some() || opts.is_empty();
        let option_values: Vec<String> = opts.iter().map(|(v, _, _)| v.clone()).collect();
        let formatted_opts: Vec<Value> = opts
            .iter()
            .map(|(_, label, desc)| {
                let mut item = json!({ "label": label });
                if let Some(d) = desc {
                    item["description"] = json!(d);
                }
                item
            })
            .collect();

        questions.push(json!({
            "question": question_text,
            "header": header,
            "multiSelect": multi_select,
            "allowCustom": allow_custom,
            "options": formatted_opts,
        }));
        fields.push(ElicitationField {
            key: key.clone(),
            other_key,
            field_type,
            multi_select,
            option_values,
        });
    }

    if questions.is_empty() {
        return None;
    }

    let summary = if !message.is_empty() {
        message.to_string()
    } else {
        questions
            .first()
            .and_then(|q| q.get("question"))
            .and_then(Value::as_str)
            .unwrap_or("Question")
            .to_string()
    };
    let prompt = json!({
        "kind": "question",
        "toolUseId": tool_use_id,
        "questions": questions,
    });

    Some((
        super::host::PendingPermission {
            rpc_id,
            tool_name: summary.clone(),
            summary,
            tool_use_id,
            reason: None,
            is_question: true,
            options: Vec::new(),
            custom_prompt_override: Some(prompt),
        },
        ElicitationMeta::Form { fields },
    ))
}

/// Build the JSON-RPC result for an `elicitation/create` confirm answered through `"approve"`.
pub(super) fn build_confirm_elicitation_response(decision: &str) -> Value {
    let norm = decision.trim().to_ascii_lowercase();
    match norm.as_str() {
        "allow" | "allow_once" | "always" | "allow_always" | "1" | "2" | "y" | "yes" => {
            json!({ "action": "accept", "content": { "value": true } })
        }
        "escape" | "cancel" | "\x1b" => json!({ "action": "cancel" }),
        _ => json!({ "action": "accept", "content": { "value": false } }),
    }
}

/// Build the JSON-RPC result for an `elicitation/create` question answered through
/// `"answer_question"`.
pub(super) fn build_question_elicitation_response(
    meta: &ElicitationMeta,
    selections: &[Value],
) -> Value {
    match meta {
        ElicitationMeta::SingleConfirm => json!({ "action": "cancel" }),
        ElicitationMeta::SingleSelect { option_values } => {
            let Some(first) = selections.first() else {
                return json!({ "action": "cancel" });
            };
            if let Some(idx) = first
                .get("indices")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_u64)
            {
                if let Some(val) = option_values.get(idx as usize) {
                    return json!({ "action": "accept", "content": { "value": val } });
                }
            }
            if let Some(other) = first
                .get("other")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return json!({ "action": "accept", "content": { "value": other } });
            }
            json!({ "action": "cancel" })
        }
        ElicitationMeta::SingleInput => {
            let Some(text) = selections
                .first()
                .and_then(|s| s.get("other"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return json!({ "action": "cancel" });
            };
            json!({ "action": "accept", "content": { "value": text } })
        }
        ElicitationMeta::Form { fields } => {
            if selections.is_empty() {
                return json!({ "action": "cancel" });
            }
            let mut content = Map::new();
            for (idx, field) in fields.iter().enumerate() {
                let Some(sel) = selections.get(idx) else {
                    continue;
                };
                let indices: Vec<usize> = sel
                    .get("indices")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_u64)
                            .map(|n| n as usize)
                            .collect()
                    })
                    .unwrap_or_default();
                let other = sel
                    .get("other")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty());

                if field.multi_select {
                    let mut chosen: Vec<Value> = indices
                        .iter()
                        .filter_map(|i| field.option_values.get(*i))
                        .map(|v| json!(v))
                        .collect();
                    if let Some(custom) = other {
                        if let Some(other_key) = &field.other_key {
                            content.insert(other_key.clone(), json!(custom));
                        } else {
                            chosen.push(json!(custom));
                        }
                    }
                    if !chosen.is_empty() {
                        content.insert(field.key.clone(), Value::Array(chosen));
                    }
                } else {
                    let chosen = indices
                        .first()
                        .and_then(|i| field.option_values.get(*i))
                        .cloned();
                    if let Some(val) = chosen {
                        content.insert(field.key.clone(), json!(val));
                    }
                    if let Some(custom) = other {
                        if let Some(other_key) = &field.other_key {
                            content.insert(other_key.clone(), json!(custom));
                        } else if field.field_type == "boolean" {
                            let truthy = matches!(
                                custom.to_ascii_lowercase().as_str(),
                                "true" | "yes" | "y" | "1"
                            );
                            content.insert(field.key.clone(), json!(truthy));
                        } else {
                            content.insert(field.key.clone(), json!(custom));
                        }
                    }
                }
            }
            if content.is_empty() {
                json!({ "action": "cancel" })
            } else {
                json!({ "action": "accept", "content": Value::Object(content) })
            }
        }
    }
}
