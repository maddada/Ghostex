//! Which tool calls are file writes, and what they changed.
//!
//! Ported from `packages/core-ui/chat/session-chat-file-changes.ts`.

use ghostex_gx_protocol::ChatBlock;
use serde_json::Value;

use crate::transcript::diff::{DiffKind, DiffLine};
use crate::transcript::jsstr::{ascii_lower, js_trim_start, split_newlines};
use crate::transcript::tool_fold::pair_tool_blocks;
use crate::transcript::tool_summary::tool_file_path;

/// One write or edit a turn made.
#[derive(Clone, Debug, PartialEq)]
pub struct FileChange<'a> {
    pub path: String,
    /// `Write`, `Edit` or `Delete`.
    pub action: &'static str,
    pub lines: Vec<DiffLine>,
    pub result: Option<&'a ChatBlock>,
}

impl FileChange<'_> {
    pub fn result_is_error(&self) -> bool {
        matches!(
            self.result,
            Some(ChatBlock::ToolResult {
                is_error: Some(true),
                ..
            })
        )
    }

    pub fn result_output(&self) -> &str {
        match self.result {
            Some(ChatBlock::ToolResult { output, .. }) => output,
            _ => "",
        }
    }
}

fn code_lines(text: &str, kind: DiffKind) -> Vec<DiffLine> {
    let mut lines: Vec<&str> = split_newlines(text);
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
        .into_iter()
        .map(|line| DiffLine::new(kind, line))
        .collect()
}

/// The changes a Codex-style `*** Begin Patch` body describes.
fn patch_changes<'a>(patch: &str) -> Vec<FileChange<'a>> {
    if !js_trim_start(patch).starts_with("*** Begin Patch") {
        return Vec::new();
    }
    let mut changes: Vec<FileChange<'a>> = Vec::new();
    let mut open = false;
    for line in split_newlines(patch) {
        if let Some(rest) = line.strip_prefix("*** ") {
            if let Some((verb, path)) = rest.split_once(" File: ") {
                let action = match verb {
                    "Add" => Some("Write"),
                    "Delete" => Some("Delete"),
                    "Update" => Some("Edit"),
                    _ => None,
                };
                if let Some(action) = action {
                    changes.push(FileChange {
                        path: path.to_string(),
                        action,
                        lines: Vec::new(),
                        result: None,
                    });
                    open = true;
                    continue;
                }
            }
        }
        if !open {
            if line == "*** End Patch" {
                open = false;
            }
            continue;
        }
        let current = changes
            .last_mut()
            .expect("an open patch has a current file");
        if let Some(path) = line.strip_prefix("*** Move to: ") {
            current.path = path.to_string();
        } else if line.starts_with(['+', '-', ' ']) {
            let kind = match line.as_bytes()[0] {
                b'+' => DiffKind::Add,
                b'-' => DiffKind::Del,
                _ => DiffKind::Context,
            };
            current.lines.push(DiffLine::new(kind, &line[1..]));
        } else if line.starts_with("@@") {
            current.lines.push(DiffLine::new(DiffKind::Meta, line));
        } else if line == "*** End Patch" {
            open = false;
        }
    }
    changes
}

/// The trailing segment of a dotted tool name, lowercased: `mcp.fs.write` is a `write`.
fn tool_kind(name: &str) -> String {
    ascii_lower(name.split('.').next_back().unwrap_or(name))
}

/// `\btools\.apply_patch\(\s*("(?:\\.|[^"\\])*")\s*\)`, every match.
fn embedded_apply_patch_literals(source: &str) -> Vec<&str> {
    const MARKER: &str = "tools.apply_patch(";
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(at) = source[cursor..].find(MARKER) {
        let start = cursor + at;
        cursor = start + MARKER.len();
        let before = source[..start].chars().next_back();
        if before.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_') {
            continue;
        }
        let mut index = cursor;
        while source[index..]
            .chars()
            .next()
            .is_some_and(crate::transcript::jsstr::is_js_space)
        {
            index += source[index..].chars().next().map_or(0, char::len_utf8);
        }
        let Some(literal) = json_string_literal(source, index) else {
            continue;
        };
        let mut after = index + literal.len();
        while source[after..]
            .chars()
            .next()
            .is_some_and(crate::transcript::jsstr::is_js_space)
        {
            after += source[after..].chars().next().map_or(0, char::len_utf8);
        }
        if source[after..].starts_with(')') {
            found.push(literal);
            cursor = after + 1;
        }
    }
    found
}

fn json_string_literal(text: &str, at: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    let mut index = at + 1;
    while index < text.len() {
        match bytes[index] {
            b'\\' => {
                index += 1;
                while index < text.len() && !text.is_char_boundary(index + 1) {
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

/// The tool argument, with a JSON object that arrived as a string parsed back.
///
/// A streaming JSON argument may still be incomplete, in which case it stays a string.
fn decoded_input(input: &Value) -> Value {
    if let Value::String(text) = input {
        if js_trim_start(text).starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                return parsed;
            }
        }
    }
    input.clone()
}

fn edit_lines(edit: &Value) -> Option<Vec<DiffLine>> {
    let entries = edit.as_object()?;
    let pick = |keys: &[&str]| -> Option<&Value> {
        keys.iter().find_map(|key| match entries.get(*key) {
            Some(Value::Null) | None => None,
            Some(value) => Some(value),
        })
    };
    let before = pick(&["old_string", "oldString", "old"]);
    let after = pick(&["new_string", "newString", "new", "content", "file_text"]);
    let Some(Value::String(after)) = after else {
        return None;
    };
    let mut lines = Vec::new();
    if let Some(Value::String(before)) = before {
        lines.extend(code_lines(before, DiffKind::Del));
    }
    lines.extend(code_lines(after, DiffKind::Add));
    Some(lines)
}

fn file_changes<'a>(name: &str, input: &Value) -> Vec<FileChange<'a>> {
    let kind = tool_kind(name);
    let input = decoded_input(input);
    if kind == "apply_patch" {
        let patch = match &input {
            Value::String(text) => Some(text.clone()),
            Value::Object(entries) => entries
                .get("patch")
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        };
        return patch.map(|patch| patch_changes(&patch)).unwrap_or_default();
    }
    // Codex's exec wrapper records literal apply_patch arguments inside JavaScript. Decode only
    // JSON string literals; never execute transcript source.
    if kind == "exec" {
        if let Value::String(source) = &input {
            return embedded_apply_patch_literals(source)
                .into_iter()
                .flat_map(|literal| match serde_json::from_str::<Value>(literal) {
                    Ok(Value::String(patch)) => patch_changes(&patch),
                    _ => Vec::new(),
                })
                .collect();
        }
        return Vec::new();
    }
    if !matches!(
        kind.as_str(),
        "write" | "edit" | "multiedit" | "str_replace"
    ) || !input.is_object()
    {
        return Vec::new();
    }
    let Some(path) = tool_file_path(&input) else {
        return Vec::new();
    };
    let path = path.to_string();
    let edits: Vec<Value> = match (kind.as_str(), input.get("edits")) {
        ("multiedit", Some(Value::Array(items))) => items.clone(),
        _ => vec![input.clone()],
    };
    let mut lines = Vec::new();
    for edit in &edits {
        if !edit.is_object() {
            continue;
        }
        match edit_lines(edit) {
            None => return Vec::new(),
            Some(more) => lines.extend(more),
        }
    }
    vec![FileChange {
        path,
        action: if kind == "write" { "Write" } else { "Edit" },
        lines,
        result: None,
    }]
}

/// The file changes and the remaining tool blocks of one message.
///
/// CDXC:SessionChat 2026-09-09 DECISION:
/// User: show file writes and code edits at the top level, outside other tools, with the filename
/// above a seven-line preview that expands and collapses on click. Pair before extracting so
/// removing a write cannot attach its result to the next tool.
pub fn split_file_changes<'a>(
    blocks: impl IntoIterator<Item = &'a ChatBlock>,
) -> (Vec<&'a ChatBlock>, Vec<FileChange<'a>>) {
    let blocks: Vec<&'a ChatBlock> = blocks.into_iter().collect();
    let mut tools: Vec<&'a ChatBlock> = Vec::new();
    let mut changes: Vec<FileChange<'a>> = Vec::new();
    for pair in pair_tool_blocks(blocks.iter().copied()) {
        let files = match pair.call {
            Some(ChatBlock::ToolCall { name, input, .. }) => file_changes(name, input),
            _ => Vec::new(),
        };
        // Deliberately case-sensitive, unlike the classification above: the TypeScript retains a
        // raw `exec` wrapper's activity without lowercasing the name first.
        let is_exec = pair
            .call_name()
            .and_then(|name| name.split('.').next_back())
            == Some("exec");
        changes.extend(files.iter().cloned().map(|file| FileChange {
            result: pair.result,
            ..file
        }));
        // An exec wrapper can also run unrelated commands, so retain its raw activity.
        if files.is_empty() || is_exec {
            if let Some(call) = pair.call {
                tools.push(call);
            }
            if let Some(result) = pair.result {
                tools.push(result);
            }
        }
    }
    (tools, changes)
}
