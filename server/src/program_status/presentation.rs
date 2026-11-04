use serde_json::{json, Map, Value};

use super::{
    derive_program_status, effective_program_app, program_status_combines_entry,
    program_status_display_text, program_status_entry,
};

/// CDXC:SessionStatus 2026-10-14 WHY:
/// The session's OSC 7501 records as every client sees them (`programStatus` on the presentation session, `ghostex sessions --json`). `governs` says whether they are in use for the session's status (while the reporting program runs), which is when clients show the message and the red failed dot; `state`, `kind`, `progress` and `message` describe the record that decides the row; `records` are all of them, ids and titles included, for clients that show more. Absent when the session has no records and nothing to say.
/// SEE-ALSO: packages/gx-protocol/src/presentation.rs `ProgramStatus`, packages/gx-core/src/sidebar_view/rows.rs (hover text and the red failed dot).
pub(crate) fn presentation_program_status(project_id: &str, session_id: &str) -> Option<Value> {
    let entry = program_status_entry(project_id, session_id)?;
    let governs = program_status_combines_entry(Some(&entry));
    if entry.records.is_empty() && !governs {
        return None;
    }
    let derived = derive_program_status(&entry.records);
    let mut output = Map::new();
    output.insert("governs".to_string(), json!(governs));
    output.insert("state".to_string(), json!(derived.state));
    if let Some(record) = derived.record.as_ref() {
        if derived.state == "blocked" {
            if let Some(kind) = record.kind.as_ref() {
                output.insert("kind".to_string(), json!(kind));
            }
        }
        if let Some(progress) = record.progress {
            output.insert("progress".to_string(), json!(progress));
        }
        if let Some(app) = effective_program_app(&entry.records, record) {
            output.insert("app".to_string(), json!(app));
        }
        let title = record
            .title
            .as_deref()
            .and_then(program_status_display_text);
        let message = record.msg.as_deref().and_then(program_status_display_text);
        // A worker's title says which worker the message is about.
        let message = match (title, message) {
            (Some(title), Some(message)) if !record.id.is_empty() => {
                Some(format!("{title}: {message}"))
            }
            (title, message) => message.or(title),
        };
        if let Some(message) = message {
            output.insert("message".to_string(), json!(message));
        }
    }
    let records: Vec<Value> = entry
        .records
        .iter()
        .map(|record| {
            let mut value = serde_json::to_value(record).unwrap_or(Value::Null);
            if let Some(object) = value.as_object_mut() {
                object.remove("seq");
                for key in ["title", "msg"] {
                    let cleaned = object
                        .get(key)
                        .and_then(Value::as_str)
                        .and_then(program_status_display_text);
                    match cleaned {
                        Some(text) => object.insert(key.to_string(), json!(text)),
                        None => object.remove(key),
                    };
                }
                if record.app.is_none() {
                    if let Some(app) = effective_program_app(&entry.records, record) {
                        object.insert("app".to_string(), json!(app));
                    }
                }
            }
            value
        })
        .collect();
    output.insert("records".to_string(), Value::Array(records));
    Some(Value::Object(output))
}
