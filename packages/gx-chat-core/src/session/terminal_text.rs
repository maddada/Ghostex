//! The text rules the terminal status machine matches on.
//!
//! Ported from `packages/core-ui/chat/session-chat-terminal-status.ts`. No regex dependency: every
//! JavaScript pattern is hand-rolled with the original quoted beside it.

use ghostex_gx_protocol::{ChatBlock, ChatMessage};

use crate::session::text::{collapse_whitespace, is_js_space};

/// `sessionChatTerminalStatusText`.
///
/// Text as the terminal would paint it: markdown decoration gone, whitespace collapsed, and any
/// trailing ellipsis dropped so a status the agent itself truncated still counts as a prefix of the
/// sentence it was cut from.
///
/// `.replace(/[*`_~]/g, '').replace(/\s+/g, ' ').replace(/(?:…|\.{3})\s*$/u, '').trim()`
pub fn terminal_status_text(text: &str) -> String {
    let undecorated: String = text
        .chars()
        .filter(|value| !matches!(value, '*' | '`' | '_' | '~'))
        .collect();
    let collapsed = collapse_whitespace(&undecorated);
    strip_trailing_ellipsis(&collapsed)
        .trim_matches(is_js_space)
        .to_string()
}

/// `/(?:…|\.{3})\s*$/u` removed. The whitespace tail is already gone after the collapse, so only
/// the ellipsis itself is left to strip.
pub fn strip_trailing_ellipsis(text: &str) -> String {
    let trimmed = text.trim_end_matches(is_js_space);
    if let Some(rest) = trimmed.strip_suffix('\u{2026}') {
        return rest.to_string();
    }
    if let Some(rest) = trimmed.strip_suffix("...") {
        return rest.to_string();
    }
    trimmed.to_string()
}

/// Whether the text ends in an ellipsis, `/(?:…|\.{3})$/`.
pub fn ends_with_ellipsis(text: &str) -> bool {
    text.ends_with('\u{2026}') || text.ends_with("...")
}

/// A message's text blocks joined the way the status rules compare them.
pub fn joined_text(message: &ChatMessage, separator: &str) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(separator)
}

/// The normalized text a transient row is matched by.
pub fn message_text(message: &ChatMessage) -> String {
    terminal_status_text(&joined_text(message, "\n\n"))
}

/// `isSessionChatCompletedToolSummary`: the agent's count-only completed-tool label, not a
/// description of a running tool.
///
/// `text.trim().replace(/\s+/g, ' ').replace(/[.!]$/, '').split(/,\s*(?:and\s+)?|\s+and\s+/i)`, then
/// every part against
/// `/^(?:searched for \d+ patterns?|ran \d+ shell commands?|read \d+ files?|listed \d+ (?:directories|directory))$/i`.
pub fn is_completed_tool_summary(text: &str) -> bool {
    let collapsed = collapse_whitespace(text);
    let body = match collapsed.chars().last() {
        Some('.') | Some('!') => collapsed[..collapsed.len() - 1].to_string(),
        _ => collapsed,
    };
    let parts = split_summary_parts(&body);
    !parts.is_empty() && parts.iter().all(|part| is_summary_part(part))
}

/// A painted shell-command row (`Bash(cd … && …)`, `PowerShell(…)`): the agent gave the call no
/// description, so the only label is the raw command.
///
/// CDXC:SessionChat 2026-09-24 DECISION: User: Claude chat sessions do not show the Bash/command status cards; a card whose label is the tool's description ("Running server tests, …") still shows.
pub fn is_shell_command_tool_label(text: &str) -> bool {
    let text = text.trim_start_matches(is_js_space);
    ["Bash(", "PowerShell("]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

/// `split(/,\s*(?:and\s+)?|\s+and\s+/i)` over an already space-collapsed string.
fn split_summary_parts(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == ',' {
            let mut next = index + 1;
            while next < chars.len() && is_js_space(chars[next]) {
                next += 1;
            }
            if let Some(after) = word_and_at(&chars, next) {
                index = after;
            } else {
                index = next;
            }
            parts.push(std::mem::take(&mut current));
            continue;
        }
        if is_js_space(chars[index]) {
            let mut next = index;
            while next < chars.len() && is_js_space(chars[next]) {
                next += 1;
            }
            if let Some(after) = word_and_at(&chars, next) {
                index = after;
                parts.push(std::mem::take(&mut current));
                continue;
            }
        }
        current.push(chars[index]);
        index += 1;
    }
    parts.push(current);
    parts
}

/// The index after a case-insensitive `and` plus at least one space starting at `at`, if there is
/// one. This is the `and\s+` half of both alternatives.
fn word_and_at(chars: &[char], at: usize) -> Option<usize> {
    if at + 3 > chars.len() {
        return None;
    }
    let word: String = chars[at..at + 3].iter().collect();
    if !word.eq_ignore_ascii_case("and") {
        return None;
    }
    let mut next = at + 3;
    if next >= chars.len() || !is_js_space(chars[next]) {
        return None;
    }
    while next < chars.len() && is_js_space(chars[next]) {
        next += 1;
    }
    Some(next)
}

/// One `searched for 3 patterns` style clause.
fn is_summary_part(part: &str) -> bool {
    let lower = part.to_lowercase();
    for (prefix, suffixes) in [
        ("searched for ", &["pattern", "patterns"][..]),
        ("ran ", &["shell command", "shell commands"][..]),
        ("read ", &["file", "files"][..]),
        ("listed ", &["directories", "directory"][..]),
    ] {
        let Some(rest) = lower.strip_prefix(prefix) else {
            continue;
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            continue;
        }
        let tail = &rest[digits.len()..];
        let Some(tail) = tail.strip_prefix(' ') else {
            continue;
        };
        if suffixes.contains(&tail) {
            return true;
        }
    }
    false
}

/// `value.replace(/\s+/g, ' ').trim()`, the comparison form for a painted tool row.
pub fn normalized_tool_text(value: &str) -> String {
    collapse_whitespace(value)
}
