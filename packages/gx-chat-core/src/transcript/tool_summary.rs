//! Tool input previews and the expanded tool body.
//!
//! Ported from `packages/core-ui/chat/session-chat-tool-summary.ts`.

use serde_json::{Map, Value};

use crate::transcript::json_text::{stringify, stringify_pretty};
use crate::transcript::jsstr::{
    collapse_whitespace, js_trim, js_trim_end, split_newlines, utf16_len, utf16_take,
};

pub const MAX_PREVIEW_LENGTH: usize = 80;
pub const MAX_PREVIEW_STRING_INPUT: usize = 160;
pub const MAX_PREVIEW_COLLECTION_ITEMS: usize = 8;
pub const MAX_PREVIEW_DEPTH: usize = 2;
pub const MAX_COMMAND_PREVIEW_LINES: usize = 3;

/// CDXC:SessionChat 2026-09-06 DECISION:
/// User: show an ellipsis at the end of tool-call text when it is truncated.
pub fn truncate_tool_preview(text: &str, max_length: usize) -> String {
    if utf16_len(text) <= max_length {
        return text.to_string();
    }
    format!("{}\u{2026}", js_trim_end(utf16_take(text, max_length - 1)))
}

/// A value trimmed to what a preview can carry: bounded strings, bounded collections, bounded
/// depth.
///
/// The TypeScript also guards against a value that appears twice in the tree by marking it
/// `[circular]`. A tree parsed from the wire never shares a node, so that arm is unreachable here.
fn bounded_preview_value(value: &Value, depth: usize) -> Value {
    match value {
        Value::String(text) => {
            if utf16_len(text) > MAX_PREVIEW_STRING_INPUT {
                Value::String(format!(
                    "{}\u{2026}",
                    utf16_take(text, MAX_PREVIEW_STRING_INPUT)
                ))
            } else {
                value.clone()
            }
        }
        Value::Array(items) => {
            if depth >= MAX_PREVIEW_DEPTH {
                return Value::String("[\u{2026}]".to_string());
            }
            let mut bounded: Vec<Value> = items
                .iter()
                .take(MAX_PREVIEW_COLLECTION_ITEMS)
                .map(|item| bounded_preview_value(item, depth + 1))
                .collect();
            if items.len() > MAX_PREVIEW_COLLECTION_ITEMS {
                bounded.push(Value::String("\u{2026}".to_string()));
            }
            Value::Array(bounded)
        }
        Value::Object(entries) => {
            if depth >= MAX_PREVIEW_DEPTH {
                return Value::String("[\u{2026}]".to_string());
            }
            let mut out = Map::new();
            for (key, item) in entries.iter().take(MAX_PREVIEW_COLLECTION_ITEMS) {
                out.insert(key.clone(), bounded_preview_value(item, depth + 1));
            }
            if entries.len() > MAX_PREVIEW_COLLECTION_ITEMS {
                out.insert(
                    "\u{2026}".to_string(),
                    Value::String("\u{2026}".to_string()),
                );
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}

/// `String(value)` for the primitives a tool input can hold.
fn primitive_text(value: &Value) -> String {
    match value {
        Value::Bool(flag) => flag.to_string(),
        Value::Number(_) => stringify(value),
        _ => String::new(),
    }
}

fn raw_preview(input: &Value) -> String {
    match input {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(_) | Value::Object(_) => stringify(&bounded_preview_value(input, 0)),
        other => primitive_text(other),
    }
}

/// CDXC:SessionChat 2026-09-26 WHY: a tool row used to preview its whole argument object as JSON, so a Read of a file under a long temporary folder showed `{"file_path":"/private/tmp/claude-501/…` with the file name cut off. A row now previews the argument that names the call: the search pattern or query, the URL, the file (relative to the session's folder, or `~/…`), the task's description, the skill, or the stopped task; the JSON stays for tools with none of these.
pub fn summarize_primary_argument(
    input: &Value,
    working_directory: Option<&str>,
) -> Option<String> {
    let Value::Object(entries) = input else {
        return None;
    };
    let text = |key: &str| non_empty_string(entries.get(key)).map(str::to_string);
    let primary = text("pattern")
        .or_else(|| text("query"))
        .or_else(|| text("url"))
        .or_else(|| {
            tool_file_path(input).map(|path| {
                crate::transcript::file_change_rows::file_change_display_path(
                    path,
                    working_directory,
                )
            })
        })
        .or_else(|| text("description"))
        .or_else(|| text("skill"))
        .or_else(|| text("task_id"))
        .or_else(|| text("shell_id"))
        .or_else(|| text("subject"))
        .or_else(|| match entries.get("todos") {
            Some(Value::Array(todos)) => Some(format!(
                "{} {}",
                todos.len(),
                if todos.len() == 1 { "item" } else { "items" }
            )),
            _ => None,
        })?;
    let collapsed = js_trim(&collapse_whitespace(&primary)).to_string();
    (!collapsed.is_empty()).then(|| truncate_tool_preview(&collapsed, MAX_PREVIEW_LENGTH))
}

/// The compact one-line form of a tool's arguments.
pub fn summarize_tool_input(input: &Value) -> String {
    let collapsed = js_trim(&collapse_whitespace(&raw_preview(input))).to_string();
    truncate_tool_preview(&collapsed, MAX_PREVIEW_LENGTH)
}

/// `\b` before an ASCII word character, as JavaScript reads it.
fn at_word_boundary(text: &str, at: usize) -> bool {
    let before = text[..at].chars().next_back();
    !before.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// A JSON string literal starting at `at`, as `("(?:\\.|[^"\\])*")` reads it.
fn json_string_literal(text: &str, at: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    let mut index = at + 1;
    while index < text.len() {
        match bytes[index] {
            b'\\' => {
                if index + 1 >= text.len() {
                    return None;
                }
                index += 1;
                while !text.is_char_boundary(index + 1) {
                    index += 1;
                }
                index += 1;
            }
            b'"' => return Some(&text[at..index + 1]),
            _ => index += 1,
        }
    }
    None
}

/// `(?:\bcmd|["']cmd["']|\bcommand|["']command["'])\s*:\s*("(?:\\.|[^"\\])*")` with the `s` flag.
///
/// Returns the JSON string literal the command hides behind, still quoted.
fn embedded_command_literal(input: &str) -> Option<&str> {
    let bytes = input.as_bytes();
    for start in 0..input.len() {
        if !input.is_char_boundary(start) {
            continue;
        }
        let rest = &input[start..];
        let quoted = |word: &str| {
            let length = word.len() + 2;
            rest.len() >= length
                && matches!(bytes[start], b'"' | b'\'')
                && matches!(bytes[start + length - 1], b'"' | b'\'')
                && rest[1..word.len() + 1] == *word
        };
        let head = if rest.starts_with("cmd") && at_word_boundary(input, start) {
            Some(3)
        } else if quoted("cmd") {
            Some(5)
        } else if rest.starts_with("command") && at_word_boundary(input, start) {
            Some(7)
        } else if quoted("command") {
            Some(9)
        } else {
            None
        };
        let Some(head) = head else {
            continue;
        };
        let mut at = start + head;
        while input[at..]
            .chars()
            .next()
            .is_some_and(crate::transcript::jsstr::is_js_space)
        {
            at += input[at..].chars().next().map_or(0, char::len_utf8);
        }
        if bytes.get(at) != Some(&b':') {
            continue;
        }
        at += 1;
        while input[at..]
            .chars()
            .next()
            .is_some_and(crate::transcript::jsstr::is_js_space)
        {
            at += input[at..].chars().next().map_or(0, char::len_utf8);
        }
        if let Some(literal) = json_string_literal(input, at) {
            return Some(literal);
        }
    }
    None
}

/// The command a tool ran, dug out of whatever shape the harness recorded it in.
fn command_text(input: &Value) -> String {
    match input {
        Value::String(text) => {
            let trimmed = js_trim(text);
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                // Freeform Codex exec input is JavaScript, not necessarily JSON.
                if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                    return command_text(&parsed);
                }
            }
            if let Some(literal) = embedded_command_literal(text) {
                // Keep the freeform source as the honest preview when it is not JSON.
                if let Ok(Value::String(parsed)) = serde_json::from_str::<Value>(literal) {
                    return parsed;
                }
            }
            text.clone()
        }
        Value::Object(entries) => {
            for key in ["command", "cmd", "script"] {
                match entries.get(key) {
                    Some(Value::String(text)) => return text.clone(),
                    Some(Value::Null) | None => continue,
                    Some(_) => return raw_preview(input),
                }
            }
            raw_preview(input)
        }
        Value::Null => String::new(),
        Value::Array(_) => raw_preview(input),
        other => primitive_text(other),
    }
}

/// The whole command a shell tool ran, for its open row's "Command" block: `command`, `cmd` or
/// `script` (an argv array joined), or Codex's freeform input. `None` for any other shape, which
/// keeps its full arguments.
pub fn command_detail(input: &Value) -> Option<String> {
    match input {
        Value::Object(entries) => ["command", "cmd", "script"]
            .iter()
            .find_map(|key| match entries.get(*key) {
                Some(Value::String(text)) if !text.trim().is_empty() => Some(text.clone()),
                Some(Value::Array(parts)) if !parts.is_empty() => parts
                    .iter()
                    .map(|part| part.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()
                    .map(|parts| parts.join(" ")),
                _ => None,
            }),
        Value::String(_) => Some(command_text(input)).filter(|text| !text.trim().is_empty()),
        _ => None,
    }
}

/// First three non-empty command lines, flattened into the compact tool row.
pub fn summarize_command_input(input: &Value) -> String {
    let text = command_text(input);
    let lines: Vec<&str> = split_newlines(&text)
        .into_iter()
        .map(js_trim)
        .filter(|line| !line.is_empty())
        .collect();
    let preview = lines
        .iter()
        .take(MAX_COMMAND_PREVIEW_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    if lines.len() > MAX_COMMAND_PREVIEW_LINES {
        format!("{preview}\u{2026}")
    } else {
        preview
    }
}

/// Full detail for the expanded view.
pub fn format_tool_input(input: &Value) -> String {
    match input {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(_) | Value::Number(_) => primitive_text(input),
        _ => stringify_pretty(input, 2),
    }
}

fn non_empty_string(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::String(text)) if !text.is_empty() => Some(text),
        _ => None,
    }
}

/// The file a tool call names, under any of the four spellings the harnesses use.
pub fn tool_file_path(input: &Value) -> Option<&str> {
    let Value::Object(entries) = input else {
        return None;
    };
    non_empty_string(entries.get("file_path"))
        .or_else(|| non_empty_string(entries.get("filePath")))
        .or_else(|| non_empty_string(entries.get("path")))
        .or_else(|| non_empty_string(entries.get("notebook_path")))
}
