//! Cmd+F over the transcript, ported from
//! `packages/shared/session-chat-presentation/transcript-search.ts`.
//!
//! React searched the rendered DOM because it had one; the native chat has a list of projected
//! items instead, so the same query runs over the text those items carry. It is case-insensitive,
//! counts occurrences per row, and keeps the selected occurrence across a transcript refresh, as
//! the React search did.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One occurrence of the query.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptMatch {
    /// Index into the transcript item list the host ships to the renderer.
    pub item_index: usize,
    /// Identity that survives a transcript refresh: row id plus occurrence.
    pub key: String,
}

/// `sessionChatTranscriptItemText`: everything a reader can see in one transcript row, joined for
/// matching.
pub fn transcript_item_text(item: &Value) -> String {
    let Some(record) = item.as_object() else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(label) = record.get("label").and_then(Value::as_str) {
        parts.push(label.to_string());
    }
    for key in ["message", "user", "final"] {
        match record.get(key) {
            // `if (record[key])` skips `null`, `undefined` and the empty string alike.
            Some(value) if is_truthy(value) => parts.push(message_text(value)),
            _ => {}
        }
    }
    for key in ["work", "artifacts"] {
        if let Some(list) = record.get(key).and_then(Value::as_array) {
            for message in list {
                parts.push(message_text(message));
            }
        }
    }
    join_truthy(parts)
}

/// `sessionChatTranscriptMatches`: every occurrence of `query`, row by row, in transcript order.
pub fn transcript_matches(items: &[Value], query: &str) -> Vec<TranscriptMatch> {
    let needle = js_trim(query).to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let needle_units = needle.encode_utf16().count();
    let mut matches = Vec::new();
    for (item_index, item) in items.iter().enumerate() {
        let haystack = transcript_item_text(item).to_lowercase();
        if haystack.is_empty() {
            continue;
        }
        let id = item_id(item);
        // The TypeScript walks the string by UTF-16 index, and a `key` only has to be stable, so
        // the occurrence counter is what the two sides must agree on.
        let units: Vec<u16> = haystack.encode_utf16().collect();
        let needle_utf16: Vec<u16> = needle.encode_utf16().collect();
        let mut occurrence = 0usize;
        let mut at = 0usize;
        while let Some(found) = index_of(&units, &needle_utf16, at) {
            matches.push(TranscriptMatch {
                item_index,
                key: format!("{id}#{occurrence}"),
            });
            occurrence += 1;
            at = found + needle_units;
        }
    }
    matches
}

/// `sessionChatSearchCountLabel`: the terminal-style counter beside the field.
pub fn search_count_label(query: &str, total: usize, active_index: usize) -> String {
    if js_trim(query).is_empty() {
        return String::new();
    }
    if total > 0 {
        format!("{}/{total}", active_index + 1)
    } else {
        "N/A".to_string()
    }
}

/// `messageText`: the text, the suppressed label, each tool's name and preview, each file's path.
fn message_text(message: &Value) -> String {
    let Some(record) = message.as_object() else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(text) = record.get("text").and_then(Value::as_str) {
        parts.push(text.to_string());
    }
    if let Some(label) = record
        .get("suppressed")
        .and_then(|value| value.get("label"))
        .and_then(Value::as_str)
    {
        parts.push(label.to_string());
    }
    if let Some(tools) = record.get("tools").and_then(Value::as_array) {
        for tool in tools {
            if let Some(name) = tool
                .get("call")
                .and_then(|call| call.get("name"))
                .and_then(Value::as_str)
            {
                parts.push(name.to_string());
            }
            if let Some(preview) = tool.get("preview").and_then(Value::as_str) {
                parts.push(preview.to_string());
            }
        }
    }
    if let Some(files) = record.get("files").and_then(Value::as_array) {
        for file in files {
            if let Some(path) = file.get("path").and_then(Value::as_str) {
                parts.push(path.to_string());
            }
        }
    }
    join_truthy(parts)
}

/// `itemId`: the row kind plus whichever id the row carries.
fn item_id(item: &Value) -> String {
    let kind = item.get("kind").and_then(Value::as_str).unwrap_or_default();
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| {
            item.get("message")
                .and_then(|message| message.get("id"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    format!("{kind}:{id}")
}

/// `.filter(Boolean).join('\n')`.
fn join_truthy(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// JavaScript truthiness, for the three keys the row text reads conditionally.
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0),
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

/// `String.prototype.indexOf` over UTF-16 code units, from `at`.
fn index_of(haystack: &[u16], needle: &[u16], at: usize) -> Option<usize> {
    if needle.is_empty() || at + needle.len() > haystack.len() {
        return None;
    }
    (at..=haystack.len() - needle.len())
        .find(|start| &haystack[*start..*start + needle.len()] == needle)
}

/// `String.prototype.trim`, re-exported for the search's own use.
fn js_trim(value: &str) -> &str {
    crate::extras::agent_tasks::js_trim(value)
}
