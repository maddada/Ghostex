//! ACP form elicitation (`elicitation/create`) parser and responder.
//!
//! Translates ACP `mode: "form"` JSON-Schema elicitation requests into Ghostex's
//! `kind: "question"` prompt cards, and maps Ghostex `answer_question` selections
//! back into `{ "action": "accept", "content": { ... } }` or `{ "action": "cancel" }`.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};

#[derive(Clone, Debug)]
enum ElicitationFieldKind {
    Text,
    Number {
        integer: bool,
    },
    Boolean,
    Choice {
        options: Vec<ElicitationChoiceOption>,
        other_companion_key: Option<String>,
    },
    MultiChoice {
        options: Vec<ElicitationChoiceOption>,
        other_companion_key: Option<String>,
    },
}

#[derive(Clone, Debug)]
struct ElicitationChoiceOption {
    value: String,
    label: String,
    description: Option<String>,
}

#[derive(Clone, Debug)]
struct ElicitationField {
    key: String,
    label: String,
    description: Option<String>,
    required: bool,
    kind: ElicitationFieldKind,
}

/// Parsed ACP form elicitation kept alongside a pending question prompt so Ghostex
/// can map user selections back into typed JSON-Schema field values.
#[derive(Clone, Debug)]
pub(super) struct PendingElicitation {
    message: String,
    fields: Vec<ElicitationField>,
    /// The agent puts each question in its field's `title` and only a count ("The agent has 2
    /// questions.") in `message` (Antigravity's `ask_question`).
    titles_are_questions: bool,
}

fn non_empty_str<'a>(value: Option<&'a Value>) -> Option<&'a str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn parse_string_choices(schema: &Map<String, Value>) -> Option<Vec<ElicitationChoiceOption>> {
    if let Some(enum_vals) = schema.get("enum").and_then(Value::as_array) {
        let names = schema.get("enumNames").and_then(Value::as_array);
        let mut options = Vec::new();
        for (idx, item) in enum_vals.iter().enumerate() {
            let Some(wire) = item.as_str() else {
                continue;
            };
            let label = names
                .and_then(|arr| arr.get(idx))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(wire);
            options.push(ElicitationChoiceOption {
                value: wire.to_string(),
                label: label.to_string(),
                description: None,
            });
        }
        if !options.is_empty() {
            return Some(options);
        }
    }
    if let Some(one_of) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(Value::as_array)
    {
        let mut options = Vec::new();
        for item in one_of {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let Some(wire) = obj
                .get("const")
                .and_then(Value::as_str)
                .or_else(|| non_empty_str(obj.get("title")))
            else {
                continue;
            };
            let label = non_empty_str(obj.get("title")).unwrap_or(wire);
            let description = non_empty_str(obj.get("description")).map(str::to_string);
            options.push(ElicitationChoiceOption {
                value: wire.to_string(),
                label: label.to_string(),
                description,
            });
        }
        if !options.is_empty() {
            return Some(options);
        }
    }
    None
}

fn parse_array_choices(schema: &Map<String, Value>) -> Option<Vec<ElicitationChoiceOption>> {
    let items = schema.get("items").and_then(Value::as_object)?;
    parse_string_choices(items)
}

fn is_plain_string_schema(schema: &Map<String, Value>) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("string")
        && parse_string_choices(schema).is_none()
}

/// Parse an ACP `elicitation/create` params payload into a [`PendingElicitation`].
/// Returns `None` for non-form modes (such as `url`) or unsupported/empty schemas.
pub(super) fn parse_elicitation_request(params: &Value) -> Option<PendingElicitation> {
    let mode = params
        .get("mode")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("form");
    if !mode.is_empty() && !mode.eq_ignore_ascii_case("form") {
        return None;
    }
    let message = non_empty_str(params.get("message"))
        .unwrap_or("Please answer to continue.")
        .to_string();
    let requested = params.get("requestedSchema")?.as_object()?;
    if let Some(schema_type) = requested.get("type").and_then(Value::as_str) {
        if schema_type != "object" {
            return None;
        }
    }
    let properties = requested.get("properties")?.as_object()?;
    if properties.is_empty() {
        return None;
    }
    let required_keys: HashSet<&str> = requested
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    // Detect `<field>__other` (or `<field>_other`, Antigravity's `ask_question`) free-text
    // companion properties folded into choice/multiChoice fields.
    let mut companion_keys: HashSet<String> = HashSet::new();
    let mut companion_for: HashMap<String, String> = HashMap::new();
    for (key, prop) in properties {
        let Some(base) = key
            .strip_suffix("__other")
            .filter(|base| properties.contains_key(*base))
            .or_else(|| key.strip_suffix("_other"))
        else {
            continue;
        };
        if companion_for.contains_key(base) {
            continue;
        }
        if base.is_empty() || !properties.contains_key(base) {
            continue;
        }
        let Some(prop_obj) = prop.as_object() else {
            continue;
        };
        let Some(base_obj) = properties.get(base).and_then(Value::as_object) else {
            continue;
        };
        let base_is_choice = parse_string_choices(base_obj).is_some()
            || (base_obj.get("type").and_then(Value::as_str) == Some("array")
                && parse_array_choices(base_obj).is_some());
        if base_is_choice && is_plain_string_schema(prop_obj) {
            companion_keys.insert(key.clone());
            companion_for.insert(base.to_string(), key.clone());
        }
    }

    let mut fields = Vec::new();
    for (key, prop) in properties {
        if companion_keys.contains(key) {
            continue;
        }
        let Some(schema) = prop.as_object() else {
            return None;
        };
        let ftype = schema.get("type").and_then(Value::as_str).unwrap_or("");
        let kind = if ftype == "boolean" {
            ElicitationFieldKind::Boolean
        } else if ftype == "integer" {
            ElicitationFieldKind::Number { integer: true }
        } else if ftype == "number" {
            ElicitationFieldKind::Number { integer: false }
        } else if ftype == "array" {
            let options = parse_array_choices(schema)?;
            ElicitationFieldKind::MultiChoice {
                options,
                other_companion_key: companion_for.get(key).cloned(),
            }
        } else if let Some(options) = parse_string_choices(schema) {
            ElicitationFieldKind::Choice {
                options,
                other_companion_key: companion_for.get(key).cloned(),
            }
        } else if ftype == "string" || ftype.is_empty() {
            ElicitationFieldKind::Text
        } else {
            return None;
        };

        let label = non_empty_str(schema.get("title"))
            .unwrap_or(key.as_str())
            .to_string();
        let description = non_empty_str(schema.get("description")).map(str::to_string);
        fields.push(ElicitationField {
            key: key.clone(),
            label,
            description,
            required: required_keys.contains(key.as_str()),
            kind,
        });
    }

    if fields.is_empty() {
        return None;
    }

    Some(PendingElicitation {
        message,
        fields,
        titles_are_questions: false,
    })
}

impl PendingElicitation {
    /// Read each field's title as its question (Antigravity's `ask_question`), for fields that
    /// have a title and no description.
    pub(super) fn use_field_titles_as_questions(&mut self) {
        self.titles_are_questions = true;
    }

    pub(super) fn title(&self) -> &str {
        &self.message
    }

    /// One Ghostex question row for `field`, with its options.
    fn question_json(
        &self,
        field: &ElicitationField,
        header: String,
        question_text: String,
    ) -> Value {
        let (multi_select, allow_custom, options) = match &field.kind {
            ElicitationFieldKind::Text | ElicitationFieldKind::Number { .. } => {
                (false, true, Vec::new())
            }
            ElicitationFieldKind::Boolean => (
                false,
                false,
                vec![json!({ "label": "Yes" }), json!({ "label": "No" })],
            ),
            ElicitationFieldKind::Choice { options, .. } => {
                let rendered = options
                    .iter()
                    .map(|opt| {
                        let mut row = json!({ "label": opt.label });
                        if let Some(desc) = opt.description.as_deref() {
                            row["description"] = json!(desc);
                        }
                        row
                    })
                    .collect();
                (false, true, rendered)
            }
            ElicitationFieldKind::MultiChoice { options, .. } => {
                let rendered = options
                    .iter()
                    .map(|opt| {
                        let mut row = json!({ "label": opt.label });
                        if let Some(desc) = opt.description.as_deref() {
                            row["description"] = json!(desc);
                        }
                        row
                    })
                    .collect();
                (true, true, rendered)
            }
        };
        json!({
            "id": field.key,
            "header": header,
            "question": question_text,
            "multiSelect": multi_select,
            "allowCustom": allow_custom,
            "options": options,
        })
    }

    /// Build a Ghostex `kind: "question"` prompt payload matching Claude/Codex `AskUserQuestion`.
    pub(super) fn to_ghostex_prompt(&self, tool_use_id: Option<&str>) -> Value {
        let single_field = self.fields.len() == 1;
        let questions: Vec<Value> = self
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let titled = self.titles_are_questions
                    && field.description.is_none()
                    && field.label != field.key;
                if titled {
                    let header = format!("Question {}", index + 1);
                    return self.question_json(field, header, field.label.clone());
                }
                let question_text = if single_field {
                    self.message.clone()
                } else if let Some(desc) = field.description.as_deref() {
                    format!("{}: {} ({})", self.message, field.label, desc)
                } else {
                    format!("{}: {}", self.message, field.label)
                };
                let header = if single_field && field.key.eq_ignore_ascii_case("answer") {
                    "Question".to_string()
                } else {
                    field.label.clone()
                };
                self.question_json(field, header, question_text)
            })
            .collect();

        json!({
            "kind": "question",
            "toolUseId": tool_use_id,
            "questions": questions,
        })
    }

    /// Map Ghostex `answer_question` `selections` into an ACP `elicitation/create` response
    /// (`{ "action": "accept", "content": ... }` or `{ "action": "cancel" }`).
    pub(super) fn build_response(&self, selections: &[Value]) -> Value {
        let mut content = Map::new();
        for (idx, field) in self.fields.iter().enumerate() {
            let Some(sel) = selections.get(idx) else {
                continue;
            };
            let other = non_empty_str(sel.get("other"));
            let indices: Vec<usize> = sel
                .get("indices")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_u64)
                        .map(|i| i as usize)
                        .collect()
                })
                .unwrap_or_default();

            match &field.kind {
                ElicitationFieldKind::Text => {
                    if let Some(text) = other {
                        content.insert(field.key.clone(), Value::String(text.to_string()));
                    }
                }
                ElicitationFieldKind::Number { integer } => {
                    if let Some(text) = other {
                        let parsed = if *integer {
                            text.parse::<i64>().ok().map(Value::from)
                        } else {
                            text.parse::<f64>()
                                .ok()
                                .filter(|v| v.is_finite())
                                .and_then(serde_json::Number::from_f64)
                                .map(Value::Number)
                        };
                        if let Some(num) = parsed {
                            content.insert(field.key.clone(), num);
                        } else if !field.required {
                            continue;
                        } else {
                            content.insert(field.key.clone(), Value::String(text.to_string()));
                        }
                    }
                }
                ElicitationFieldKind::Boolean => {
                    let chosen = indices
                        .first()
                        .and_then(|&i| match i {
                            0 => Some(true),
                            1 => Some(false),
                            _ => None,
                        })
                        .or_else(|| {
                            other.and_then(|text| match text.to_ascii_lowercase().as_str() {
                                "yes" | "true" | "1" => Some(true),
                                "no" | "false" | "0" => Some(false),
                                _ => None,
                            })
                        });
                    if let Some(flag) = chosen {
                        content.insert(field.key.clone(), Value::Bool(flag));
                    }
                }
                ElicitationFieldKind::Choice {
                    options,
                    other_companion_key,
                } => {
                    if let Some(text) = other {
                        if let Some(companion) = other_companion_key {
                            content.insert(companion.clone(), Value::String(text.to_string()));
                            if let Some(opt) = indices.first().and_then(|&i| options.get(i)) {
                                content.insert(field.key.clone(), Value::String(opt.value.clone()));
                            }
                        } else {
                            // Prefer matching an option label/value if exact, else pass free text.
                            let matched = options.iter().find(|opt| {
                                opt.value == text || opt.label.eq_ignore_ascii_case(text)
                            });
                            let wire = matched
                                .map(|opt| opt.value.clone())
                                .unwrap_or_else(|| text.to_string());
                            content.insert(field.key.clone(), Value::String(wire));
                        }
                    } else if let Some(opt) = indices.first().and_then(|&i| options.get(i)) {
                        content.insert(field.key.clone(), Value::String(opt.value.clone()));
                    }
                }
                ElicitationFieldKind::MultiChoice {
                    options,
                    other_companion_key,
                } => {
                    let mut selected: Vec<Value> = Vec::new();
                    for i in indices {
                        if let Some(opt) = options.get(i) {
                            let val = Value::String(opt.value.clone());
                            if !selected.contains(&val) {
                                selected.push(val);
                            }
                        }
                    }
                    if let Some(text) = other {
                        if let Some(companion) = other_companion_key {
                            content.insert(companion.clone(), Value::String(text.to_string()));
                        } else {
                            let val = Value::String(text.to_string());
                            if !selected.contains(&val) {
                                selected.push(val);
                            }
                        }
                    }
                    if !selected.is_empty() {
                        content.insert(field.key.clone(), Value::Array(selected));
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
