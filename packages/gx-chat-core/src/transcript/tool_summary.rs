//! Tool input previews and the expanded tool body.
//!
//! Ported from `packages/core-ui/chat/session-chat-tool-summary.ts`.

use serde_json::Value;

use crate::transcript::json_text::{stringify, stringify_pretty};
use crate::transcript::jsstr::{
    collapse_whitespace, js_trim, js_trim_end, split_newlines, utf16_len, utf16_take,
};

pub const MAX_PREVIEW_LENGTH: usize = 80;
pub const MAX_PREVIEW_STRING_INPUT: usize = 160;
pub const MAX_COMMAND_PREVIEW_LINES: usize = 3;

/// CDXC:SessionChat 2026-09-06 DECISION:
/// User: show an ellipsis at the end of tool-call text when it is truncated.
pub fn truncate_tool_preview(text: &str, max_length: usize) -> String {
    if utf16_len(text) <= max_length {
        return text.to_string();
    }
    format!("{}\u{2026}", js_trim_end(utf16_take(text, max_length - 1)))
}

/// `String(value)` for the primitives a tool input can hold.
fn primitive_text(value: &Value) -> String {
    match value {
        Value::Bool(flag) => flag.to_string(),
        Value::Number(_) => stringify(value),
        _ => String::new(),
    }
}

/// CDXC:SessionChat 2026-11-04 DECISION: User, of the working bar above the composer reading `Artifact: {"action":"quickstart","intent":"other","design_systems":false}`: "please fix so we don't show this weird stuff again". A call's arguments never preview as JSON, braces or a dump: an object previews as the one value that reads as a summary of it ([`tool_input_gist`]), a list of words as those words, and anything else as nothing, so the row or bar shows the tool's name alone. Opening a tool row still shows every argument (`format_tool_input`). Supersedes the bounded JSON preview ported from `session-chat-tool-summary.ts`.
fn raw_preview(input: &Value) -> String {
    match input {
        Value::Null => String::new(),
        Value::String(text) => {
            let trimmed = js_trim(text);
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                    return raw_preview(&parsed);
                }
            }
            text.clone()
        }
        Value::Object(_) => tool_input_gist(input).unwrap_or_default(),
        Value::Array(items) => items
            .iter()
            .map(|item| item.as_str().map(js_trim).filter(|word| !word.is_empty()))
            .collect::<Option<Vec<_>>>()
            .map(|words| words.join(" "))
            .unwrap_or_default(),
        other => primitive_text(other),
    }
}

/// How long a gist may run before it is cut.
const MAX_GIST_LENGTH: usize = 60;

/// The keys whose value reads as a summary of a call, best first. The last few name what kind of
/// thing the call does (`"action": "quickstart"`), written as words.
const GIST_KEYS: &[&str] = &[
    "title",
    "description",
    "summary",
    "subject",
    "name",
    "query",
    "q",
    "pattern",
    "url",
    "file_path",
    "filePath",
    "path",
    "notebook_path",
    "target_file",
    "command",
    "cmd",
    "prompt",
    "message",
    "text",
    "action",
    "operation",
    "op",
    "mode",
];

/// Keys from [`GIST_KEYS`] whose value is a code word (`quick_start`) read as words (`quick start`).
const GIST_WORD_KEYS: &[&str] = &["action", "operation", "op", "mode"];

/// The one short human value in a call's arguments that reads as a summary of the call: its title,
/// description, query, path, command or action, on one line and cut short. `None` when no
/// argument reads that way (a flag, a number, a nested object, a JSON blob).
pub fn tool_input_gist(input: &Value) -> Option<String> {
    let Value::Object(entries) = input else {
        return None;
    };
    GIST_KEYS.iter().find_map(|key| {
        let Some(Value::String(text)) = entries.get(*key) else {
            return None;
        };
        let line = split_newlines(text)
            .into_iter()
            .map(js_trim)
            .find(|line| !line.is_empty())?;
        if line.starts_with('{') || line.starts_with('[') {
            return None;
        }
        let line = if GIST_WORD_KEYS.contains(key) {
            line.replace(['_', '-'], " ")
        } else {
            line.to_string()
        };
        let collapsed = js_trim(&collapse_whitespace(&line)).to_string();
        (!collapsed.is_empty()).then(|| truncate_tool_preview(&collapsed, MAX_GIST_LENGTH))
    })
}

/// A tool's name in plain words, for a line that has nothing better to name the call by:
/// `Artifact`, `ExitPlanMode` as "Exit plan mode", `apply_patch` as "Apply patch".
pub fn tool_display_name(name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for part in name
        .split(['_', '-', '.', ' '])
        .filter(|part| !part.is_empty())
    {
        let chars: Vec<char> = part.chars().collect();
        let mut word = String::new();
        for (index, character) in chars.iter().enumerate() {
            // A capital starts a word after a small letter or a digit ("planMode"), and before a
            // small letter after capitals ("HTTPServer" as "HTTP Server").
            let boundary = index > 0
                && character.is_uppercase()
                && (!chars[index - 1].is_uppercase()
                    || chars.get(index + 1).is_some_and(|next| next.is_lowercase()));
            if boundary && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            word.push(*character);
        }
        if !word.is_empty() {
            words.push(word);
        }
    }
    words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let acronym = word.chars().count() > 1 && word.chars().all(|c| !c.is_lowercase());
            if acronym {
                word.clone()
            } else if index == 0 {
                let mut chars = word.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            } else {
                word.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// CDXC:SessionChat 2026-09-26 WHY: a tool row used to preview its whole argument object as JSON, so a Read of a file under a long temporary folder showed `{"file_path":"/private/tmp/claude-501/…` with the file name cut off. A row now previews the argument that names the call: the search pattern or query, the URL, the file (relative to the session's folder, or `~/…`), the task's description, the skill, or the stopped task; a tool with none of these previews its gist (`tool_input_gist`), never its JSON.
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

/// A string literal starting at `at`, as `("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')` reads it.
pub(crate) fn json_string_literal(text: &str, at: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let quote = match bytes.get(at) {
        Some(&quote @ (b'"' | b'\'')) => quote,
        _ => return None,
    };
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
            byte if byte == quote => return Some(&text[at..index + 1]),
            _ => index += 1,
        }
    }
    None
}

/// A string literal's value. Codex writes `cmd: '…'` as often as `cmd: "…"`, and a single-quoted
/// literal is not JSON: its `\'` becomes `'` and a bare `"` is escaped before JSON reads it.
pub(crate) fn js_string_value(literal: &str) -> Option<String> {
    let Some(inner) = literal.strip_prefix('\'') else {
        return serde_json::from_str::<String>(literal).ok();
    };
    let inner = inner.strip_suffix('\'')?;
    let mut json = String::with_capacity(inner.len() + 2);
    json.push('"');
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => match characters.next()? {
                '\'' => json.push('\''),
                escaped => {
                    json.push('\\');
                    json.push(escaped);
                }
            },
            '"' => json.push_str("\\\""),
            other => json.push(other),
        }
    }
    json.push('"');
    serde_json::from_str(&json).ok()
}

/// Every match of `(?:\bcmd|["']cmd["']|\bcommand|["']command["'])\s*:\s*("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')` with the `s` flag.
///
/// Returns the JSON string literals the commands hide behind, still quoted, in source order. Codex's code mode batches several `tools.exec_command({cmd})` calls into one script (`Promise.allSettled([...])`), so the first match alone hides the rest.
fn embedded_command_literals(input: &str) -> Vec<&str> {
    key_literals(input, &["cmd", "command"])
}

/// Every string literal written as the value of one of `keys` (`key: "…"`, `"key": '…'`) in a
/// script or JSON-ish text, still quoted, in source order. A key matches only as a whole word.
pub(crate) fn key_literals<'a>(input: &'a str, keys: &[&str]) -> Vec<&'a str> {
    let bytes = input.as_bytes();
    let mut literals = Vec::new();
    let mut resume = 0;
    for start in 0..input.len() {
        if start < resume || !input.is_char_boundary(start) {
            continue;
        }
        let rest = &input[start..];
        let head = keys.iter().find_map(|word| {
            let length = word.len() + 2;
            let quoted = rest.len() >= length
                && matches!(bytes[start], b'"' | b'\'')
                && matches!(bytes[start + length - 1], b'"' | b'\'')
                && rest[1..word.len() + 1] == **word;
            if quoted {
                Some(length)
            } else if rest.starts_with(word)
                && at_word_boundary(input, start)
                && !rest[word.len()..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_alphanumeric() || next == '_')
            {
                Some(word.len())
            } else {
                None
            }
        });
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
            resume = at + literal.len();
            literals.push(literal);
        }
    }
    literals
}

/// The commands a freeform Codex script runs, parsed out of their string literals.
fn embedded_commands(text: &str) -> Vec<String> {
    embedded_command_literals(text)
        .into_iter()
        .filter_map(js_string_value)
        .collect()
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
            // Keep the freeform source as the honest preview when no command literal parses.
            let commands = embedded_commands(text);
            if !commands.is_empty() {
                return commands.join("\n");
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
    // A script that runs several commands names each one, rather than running them together as one line.
    if let Value::String(text) = input {
        let trimmed = js_trim(text);
        if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
            let commands = embedded_commands(text);
            if commands.len() > 1 {
                return commands
                    .iter()
                    .map(|command| summarize_command_input(&Value::String(command.clone())))
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ");
            }
        }
    }
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

/// The one line the agent wrote to say what a call does (Claude's and OpenCode's
/// `description`), which a tool row shows in place of the tool's name and its command preview in
/// every mode; empty when the call has none, and opening the row still shows the full command.
///
/// CDXC:SessionChat 2026-10-14 DECISION: User: "i dont want any of the modes to show this (just commands like this)" and "we should always show this (command description) -- even when simple mode is disabled". A described call's row reads the agent's description in Simple and verbose mode alike, with no command preview beside it; a call with no description keeps its tool name and preview. Then: "please also make sure the description of the tool call or command that we show isn't bold, should be same font style as normal agent message just 20% less bright oclor", so the renderers draw a description in the reply prose's font, size and normal weight, in the prose colour at 80%. Supersedes the 2026-10-13 rule that showed the description in Simple mode only.
pub fn tool_description(input: &Value) -> String {
    let Value::Object(entries) = input else {
        return String::new();
    };
    // OMP writes the same one line as `i` ("Reading project README").
    let Some(description) =
        non_empty_string(entries.get("description")).or_else(|| non_empty_string(entries.get("i")))
    else {
        return String::new();
    };
    let collapsed = js_trim(&collapse_whitespace(description)).to_string();
    truncate_tool_preview(&collapsed, MAX_PREVIEW_STRING_INPUT)
}

fn non_empty_string(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::String(text)) if !text.is_empty() => Some(text),
        _ => None,
    }
}

/// The file a tool call names, under any of the spellings the harnesses use: Claude's `file_path`,
/// OpenCode's `filePath`, Pi's and Hermes's `path`, Grok's `target_file` and Antigravity's `Path`.
pub fn tool_file_path(input: &Value) -> Option<&str> {
    let Value::Object(entries) = input else {
        return None;
    };
    [
        "file_path",
        "filePath",
        "path",
        "notebook_path",
        "target_file",
        "Path",
        "AbsolutePath",
    ]
    .iter()
    .find_map(|key| non_empty_string(entries.get(*key)))
}
